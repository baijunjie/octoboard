//! Signing a GitHub Copilot CLI account in: `copilot login --device-code`, which prints one line
//! naming the page and the code, then "Waiting for authorization...", and ends with "Signed in
//! successfully as <login>." and exit status 0, having named the login in the directory's state
//! file. It is run on a terminal of its own ([`TerminalProcess`]): it writes the same two plain
//! lines there as on pipes, with no interface drawn. Measured in `docs/agent-cli-reference.md`,
//! "GitHub Copilot CLI".
//!
//! Copilot's code lasts 15 minutes. The flow declares a wait a little longer than that, so a code
//! nobody enters ends as Copilot's own "Device code expired before authorization completed",
//! which is then the reason the login reports.
//!
//! Where the system has no credential store Copilot asks, once the code is authorized, whether to
//! keep the token in plain text in its state file, but only where its stdin and stdout are both a
//! terminal; on pipes it takes that for "no". That is why the login runs on a terminal: the
//! question is put to the user ([`Progress::ask`]) and their answer written back as `y` or `n`.
//! Nothing answers it for the user, however long the answer takes. On a yes Copilot keeps the
//! token in `config.json` and the login ends as it always does; on a no Copilot signs the account
//! in without a token and exits, and the login fails with Copilot's own reason
//! ([`TOKEN_NOT_KEPT`]).
//!
//! A macOS home with no keychain is another case: the login there was seen to sit at "Waiting for
//! authorization..." after its code was authorized, until the code expired, most likely behind
//! the system's "Keychain Not Found" dialog (the cause was not measured). The declared wait ends
//! such a login too, with Copilot's expiry line.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use tokio::sync::Notify;

use crate::accounts::background_cli::TerminalProcess;
use crate::accounts::login::flows::{looks_like_device_code, Flow, LoginFlow, Outcome, Progress};
use crate::protocol::{Code, LoginAnswer, LoginFailure, LoginQuestion};
use crate::providers::copilot::{COPILOT_STATE_FILE, TOKEN_NOT_KEPT};

/// How long a device code lasts, as Copilot was measured to expire one.
const DEVICE_CODE_LIFE: Duration = Duration::from_secs(15 * 60);

/// How much longer than the code's life the daemon waits: the CLI polls GitHub at intervals, so
/// its own "expired" comes a little after the code's last moment.
const EXPIRY_MARGIN: Duration = Duration::from_secs(60);

/// The id of the question whether to keep the token in plain text, worded by the UI as
/// `provider.copilot.loginQuestion.keep_token_in_plain_text`. Its one param, `file`, is the path
/// of the file the token would be kept in ([`token_file`]), so the user knows which file would
/// hold it.
pub(super) const KEEP_TOKEN_IN_PLAIN_TEXT: &str = "keep_token_in_plain_text";

/// The phrase Copilot's question is told by: its whole prompt is `System keychain unavailable.
/// Store token in plaintext config file? (y/N) `, written in one piece by `readline`.
const PLAIN_TEXT_PROMPT: &str = "Store token in plaintext config file?";

/// How long Copilot is given to end by itself once told "no", before it is stopped.
const DECLINED_GRACE: Duration = Duration::from_secs(5);

pub(super) struct CopilotLogin;

impl LoginFlow for CopilotLogin {
    fn run<'a>(
        &'a self,
        flow: &'a Flow,
        progress: &'a Progress,
        cancel: &'a Notify,
    ) -> Pin<Box<dyn Future<Output = Outcome> + Send + 'a>> {
        Box::pin(sign_in(flow, progress, cancel))
    }

    fn timeout(&self) -> Option<Duration> {
        Some(DEVICE_CODE_LIFE + EXPIRY_MARGIN)
    }
}

