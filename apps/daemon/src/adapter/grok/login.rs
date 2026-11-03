//! Signing a Grok Build account in: `grok login --device-auth`, which prints the device URL and
//! code on stderr and opens the browser itself, which cannot be stopped. Done is exit 0 with
//! `auth.json` in its home. Measured in `docs/agent-cli-reference.md` ("Signing in a config
//! directory without a terminal").

use std::future::Future;
use std::pin::Pin;

use tokio::sync::Notify;

use crate::accounts::login::flows::{Flow, LoginFlow, Outcome, Process, Progress};
use crate::providers::grok::GROK_SIGN_IN_FILE;

pub(super) struct GrokLogin;

impl LoginFlow for GrokLogin {
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
    let mut process = match Process::spawn(
        flow.command(&["login", "--device-auth"]),
        flow.agent.label(),
        false,
    ) {
        Ok(process) => process,
        Err(err) => return flow.failed(&format!("it could not be started: {err}")),
    };
    let (mut url, mut device_code) = (None, None);
    let mut last_words = String::new();
    let status = loop {
        tokio::select! {
            Some(line) = process.lines.recv() => {
                let mut found = false;
                if url.is_none() {
                    if let Some(page) = url_in(&line.text) {
                        device_code = device_code.take().or_else(|| user_code_in(&page));
                        url = Some(page);
                        found = true;
                    }
                }
                if device_code.is_none() && looks_like_device_code(&line.text) {
                    device_code = Some(line.text.clone());
                    found = true;
                }
                if found {
                    progress.waiting(url.clone(), device_code.clone(), true);
                }
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
    match &flow.resolved_dir {
        Some(dir) if !dir.join(GROK_SIGN_IN_FILE).is_file() => flow.not_confirmed(),
        _ => Outcome::Succeeded,
    }
}

/// The first `https://` word of `line`.
fn url_in(line: &str) -> Option<String> {
    line.split_whitespace()
        .find(|word| word.starts_with("https://"))
        .map(str::to_string)
}

/// The `user_code` of a device-flow page's address.
fn user_code_in(url: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix("user_code="))
        .filter(|code| !code.is_empty())
        .map(str::to_string)
}

/// A line that is only a code of two groups of four letters or digits, as Grok Build prints it
/// under "Confirm this code in your browser".
fn looks_like_device_code(line: &str) -> bool {
    let mut groups = line.split('-');
    let is_group = |group: Option<&str>| {
        group.is_some_and(|group| {
            group.len() == 4 && group.chars().all(|c| c.is_ascii_alphanumeric())
        })
    };
    is_group(groups.next()) && is_group(groups.next()) && groups.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grok_builds_its_page_and_code_from_the_lines_it_prints() {
        let url = "https://accounts.x.ai/oauth2/device?user_code=ABCD-1234";
        assert_eq!(url_in(&format!("  {url}")).as_deref(), Some(url),);
        assert_eq!(url_in("To sign in, open this URL in your browser:"), None);
        assert_eq!(user_code_in(url).as_deref(), Some("ABCD-1234"));
        assert_eq!(user_code_in("https://accounts.x.ai/oauth2/device"), None);
        assert!(looks_like_device_code("ABCD-1234"));
        assert!(!looks_like_device_code("Waiting for authorization..."));
        assert!(!looks_like_device_code("ABCD-12345"));
        assert!(!looks_like_device_code("abcd-12-34"));
    }
}
