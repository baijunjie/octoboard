//! Signing a Codex account in: `codex app-server`, where `account/login/start` answers with the
//! authorization URL, `account/login/completed` says how it went, and `account/read` confirms. The
//! config directory is created first, as Codex refuses a missing one; for the default account that
//! is the home the agent resolved. Codex's browser flow listens on a fixed local port, which two
//! logins cannot share. Measured in `docs/agent-cli-reference.md` ("Signing in a config directory
//! without a terminal").

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::Notify;

use super::app_server::{self, CODEX_APP_SERVER_ARGS};
use crate::accounts::login::flows::{Flow, LoginFlow, Outcome, Process, Progress};
use crate::agents::json_rpc;
use crate::providers::ProviderCode;

/// How long Codex's app server is given to answer a cancel before it is killed anyway.
const CANCEL_GRACE: Duration = Duration::from_secs(2);

pub(super) struct CodexLogin;

impl LoginFlow for CodexLogin {
    fn run<'a>(
        &'a self,
        flow: &'a Flow,
        progress: &'a Progress,
        cancel: &'a Notify,
    ) -> Pin<Box<dyn Future<Output = Outcome> + Send + 'a>> {
        Box::pin(sign_in(flow, progress, cancel))
    }

    /// Codex's browser flow holds one fixed local port.
    fn busy_code(&self) -> Option<ProviderCode> {
        Some(crate::providers::codex::LOGIN_BUSY)
    }
}

async fn sign_in(flow: &Flow, progress: &Progress, cancel: &Notify) -> Outcome {
    // Codex refuses a home that does not exist, the default account's included.
    if let Some(dir) = flow.config_dir.as_ref().or(flow.resolved_dir.as_ref()) {
        if let Err(err) = std::fs::create_dir_all(dir) {
            return flow.failed(&format!("creating {} failed: {err}", dir.display()));
        }
    }
    let mut process = match Process::spawn(
        flow.command(CODEX_APP_SERVER_ARGS),
        flow.agent.label(),
        true,
    ) {
        Ok(process) => process,
        Err(err) => return flow.failed(&format!("it could not be started: {err}")),
    };
    let requests = [
        app_server::initialize(),
        app_server::initialized(),
        json!({"jsonrpc": "2.0", "id": 2, "method": "account/login/start",
            "params": {"type": "chatgpt"}}),
    ];
    let mut last_words = String::new();
    for request in &requests {
        if let Err(outcome) = flow.send(&mut process, request, &mut last_words).await {
            return outcome;
        }
    }

    let mut login_id: Option<String> = None;
    loop {
        tokio::select! {
            Some(line) = process.lines.recv() => {
                let Ok(message) = serde_json::from_str::<Value>(&line.text) else {
                    if let Some(words) = line.into_words() {
                        last_words = words;
                    }
                    continue;
                };
                match codex_event(&message, login_id.as_deref()) {
                    CodexEvent::Started { login, url } => {
                        login_id = Some(login);
                        progress.waiting(Some(url), None, false);
                    }
                    CodexEvent::Completed => {
                        let read = json!({"jsonrpc": "2.0", "id": 3, "method": "account/read",
                            "params": {}});
                        if let Err(outcome) =
                            flow.send(&mut process, &read, &mut last_words).await
                        {
                            return outcome;
                        }
                    }
                    CodexEvent::Confirmed(true) => return Outcome::Succeeded,
                    CodexEvent::Confirmed(false) => return flow.not_confirmed(),
                    CodexEvent::Rejected(detail) => return flow.failed(&detail),
                    CodexEvent::GaveUp(detail) if is_timeout(&detail) => {
                        return Outcome::TimedOut;
                    }
                    CodexEvent::GaveUp(detail) => return flow.failed(&detail),
                    CodexEvent::Other => {}
                }
            }
            status = process.child.wait() => {
                return match status {
                    Ok(status) => {
                        process.last_words(&mut last_words).await;
                        flow.ended(status, &last_words)
                    }
                    Err(err) => flow.failed(&err.to_string()),
                };
            }
            () = cancel.notified() => {
                if let Some(login) = &login_id {
                    let cancel = json!({"jsonrpc": "2.0", "id": 4,
                        "method": "account/login/cancel", "params": {"loginId": login}});
                    // Best effort: its listener goes with the process either way, but this
                    // lets it close the flow itself first.
                    if process.send(&cancel).await.is_ok() {
                        let completed = until_completed(&mut process);
                        let _ = tokio::time::timeout(CANCEL_GRACE, completed).await;
                    }
                }
                process.stop().await;
                return Outcome::Cancelled;
            }
        }
    }
}