async fn sign_in(flow: &Flow, progress: &Progress, cancel: &Notify) -> Outcome {
    let command = match flow.command(&["login", "--device-code"]) {
        Ok(command) => command,
        Err(outcome) => return outcome,
    };
    let mut process = match TerminalProcess::spawn(&command, flow.agent.label()) {
        Ok(process) => process,
        Err(err) => return flow.failed(&format!("it could not be started: {err}")),
    };
    let mut announced = false;
    let mut asked = false;
    let mut last_words = String::new();
    loop {
        let line = tokio::select! {
            () = cancel.notified() => {
                process.stop().await;
                return Outcome::Cancelled;
            }
            success = process.child.exited() => {
                return ended(flow, &mut process, last_words, success).await;
            }
            Some(line) = process.lines.recv() => line,
        };
        if !announced {
            if let Some((page, code)) = page_and_code_in(&line.text) {
                progress.waiting(Some(page), Some(code), false);
                announced = true;
            }
        }
        // The prompt is never the CLI's last word: the terminal echoes the answer on its line.
        if !line.text.contains(PLAIN_TEXT_PROMPT) {
            if let Some(words) = line.into_words() {
                last_words = words;
            }
            continue;
        }
        // Asked once: a terminal may hand the prompt over again.
        if asked {
            continue;
        }
        asked = true;
        let file = token_file(flow);
        let question = LoginQuestion::new(
            KEEP_TOKEN_IN_PLAIN_TEXT,
            "System keychain unavailable. Store token in plaintext config file?",
            &[("file", &file)],
        );
        let answer = tokio::select! {
            () = cancel.notified() => {
                process.stop().await;
                return Outcome::Cancelled;
            }
            // A CLI that gives up by itself leaves nothing to answer.
            success = process.child.exited() => {
                return ended(flow, &mut process, last_words, success).await;
            }
            answer = progress.ask(question) => answer,
        };
        match answer {
            Ok(LoginAnswer::Yes) => {
                if let Err(err) = process.answer("y").await {
                    process.stop().await;
                    return flow.failed(&format!("it could not be answered: {err}"));
                }
            }
            Ok(LoginAnswer::No) => return declined(flow, &mut process, cancel).await,
            _ => {
                process.stop().await;
                return flow.failed("the login question did not receive a choice");
            }
        }
    }
}

/// The file Copilot keeps the token in on a yes: `config.json` in the account's directory, by its
/// full path where the directory is known, else by its name alone, which is all that can be said.
fn token_file(flow: &Flow) -> String {
    match &flow.resolved_dir {
        Some(dir) => dir.join(COPILOT_STATE_FILE).to_string_lossy().into_owned(),
        None => COPILOT_STATE_FILE.to_string(),
    }
}

/// How the login ends once its CLI has exited by itself: signed in on status 0 where the account's
/// directory names a login (or is not known), else with what the CLI said last.
async fn ended(
    flow: &Flow,
    process: &mut TerminalProcess,
    mut last_words: String,
    success: Option<bool>,
) -> Outcome {
    process.last_words(&mut last_words).await;
    if success != Some(true) {
        return if last_words.is_empty() {
            flow.failed("it ended without signing in")
        } else {
            flow.failed(&last_words)
        };
    }
    match &flow.resolved_dir {
        Some(dir) if super::accounts::signed_in_login(dir).is_none() => flow.not_confirmed(),
        _ => Outcome::Succeeded,
    }
}

/// The user said no to keeping the token in plain text: Copilot is told so, which ends its login
/// with no token kept, and is stopped if it has not ended within [`DECLINED_GRACE`]. The login
/// fails with Copilot's own reason, whatever the CLI then says; a cancel meanwhile cancels it.
async fn declined(flow: &Flow, process: &mut TerminalProcess, cancel: &Notify) -> Outcome {
    let mut cancelled = false;
    if process.answer("n").await.is_ok() {
        tokio::select! {
            () = cancel.notified() => cancelled = true,
            _ = process.child.exited() => {}
            () = tokio::time::sleep(DECLINED_GRACE) => {}
        }
    }
    process.stop().await;
    if cancelled {
        return Outcome::Cancelled;
    }
    let agent = flow.agent.label();
    Outcome::Failed(LoginFailure::new(
        Code::of_provider(TOKEN_NOT_KEPT),
        format!("{agent} kept no token: storing it in plain text was declined"),
        &[("agent", agent)],
    ))
}

