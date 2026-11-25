//! Antigravity CLI's Google OAuth flow uses a hosted redirect that gives the user a code.
//! See docs/agent-cli-reference.md, "Antigravity CLI", "Sign-in".

use std::future::Future;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::pin::Pin;
use std::time::Duration;

use tokio::sync::Notify;

use crate::accounts::background_cli::TerminalProcess;
use crate::accounts::login::flows::{Flow, LoginFlow, Outcome, Progress};
use crate::platform::term::Size;
use crate::protocol::{LoginAnswer, LoginAnswerKind, LoginQuestion};
use crate::providers::antigravity::APP_DATA_DIR;

pub(super) struct AntigravityLogin;

const CODE_QUESTION: &str = "google_authorization_code";
const LOGIN_MENU: &str = "Select login method:";
const GOOGLE_OAUTH: &str = "Google OAuth";
const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/";
const OAUTH_LOG_SUCCESS: &str = "OAuth: authenticated successfully as ";
const OAUTH_LOG_TAIL: u64 = 64 * 1024;
// The OAuth URL occupies one terminal row.
const LOGIN_SIZE: Size = Size {
    rows: 50,
    cols: 1024,
};

impl LoginFlow for AntigravityLogin {
    fn run<'a>(
        &'a self,
        flow: &'a Flow,
        progress: &'a Progress,
        cancel: &'a Notify,
    ) -> Pin<Box<dyn Future<Output = Outcome> + Send + 'a>> {
        Box::pin(sign_in(flow, progress, cancel))
    }
}

async fn sign_in(flow: &Flow, progress: &Progress, cancel: &Notify) -> Outcome {
    let account =
        crate::adapter::account_dir(flow.agent, flow.config_dir.as_deref(), &flow.shell_env);
    let home = match super::home::prepare_login_home(flow.scratch_dir(), &account) {
        Ok(home) => home,
        Err(err) => {
            return flow.failed(&format!(
                "the sign-in directory could not be prepared: {err}"
            ))
        }
    };
    let flag = super::home::directory_argument(&home);
    let mut command = match flow.command(&[&flag]) {
        Ok(command) => command,
        Err(outcome) => return outcome,
    };
    command
        .current_dir(flow.scratch_dir())
        .env("AGY_CLI_DISABLE_AUTO_UPDATE", "true");
    let mut process = match TerminalProcess::spawn_sized(&command, flow.agent.label(), LOGIN_SIZE) {
        Ok(process) => process,
        Err(err) => return flow.failed(&format!("it could not be started: {err}")),
    };

    let outcome = exchange(flow, progress, cancel, &mut process, &home).await;
    process.stop().await;
    outcome
}

async fn exchange(
    flow: &Flow,
    progress: &Progress,
    cancel: &Notify,
    process: &mut TerminalProcess,
    home: &Path,
) -> Outcome {
    let mut menu = false;
    let mut method_selected = false;
    let mut url_seen = false;
    let mut code_requested = false;
    let mut submitted = false;
    let log_success = wait_for_oauth_log(home);
    tokio::pin!(log_success);
    loop {
        let line = tokio::select! {
            () = cancel.notified() => return Outcome::Cancelled,
            _ = process.child.exited() => return flow.not_confirmed(),
            () = &mut log_success, if submitted => return Outcome::Succeeded,
            Some(line) = process.lines.recv() => line.text,
        };
        if line.contains("Authentication successful!") || line.starts_with("Logged in as ") {
            return Outcome::Succeeded;
        }
        if submitted {
            // The terminal may echo the answer. Only a known outcome claims it was accepted.
            if line.contains("Authentication failed") || line.contains("authorization failed") {
                return flow.not_confirmed();
            }
            continue;
        }
        if !method_selected && line.contains("Choose your color scheme:") {
            return flow.failed(
                "complete Antigravity CLI's first-run setup in its terminal, then try again",
            );
        }
        if !method_selected {
            menu |= line.contains(LOGIN_MENU);
            if menu && line.contains(GOOGLE_OAUTH) {
                // Google OAuth is the first option in the CLI's sign-in menu.
                if process.answer("").await.is_err() {
                    return flow.failed("the sign-in option could not be selected");
                }
                method_selected = true;
            }
        }
        if !url_seen {
            if let Some(url) = line.split_whitespace().find(|word| {
                word.strip_prefix(GOOGLE_AUTH_URL)
                    .is_some_and(|path| path.starts_with("v2/auth?") || path.starts_with("auth?"))
            }) {
                progress.waiting(Some(url.to_owned()), None, true);
                url_seen = true;
            }
        }
        let lower = line.to_ascii_lowercase();
        code_requested |= lower.contains("paste the authorization code")
            || lower.contains("enter the authorization code");
        if url_seen && code_requested {
            let mut question = LoginQuestion::new(
                CODE_QUESTION,
                "Paste the authorization code from the browser.",
                &[],
            );
            question.answer_kind = Some(LoginAnswerKind::SecretText);
            let answer = tokio::select! {
                () = cancel.notified() => return Outcome::Cancelled,
                _ = process.child.exited() => return flow.not_confirmed(),
                answer = progress.ask(question) => answer,
            };
            let Ok(LoginAnswer::SecretText { secret_text }) = answer else {
                return flow.failed("the login question did not receive text");
            };
            if secret_text.as_str().chars().any(char::is_whitespace) {
                return flow.failed("the authorization code must be one word");
            }
            if process.paste_secret(&secret_text).await.is_err() {
                return flow.failed("the authorization code could not be sent");
            }
            submitted = true;
        }
    }
}

