//! Pi 1.5.2's xAI sign-in uses a device code on a private terminal. The CLI owns the entire
//! authorization exchange and stores the credential after the user authorizes it.

use std::future::Future;
use std::pin::Pin;

use tokio::sync::Notify;

use crate::accounts::background_cli::TerminalProcess;
use crate::accounts::login::flows::{Flow, LoginFlow, Outcome, Progress};

pub(super) struct PiLogin;

impl LoginFlow for PiLogin {
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
    let command = match flow.command(&["auth", "login", "xai", "--no-browser"]) {
        Ok(command) => command,
        Err(outcome) => return outcome,
    };
    let mut process = match TerminalProcess::spawn(&command, flow.agent.label()) {
        Ok(process) => process,
        Err(err) => return flow.failed(&format!("it could not be started: {err}")),
    };
    let mut announced = false;
    let mut expired = false;
    loop {
        tokio::select! {
            () = cancel.notified() => {
                process.stop().await;
                return Outcome::Cancelled;
            }
            Some(line) = process.lines.recv() => {
                if !announced {
                    if let Some((page, code)) = page_and_code(&line.text) {
                        progress.waiting(Some(page.to_owned()), Some(code.to_owned()), false);
                        announced = true;
                    }
                }
                expired |= line.text.contains("xAI device code expired");
            }
            success = process.child.exited() => {
                let mut last = String::new();
                process.last_words(&mut last).await;
                expired |= last.contains("xAI device code expired");
                return if success == Some(true) && announced {
                    Outcome::Succeeded
                } else if success == Some(true) {
                    flow.not_confirmed()
                } else if expired {
                    Outcome::TimedOut
                } else {
                    flow.failed("Pi ended the xAI device authorization")
                };
            }
        }
    }
}

fn page_and_code(line: &str) -> Option<(&str, &str)> {
    let (page, code) = line
        .trim()
        .strip_prefix("Open ")?
        .split_once(" and enter code: ")?;
    if !page.starts_with("https://accounts.x.ai/")
        || page.chars().any(char::is_whitespace)
        || !(4..=32).contains(&code.len())
        || !code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return None;
    }
    Some((page, code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_xai_device_authorization_line() {
        assert_eq!(
            page_and_code("Open https://accounts.x.ai/device and enter code: XXXX-XXXX"),
            Some(("https://accounts.x.ai/device", "XXXX-XXXX"))
        );
    }
}
