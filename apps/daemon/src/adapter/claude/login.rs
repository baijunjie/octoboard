//! Signing a Claude Code account in: `claude auth login` with `BROWSER` pointed at a script that
//! appends the URL it is given to a file the flow watches. That URL is the one whose loopback
//! callback finishes the login with nothing pasted. Done is exit 0, confirmed by `claude auth
//! status`. Measured in `docs/agent-cli-reference.md` ("Signing in a config directory without a
//! terminal").

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::Notify;

use crate::accounts::background_cli::Process;
use crate::accounts::login::flows::{last_line_of, Flow, LoginFlow, Outcome, Progress};
use crate::adapter::account_cli;
use crate::platform::fs as platform;
use crate::platform::shell;

/// How often the file Claude Code's `BROWSER` script writes is looked at.
const URL_POLL: Duration = Duration::from_millis(100);

/// How long `claude auth status` may take.
pub(super) const STATUS_TIMEOUT: Duration = Duration::from_secs(15);

/// The most `claude auth status` may print: a short JSON object.
pub(super) const STATUS_OUTPUT_LIMIT: usize = 64 * 1024;

pub(super) struct ClaudeLogin;

impl LoginFlow for ClaudeLogin {
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
    let (script, urls) = match write_browser_script(flow) {
        Ok(written) => written,
        Err(err) => return flow.failed(&format!("preparing the sign-in failed: {err}")),
    };
    let mut command = flow.command(&["auth", "login"]);
    command.env("BROWSER", script);
    let mut process = match Process::spawn(command, flow.agent.label(), false) {
        Ok(process) => process,
        Err(err) => return flow.failed(&format!("it could not be started: {err}")),
    };

    let mut reported = String::new();
    let mut last_words = String::new();
    let mut poll = tokio::time::interval(URL_POLL);
    let status = loop {
        tokio::select! {
            _ = poll.tick() => {
                let url = last_line_of(&urls);
                if !url.is_empty() && url != reported {
                    progress.waiting(Some(url.clone()), None, false);
                    reported = url;
                }
            }
            Some(line) = process.lines.recv() => {
                if let Some(words) = line.into_words() {
                    last_words = words;
                }
            }
            status = process.child.wait() => break status,
            () = cancel.notified() => {
                process.stop().await;
                return Outcome::Cancelled;
            }
        }
    };
    let status = match status {
        Ok(status) => status,
        Err(err) => return flow.failed(&err.to_string()),
    };
    process.last_words(&mut last_words).await;
    if !status.success() {
        return flow.ended(status, &last_words);
    }
    // The CLI has gone, but a cancel is still honoured while its answer is waited for.
    let signed_in = tokio::select! {
        signed_in = claude_signed_in(flow) => signed_in,
        () = cancel.notified() => return Outcome::Cancelled,
    };
    if signed_in {
        Outcome::Succeeded
    } else {
        flow.not_confirmed()
    }
}

/// Writes the `BROWSER` script, which appends its argument to the returned file, into the
/// scratch directory. A script because `BROWSER` is run as `$BROWSER <url>`.
fn write_browser_script(flow: &Flow) -> std::io::Result<(PathBuf, PathBuf)> {
    // Private to the user, as the script is run by whatever the CLI hands the URL to.
    platform::create_private_dir(flow.scratch_dir())?;
    let urls = flow.scratch_dir().join("urls");
    let script = shell::write_argument_recorder(&flow.scratch_dir().join("browser.sh"), &urls)?;
    Ok((script, urls))
}

/// Whether `claude auth status` says the account is signed in. It exits 1 when it is not, with
/// the answer on stdout either way, so the exit status is not looked at.
async fn claude_signed_in(flow: &Flow) -> bool {
    let mut command = flow.command(&["auth", "status"]);
    let output = tokio::task::spawn_blocking(move || {
        account_cli::run_query(&mut command, STATUS_TIMEOUT, STATUS_OUTPUT_LIMIT, None)
    })
    .await;
    let Ok(Ok(output)) = output else {
        return false;
    };
    logged_in(&output.stdout).unwrap_or(false)
}

/// The `loggedIn` of what `claude auth status` printed; `None` when it printed no such answer.
pub(super) fn logged_in(stdout: &[u8]) -> Option<bool> {
    serde_json::from_slice::<Value>(stdout)
        .ok()?
        .get("loggedIn")?
        .as_bool()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::AgentId;
    use crate::test_support::PATIENCE;

    #[cfg(unix)]
    fn text_file_busy(err: &std::io::Error) -> bool {
        err.raw_os_error() == Some(libc::ETXTBSY)
    }

    #[cfg(not(unix))]
    fn text_file_busy(_: &std::io::Error) -> bool {
        false
    }

    #[test]
    fn the_browser_script_appends_its_argument_to_the_file_the_flow_watches() {
        let dir = crate::test_support::ScratchDir::new("login-browser-script");
        let flow = Flow::unresolved(AgentId::CLAUDE, dir.join("it's here"));
        let (script, urls) = write_browser_script(&flow).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(flow.scratch_dir())
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o700);
        }
        for url in ["https://a/one?x=1&y=2", "https://a/two"] {
            // A process forked by another thread while the script was open for writing holds it
            // open until it execs, and until then the script cannot be run (`ETXTBSY`).
            let deadline = std::time::Instant::now() + PATIENCE;
            let status = loop {
                match std::process::Command::new(&script).arg(url).status() {
                    Err(err) if text_file_busy(&err) && std::time::Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    other => break other.unwrap(),
                }
            };
            assert!(status.success());
        }
        assert_eq!(last_line_of(&urls), "https://a/two");
        assert_eq!(
            std::fs::read_to_string(&urls)
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            ["https://a/one?x=1&y=2", "https://a/two"]
        );
    }
}