/// Antigravity can complete OAuth without delivering a success banner to the
/// private terminal. Its private login home holds the confirmation in a dated log file.
async fn wait_for_oauth_log(home: &Path) {
    let logs = home.join(APP_DATA_DIR).join("log");
    loop {
        let logs = logs.clone();
        if tokio::task::spawn_blocking(move || oauth_confirmed_in(&logs))
            .await
            .unwrap_or(false)
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

fn oauth_confirmed_in(logs: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(logs) else {
        return false;
    };
    let newest = entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_file())
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with("cli-") && name.ends_with(".log"))
        })
        .max_by_key(std::fs::DirEntry::file_name);
    let Some(newest) = newest else {
        return false;
    };
    let Ok(mut file) = crate::hostfs::open_regular(&newest.path()) else {
        return false;
    };
    let Ok(metadata) = file.metadata() else {
        return false;
    };
    let offset = metadata.len().saturating_sub(OAUTH_LOG_TAIL);
    if file.seek(SeekFrom::Start(offset)).is_err() {
        return false;
    }
    let mut tail = Vec::new();
    if file.take(OAUTH_LOG_TAIL).read_to_end(&mut tail).is_err() {
        return false;
    }
    // The remainder of this line names the user. Only the completion marker is used.
    String::from_utf8_lossy(&tail).lines().any(|line| {
        line.split_once("] ")
            .is_some_and(|(_, message)| message.starts_with(OAUTH_LOG_SUCCESS))
    })
}

#[cfg(test)]
#[test]
fn oauth_completion_is_read_from_the_dated_log() {
    let scratch = crate::test_support::ScratchDir::new("antigravity-oauth-log");
    let log = scratch.join("cli-20000101_000000.log");
    std::fs::write(&log, "[I browser.go] OAuth authorization is waiting\r\n").unwrap();
    assert!(!oauth_confirmed_in(&scratch));
    std::fs::write(
        &log,
        "[I oauth.go] OAuth: authenticated successfully as example@example.invalid\r\n",
    )
    .unwrap();
    assert!(oauth_confirmed_in(&scratch));
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::accounts::login::{AccountLogins, Start};
    use crate::protocol::{AgentId, Event, LoginStatus, Request, RequestBody};
    use crate::test_support::{app_state, fake_agent, PATIENCE};

    #[tokio::test]
    async fn the_browser_code_reaches_the_login_pty_as_a_paste() {
        sign_in_with_completion(r#"echo 'Authentication successful!'"#).await;
    }

    #[tokio::test]
    async fn a_log_only_oauth_completion_finishes_the_login() {
        sign_in_with_completion(
            r#"logs="${1#--gemini_dir=}/antigravity-cli/log"
               mkdir -p "$logs"
               echo '[I oauth.go] OAuth: authenticated successfully as example@example.invalid' > "$logs/cli-20000101_000000.log""#,
        )
        .await;
    }

    async fn sign_in_with_completion(completion: &str) {
        let (state, dir) = app_state("antigravity-login");
        let agent = AgentId::ANTIGRAVITY;
        let script = format!(
            r#"echo 'Select login method:'
               echo '1. Google OAuth'
               IFS= read -r choice
               test -z "$choice" || exit 1
               echo 'https://accounts.google.com/o/oauth2/v2/auth?state=example'
               echo 'Paste the authorization code below:'
               IFS= read -r answer
               test "$answer" = "$(printf '\033[200~example-code\033[201~')" || exit 2
               {completion}
               IFS= read -r ignored"#,
        );
        let env = fake_agent(&dir, agent.descriptor().binary, &script);
        let mut events = state.subscribe();
        AccountLogins::start_for_test(
            &state,
            Start {
                agent,
                account: None,
                config_dir: None,
                resolved_dir: None,
            },
            env,
            dir.join("run"),
        )
        .unwrap();

        tokio::time::timeout(PATIENCE, async {
            loop {
                let Ok(Event::AccountLoginUpdated { login }) = events.recv().await else {
                    continue;
                };
                if let Some(question) = login.question {
                    assert_eq!(question.id, CODE_QUESTION);
                    assert_eq!(question.answer_kind, Some(LoginAnswerKind::SecretText));
                    assert!(login.authorization_url.is_some());
                    let request: Request = serde_json::from_str(
                        r#"{"type":"answer_account_login_question","agent":"antigravity","question":"google_authorization_code","answer":{"secret_text":"example-code"}}"#,
                    )
                    .unwrap();
                    let RequestBody::AnswerAccountLoginQuestion { answer, .. } = request.body else {
                        panic!("expected a login answer");
                    };
                    assert!(!format!("{answer:?}").contains("example-code"));
                    state.account_logins.answer(&state, agent, None, &question.id, answer).unwrap();
                } else if !matches!(login.status, LoginStatus::Starting | LoginStatus::Waiting) {
                    assert_eq!(login.status, LoginStatus::Succeeded);
                    assert!(!format!("{login:?}").contains("example-code"));
                    break;
                }
            }
        })
        .await
        .expect("the login ended");
    }
}
