//! A Qwen Code API-key sign-in through its own Web Shell. The user gives the key directly to
//! Qwen Code. Octoboard only starts the loopback listener and reads the resulting sign-in state.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::time::{Duration, SystemTime};

use tokio::sync::Notify;
use uuid::Uuid;

use super::accounts::QwenAccounts;
use crate::accounts::background_cli::Process;
use crate::accounts::login::flows::{Flow, LoginFlow, Outcome, Progress};
use crate::accounts::quotas::sign_in::SignInReading;
use crate::accounts::AccountOps;
use crate::adapter::account_dir;
use crate::providers::qwen::QWEN_SETTINGS_FILE;

const SETTINGS_POLL: Duration = Duration::from_millis(500);

pub(super) struct QwenLogin;

impl LoginFlow for QwenLogin {
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
    let mut command = match flow.command(&[
        "serve",
        "--hostname",
        "127.0.0.1",
        "--port",
        "0",
        "--web",
        "--require-auth",
        "--no-open",
        "--no-open-with-auth",
        "--no-token-qr",
    ]) {
        Ok(command) => command,
        Err(outcome) => return outcome,
    };
    let Ok(workspace) = super::settings::login_workspace(flow.scratch_dir()) else {
        return flow.failed("the login workspace could not be created");
    };
    // The secret is passed to the CLI in its environment and to the Web Shell in its URL
    // fragment. Qwen Code leaves it off its startup output when it doesn't open the browser.
    let access = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    command
        .current_dir(workspace.path())
        .env("QWEN_SERVER_TOKEN", &access);
    let account = account_dir(flow.agent, flow.config_dir.as_deref(), &flow.shell_env);
    let settings = account.join(QWEN_SETTINGS_FILE);
    let mut process = match Process::spawn(command, flow.agent.label(), false) {
        Ok(process) => process,
        Err(_) => return flow.failed("the Qwen Code Web Shell could not be started"),
    };

    let port = loop {
        tokio::select! {
            Some(line) = process.lines.recv() => {
                if let Some(port) = listening_port(&line.text) {
                    break port;
                }
            }
            _ = process.child.wait() => return flow.failed("the Qwen Code Web Shell exited before listening"),
            () = cancel.notified() => {
                process.stop().await;
                return Outcome::Cancelled;
            }
        }
    };
    // Startup can migrate old settings. Only a change after the listener came up can indicate
    // the user's browser action.
    let mut last_stamp = stamp(&settings);
    progress.waiting(
        Some(format!("http://127.0.0.1:{port}/#token={access}")),
        None,
        false,
    );

    let mut polls = tokio::time::interval(SETTINGS_POLL);
    loop {
        tokio::select! {
            _ = polls.tick() => {
                let now = stamp(&settings);
                if now == last_stamp {
                    continue;
                }
                last_stamp = now;
                if matches!(
                    QwenAccounts.sign_in_state(flow.config_dir.as_deref(), &flow.shell_env),
                    SignInReading::SignedIn { .. }
                ) {
                    process.stop().await;
                    return Outcome::Succeeded;
                }
            }
            _ = process.child.wait() => return flow.not_confirmed(),
            () = cancel.notified() => {
                process.stop().await;
                return Outcome::Cancelled;
            }
        }
    }
}

fn stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

fn listening_port(line: &str) -> Option<u16> {
    let address = line.strip_prefix("qwen serve listening on http://127.0.0.1:")?;
    address
        .split_whitespace()
        .next()?
        .parse()
        .ok()
        .filter(|port| *port > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_web_shell_reports_the_loopback_port_it_was_given() {
        let line =
            "qwen serve listening on http://127.0.0.1:50245 (mode=http-bridge, workspace=/scratch)";
        assert_eq!(listening_port(line), Some(50245));
    }
}