/// Whether Codex's account of why a login did not complete says it ran out of time.
fn is_timeout(detail: &str) -> bool {
    detail.to_lowercase().contains("timed out")
}

/// What a line of Codex's app server means to the login.
enum CodexEvent {
    /// The reply to `account/login/start`.
    Started {
        login: String,
        url: String,
    },
    /// `account/login/completed` with `success: true`.
    Completed,
    /// The reply to `account/read`: whether an account is there.
    Confirmed(bool),
    /// The reply to `account/login/start` was an error.
    Rejected(String),
    /// `account/login/completed` with `success: false`, and what it said.
    GaveUp(String),
    Other,
}

fn codex_event(message: &Value, login: Option<&str>) -> CodexEvent {
    if json_rpc::is_reply_to(message, 2) {
        if let Some(detail) = json_rpc::error_detail(message) {
            return CodexEvent::Rejected(detail);
        }
        let result = message.get("result");
        let field = |name: &str| {
            result
                .and_then(|result| result.get(name))
                .and_then(Value::as_str)
        };
        return match (field("loginId"), field("authUrl")) {
            (Some(login), Some(url)) => CodexEvent::Started {
                login: login.to_string(),
                url: url.to_string(),
            },
            _ => CodexEvent::Rejected("the login start was answered without a page".to_string()),
        };
    }
    if json_rpc::is_reply_to(message, 3) {
        let signed_in = message
            .pointer("/result/account")
            .is_some_and(|account| !account.is_null());
        return CodexEvent::Confirmed(signed_in);
    }
    if message.get("method").and_then(Value::as_str) == Some("account/login/completed") {
        let params = message.get("params");
        let named = params
            .and_then(|params| params.get("loginId"))
            .and_then(Value::as_str);
        if login.is_some() && named.is_some() && named != login {
            return CodexEvent::Other;
        }
        if params.and_then(|params| params.get("success")) == Some(&Value::Bool(true)) {
            return CodexEvent::Completed;
        }
        let detail = params
            .and_then(|params| params.get("error"))
            .and_then(Value::as_str)
            .unwrap_or("the login did not complete");
        return CodexEvent::GaveUp(detail.to_string());
    }
    CodexEvent::Other
}

/// Reads on until Codex's `account/login/completed`, or until the CLI's output ends.
async fn until_completed(process: &mut Process) {
    while let Some(line) = process.lines.recv().await {
        let completed = serde_json::from_str::<Value>(&line.text).is_ok_and(|message| {
            message.get("method").and_then(Value::as_str) == Some("account/login/completed")
        });
        if completed {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_lines_are_told_apart_by_their_ids_and_methods() {
        let started: Value = serde_json::from_str(
            r#"{"id":2,"result":{"type":"chatgpt","loginId":"L","authUrl":"https://a/b"}}"#,
        )
        .unwrap();
        assert!(matches!(
            codex_event(&started, None),
            CodexEvent::Started { login, url } if login == "L" && url == "https://a/b"
        ));
        let refused: Value =
            serde_json::from_str(r#"{"id":2,"error":{"code":-32600,"message":"busy"}}"#).unwrap();
        assert!(matches!(codex_event(&refused, None), CodexEvent::Rejected(m) if m == "busy"));
        let done = |extra: &str| -> Value {
            serde_json::from_str(&format!(
                r#"{{"method":"account/login/completed","params":{{"loginId":"L",{extra}}}}}"#
            ))
            .unwrap()
        };
        assert!(matches!(
            codex_event(&done(r#""success":true"#), Some("L")),
            CodexEvent::Completed
        ));
        assert!(matches!(
            codex_event(&done(r#""success":false,"error":"no""#), Some("L")),
            CodexEvent::GaveUp(m) if m == "no"
        ));
        // A message with a method is the server's own, whatever id it numbers itself by.
        let request: Value =
            serde_json::from_str(r#"{"id":2,"method":"item/tool/call","params":{}}"#).unwrap();
        assert!(matches!(codex_event(&request, None), CodexEvent::Other));
        // Another login's end is not this one's.
        assert!(matches!(
            codex_event(&done(r#""success":true"#), Some("other")),
            CodexEvent::Other
        ));
        let read = |account: &str| -> Value {
            serde_json::from_str(&format!(r#"{{"id":3,"result":{{"account":{account}}}}}"#))
                .unwrap()
        };
        assert!(matches!(
            codex_event(&read(r#"{"type":"chatgpt"}"#), None),
            CodexEvent::Confirmed(true)
        ));
        assert!(matches!(
            codex_event(&read("null"), None),
            CodexEvent::Confirmed(false)
        ));
    }
}