/// The page and the code of "To authenticate, visit <page> and enter code <code>": the line's
/// first `https://` word, and the word after "code" when it is two groups of four letters or
/// digits.
pub(super) fn page_and_code_in(line: &str) -> Option<(String, String)> {
    let mut words = line.split_whitespace();
    let page = words.find(|word| word.starts_with("https://"))?;
    let code = words.skip_while(|word| *word != "code").nth(1)?;
    looks_like_device_code(code).then(|| (page.to_string(), code.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "https://github.com/login/device";

    /// The line `copilot login --device-code` prints with no terminal, as the reference gives it.
    #[test]
    fn the_page_and_the_code_are_read_from_the_line_the_cli_prints() {
        for (line, code) in [
            (
                "To authenticate, visit https://github.com/login/device and enter code XXXX-XXXX",
                "XXXX-XXXX",
            ),
            (
                "To authenticate, visit https://github.com/login/device and enter code A1B2-C3D4",
                "A1B2-C3D4",
            ),
            (
                "  To authenticate, visit https://github.com/login/device and enter code 12AB-9Z0Y  ",
                "12AB-9Z0Y",
            ),
        ] {
            assert_eq!(
                page_and_code_in(line),
                Some((PAGE.to_string(), code.to_string())),
                "{line}"
            );
        }
    }

    /// The daemon waits longer than a device code lasts, so that Copilot's own expiry is what ends
    /// a login nobody completes.
    #[test]
    fn the_declared_wait_outlasts_the_device_code() {
        let wait = CopilotLogin.timeout().expect("a declared wait");
        assert!(wait > Duration::from_secs(15 * 60), "{wait:?}");
        assert!(wait <= Duration::from_secs(20 * 60), "{wait:?}");
    }

    #[test]
    fn the_other_lines_the_cli_prints_are_not_the_page_and_the_code() {
        for line in [
            "Waiting for authorization...",
            "Login failed: Error: Device code expired before authorization completed",
            "Login succeeded, but the token was not saved. Install a system keychain or rerun login and accept plaintext storage.",
            "Please use /login to sign in to use Copilot",
            "",
            // The page without a code, and a code without a page.
            "To authenticate, visit https://github.com/login/device",
            "To authenticate, visit https://github.com/login/device and enter code",
            "enter code ABCD-1234",
            // A word after "code" that is not a device code.
            "To authenticate, visit https://github.com/login/device and enter code ABCD-12345",
            "To authenticate, visit https://github.com/login/device and enter code ABCD1234",
            "To authenticate, visit https://github.com/login/device and enter code AB-CD-EF",
            "To authenticate, visit https://github.com/login/device and enter code abcd-12+4",
        ] {
            assert_eq!(page_and_code_in(line), None, "{line}");
        }
    }

    mod on_a_terminal {
        use std::sync::Arc;

        use tokio::sync::broadcast::Receiver;

        use super::*;
        use crate::accounts::login::{AccountLogins, Start};
        use crate::protocol::{AccountLogin, AgentId, Event, LoginStatus};
        use crate::state::AppState;
        use crate::test_support::{
            is_running, read_pid_file, ScratchDir, PATIENCE, UNPROMPTED_WINDOW,
        };

        /// The lines `copilot login --device-code` prints before it waits, as measured.
        #[cfg(unix)]
        const STARTS: &str = r#"echo $$ > "$HOME/pid"
echo 'To authenticate, visit https://github.com/login/device and enter code ABCD-1234'
echo 'Waiting for authorization...'
"#;

        /// What Copilot writes into `config.json` once it keeps the token in it: the token under
        /// `authTokens` beside the login's own keys. The token is made up.
        #[cfg(unix)]
        const KEEPS_THE_TOKEN: &str = r#"printf '%s\n' '{"authTokens":{"https://github.com:octocat":"FAKE-TOKEN-NOT-A-REAL-ONE"},"loggedInUsers":[{"host":"https://github.com","login":"octocat"}],"lastLoggedInUser":{"host":"https://github.com","login":"octocat"}}' > "$COPILOT_HOME/config.json"
echo 'Signed in successfully as octocat.'
exit 0
"#;

        const NOT_SAVED: &str = "Login succeeded, but the token was not saved. Install a system keychain or rerun login and accept plaintext storage.";

        /// A stand-in for Copilot on a host with no credential store: once the code is "authorized"
        /// it asks, only where stdin and stdout are both a terminal, whether to keep the token in
        /// plain text, in Copilot's own words, writes the answer it read to `$HOME/answer`, and
        /// keeps the token on `y` or `yes` in any case; anything else, pipes among them, keeps none.
        #[cfg(unix)]
        fn asks_on_a_terminal(after_asking: &str) -> String {
            format!(
                r#"{STARTS}if [ -t 0 ] && [ -t 1 ]; then
  printf 'System keychain unavailable. Store token in plaintext config file? (y/N) '
  {after_asking}
  printf '%s' "$answer" > "$HOME/answer"
  case "$(printf '%s' "$answer" | tr 'A-Z' 'a-z')" in
    y|yes)
{KEEPS_THE_TOKEN}    ;;
  esac
fi
echo '{NOT_SAVED}' >&2
exit 1
"#
            )
        }

        enum Scenario {
            Question,
            Keychain,
            Unconfirmed,
            GivesUp,
        }

        #[cfg(unix)]
        fn fake_copilot(
            bin: &std::path::Path,
            _home: &std::path::Path,
            scenario: Scenario,
        ) -> std::collections::HashMap<String, String> {
            let body = match scenario {
                Scenario::Question => asks_on_a_terminal("read answer"),
                Scenario::Keychain => format!(
                    "{STARTS}printf '%s\\n' '{{\"loggedInUsers\":[{{\"host\":\"https://github.com\",\"login\":\"octocat\"}}]}}' > \"$COPILOT_HOME/config.json\"\necho 'Signed in successfully as octocat.'\nexit 0\n"
                ),
                Scenario::Unconfirmed => format!("{STARTS}echo 'Signed in successfully as octocat.'\nexit 0\n"),
                Scenario::GivesUp => asks_on_a_terminal("sleep 2; echo 'Gave up waiting.'; exit 1"),
            };
            crate::test_support::fake_agent(bin, AgentId::COPILOT.descriptor().binary, &body)
        }

        #[cfg(windows)]
        fn fake_copilot(
            bin: &std::path::Path,
            home: &std::path::Path,
            scenario: Scenario,
        ) -> std::collections::HashMap<String, String> {
            use crate::test_support::{fake_native_agent, fixture::Steps};

            let login = "Signed in successfully as octocat.";
            let config = home.join("config.json");
            let prompt =
                "System keychain unavailable. Store token in plaintext config file? (y/N) ";
            let steps = Steps::new()
                .write_pid(&bin.join("pid"))
                .say("To authenticate, visit https://github.com/login/device and enter code ABCD-1234")
                .say("Waiting for authorization...");
            let steps = match scenario {
                Scenario::Question => steps.ask(
                    prompt,
                    Some(&bin.join("answer")),
                    Steps::new()
                        .write_file(&config, r#"{"authTokens":{"https://github.com:octocat":"FAKE-TOKEN-NOT-A-REAL-ONE"},"loggedInUsers":[{"host":"https://github.com","login":"octocat"}],"lastLoggedInUser":{"host":"https://github.com","login":"octocat"}}"#)
                        .say(login)
                        .exit(),
                    Steps::new().print(NOT_SAVED).fail(),
                    Steps::new().say(NOT_SAVED).fail(),
                ),
                Scenario::Keychain => steps
                    .write_file(&config, r#"{"loggedInUsers":[{"host":"https://github.com","login":"octocat"}]}"#)
                    .say(login)
                    .exit(),
                Scenario::Unconfirmed => steps.say(login).exit(),
                Scenario::GivesUp => steps
                    .print(prompt)
                    .sleep(2)
                    .print("\r\nGave up waiting.")
                    .fail(),
            };
            fake_native_agent(bin, AgentId::COPILOT.descriptor().binary, steps)
        }

        struct Login {
            state: Arc<AppState>,
            dir: ScratchDir,
            home: std::path::PathBuf,
            events: Receiver<Event>,
        }

        /// Starts the login of Copilot's default account, pinned to a directory of its own, over a
        /// stand-in CLI.
        fn start(name: &str, scenario: Scenario) -> Login {
            let (state, dir) = crate::test_support::app_state(name);
            let bin = dir.join("bin");
            std::fs::create_dir(&bin).unwrap();
            let home = dir.join("copilot-home");
            std::fs::create_dir(&home).unwrap();
            let env = fake_copilot(&bin, &home, scenario);
            let events = state.subscribe();
            let pinned = home.to_string_lossy().into_owned();
            AccountLogins::start_for_test(
                &state,
                Start {
                    agent: AgentId::COPILOT,
                    account: None,
                    config_dir: Some(pinned.clone()),
                    resolved_dir: Some(pinned),
                },
                env,
                dir.join("run"),
            )
            .unwrap();
            Login {
                state,
                dir,
                home,
                events,
            }
        }

        impl Login {
            async fn next(&mut self) -> AccountLogin {
                tokio::time::timeout(PATIENCE, async {
                    loop {
                        if let Ok(Event::AccountLoginUpdated { login }) = self.events.recv().await {
                            return login;
                        }
                    }
                })
                .await
                .expect("the login said nothing more")
            }

            /// Every update up to the one that carries a question, that one included.
            async fn until_asked(&mut self) -> Vec<AccountLogin> {
                let mut updates = Vec::new();
                loop {
                    let login = self.next().await;
                    let asked = login.question.is_some();
                    assert!(
                        asked
                            || matches!(login.status, LoginStatus::Starting | LoginStatus::Waiting),
                        "the login ended before it asked: {login:?}"
                    );
                    updates.push(login);
                    if asked {
                        return updates;
                    }
                }
            }

            /// Every update up to the one that ends the login, that one included.
            async fn until_ended(&mut self) -> Vec<AccountLogin> {
                let mut updates = Vec::new();
                loop {
                    let login = self.next().await;
                    let ended =
                        !matches!(login.status, LoginStatus::Starting | LoginStatus::Waiting);
                    updates.push(login);
                    if ended {
                        return updates;
                    }
                }
            }

            fn answer(&self, answer: LoginAnswer) -> anyhow::Result<()> {
                self.state.account_logins.answer(
                    &self.state,
                    AgentId::COPILOT,
                    None,
                    KEEP_TOKEN_IN_PLAIN_TEXT,
                    answer,
                )
            }

            /// What the stand-in read as its answer, if it read one.
            fn answer_read(&self) -> Option<String> {
                std::fs::read_to_string(self.dir.join("bin/answer")).ok()
            }

            fn token_kept(&self) -> bool {
                std::fs::read_to_string(self.home.join("config.json"))
                    .is_ok_and(|text| text.contains("authTokens"))
            }
        }

        /// The question Copilot asks only on a terminal reaches the user, after the page and the
        /// code, naming the file the token would be kept in; nothing answers it meanwhile, and the
        /// user's yes reaches Copilot, which keeps the token and signs the account in.
        #[tokio::test]
        async fn the_plain_text_question_reaches_the_user_and_a_yes_keeps_the_token() {
            let mut login = start("copilot-login-yes", Scenario::Question);
            let updates = login.until_asked().await;
            let asked = updates.last().unwrap();
            let question = asked.question.clone().unwrap();
            assert_eq!(question.id, KEEP_TOKEN_IN_PLAIN_TEXT);
            assert_eq!(
                question.params.get("file").map(String::as_str),
                Some(
                    login
                        .home
                        .join(COPILOT_STATE_FILE)
                        .to_string_lossy()
                        .as_ref()
                )
            );
            assert_eq!(asked.status, LoginStatus::Waiting);
            assert_eq!(asked.device_code.as_deref(), Some("ABCD-1234"));
            assert_eq!(
                asked.authorization_url.as_deref(),
                Some("https://github.com/login/device")
            );
            // The stand-in runs in a process of its own and says nothing until it is answered.
            tokio::time::sleep(UNPROMPTED_WINDOW).await;
            assert_eq!(
                login.answer_read(),
                None,
                "the question was answered for the user"
            );

            login.answer(LoginAnswer::Yes).unwrap();
            let ended = login.until_ended().await.pop().unwrap();
            assert_eq!(ended.status, LoginStatus::Succeeded, "{ended:?}");
            assert_eq!(login.answer_read().as_deref(), Some("y"));
            assert!(login.token_kept());
        }

        /// A no reaches Copilot, which keeps no token, and the login fails with Copilot's own
        /// reason, not with the line Copilot prints for it.
        #[tokio::test]
        async fn a_no_reaches_copilot_and_the_login_fails_with_its_own_reason() {
            let mut login = start("copilot-login-no", Scenario::Question);
            login.until_asked().await;

            login.answer(LoginAnswer::No).unwrap();
            let ended = login.until_ended().await.pop().unwrap();
            assert_eq!(ended.status, LoginStatus::Failed);
            assert_eq!(ended.question, None);
            let failure = ended.failure.unwrap();
            assert_eq!(failure.code, Code::of_provider(TOKEN_NOT_KEPT));
            assert_eq!(
                failure.params.get("agent").map(String::as_str),
                Some("GitHub Copilot CLI")
            );
            assert_eq!(login.answer_read().as_deref(), Some("n"));
            assert!(!login.token_kept());
        }

        /// Where the system keeps the token, Copilot asks nothing: the login signs the account in
        /// with no question put to the user.
        #[tokio::test]
        async fn a_login_whose_token_the_system_keeps_asks_nothing() {
            let mut login = start("copilot-login-keychain", Scenario::Keychain);
            let updates = login.until_ended().await;
            assert!(
                updates.iter().all(|login| login.question.is_none()),
                "{updates:?}"
            );
            assert_eq!(updates.last().unwrap().status, LoginStatus::Succeeded);
        }

        /// A Copilot that ends with status 0 without naming a login in the account's directory has
        /// not signed it in.
        #[tokio::test]
        async fn a_login_that_names_no_login_in_the_directory_is_not_confirmed() {
            let mut login = start("copilot-login-unconfirmed", Scenario::Unconfirmed);
            let ended = login.until_ended().await.pop().unwrap();
            assert_eq!(ended.status, LoginStatus::Failed);
            assert_eq!(
                ended.failure.unwrap().code,
                crate::protocol::login_failure_code::NOT_CONFIRMED
            );
        }

        /// Cancelling a login whose question is open ends Copilot, waiting at its prompt, before
        /// the cancellation is announced, and takes the question back.
        #[tokio::test]
        async fn cancelling_a_login_at_its_question_ends_copilot() {
            let mut login = start("copilot-login-cancel", Scenario::Question);
            login.until_asked().await;
            let pid = read_pid_file(&login.dir.join("bin/pid")).expect("the stand-in's pid");
            assert!(is_running(pid));

            login
                .state
                .account_logins
                .cancel(AgentId::COPILOT, None)
                .unwrap();
            let ended = login.until_ended().await.pop().unwrap();
            assert_eq!(ended.status, LoginStatus::Cancelled);
            assert_eq!(ended.question, None);
            assert!(!is_running(pid));
            assert_eq!(login.answer_read(), None);
            assert!(!login.token_kept());
        }

        /// A Copilot that gives up at its prompt by itself leaves no question to answer: the login
        /// ends with what Copilot said, the question taken back.
        #[tokio::test]
        async fn a_copilot_that_exits_while_its_question_is_open_ends_the_login() {
            let mut login = start("copilot-login-gone", Scenario::GivesUp);
            login.until_asked().await;

            let ended = login.until_ended().await.pop().unwrap();
            assert_eq!(ended.status, LoginStatus::Failed);
            assert_eq!(ended.question, None);
            assert_eq!(
                ended
                    .failure
                    .unwrap()
                    .params
                    .get("detail")
                    .map(String::as_str),
                Some("Gave up waiting.")
            );
        }
    }
}
