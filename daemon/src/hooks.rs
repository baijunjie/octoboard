//! Turning one agent's hook event into a session status.
//!
//! Each agent names its events and shapes its payloads differently, and each has traps that make a
//! naive mapping wrong in a way nothing reports. What is handled here:
//!
//! - **Claude Code**: `Stop` and `StopFailure` are mutually exclusive, so both mean "turn over"; a
//!   `Stop` carrying a non-empty `background_tasks` means paused on background work, not finished;
//!   and `Notification` carries the state in `notification_type`, of which only `idle_prompt` was
//!   ever seen firing (its `permission_prompt` never fired at all, so a pending prompt comes from
//!   `PermissionRequest`).
//! - **Grok**: `Stop` fires twice per session — once per turn with `reason: "end_turn"` and again
//!   at teardown with `reason: "shutdown"` — so an unfiltered mapping ends every session with a
//!   phantom finished turn. Payload keys exist in both camelCase and snake_case.
//! - **Codex**: `Interrupt` replaces `Stop` for a cancelled turn, and `PermissionRequest` fires
//!   before the modal reaches the user.
//!
//! Three gaps are accepted rather than worked around, all of them settled during validation: a turn
//! that ends with a plain-text question to the user is indistinguishable from a finished turn on
//! every agent; a user interrupt on Claude Code produces no event at all, so the session reads as
//! working until the next `UserPromptSubmit`; and Grok's bash mode (`!`) bypasses hooks entirely.
//! None is recoverable by matching terminal text, which would need a VT emulator in the daemon and
//! differs between an agent's own renderers.
//!
//! For the payload fields behind these decisions — what each event carries, what it can be
//! correlated on, and what never fires — see `docs/agent-cli-reference.md`.

use serde_json::Value;

use crate::protocol::{Agent, SessionStatus};

/// What one hook event says about the session, or `None` when it says nothing Octoboard tracks.
/// A session's end is deliberately not taken from a hook at all: the process exit is observed
/// directly, and whether it means archived or interrupted is not the agent's to say.
pub fn status_from_event(agent: Agent, event: &str, payload: &Value) -> Option<SessionStatus> {
    match agent {
        Agent::Claude => claude_status(event, payload),
        Agent::Codex => codex_status(event),
        Agent::Grok => grok_status(event, payload),
    }
}

/// The agent's own session id, if this payload carries it. For Codex this is the only way the id
/// is ever learned.
pub fn agent_session_id(payload: &Value) -> Option<String> {
    for key in ["session_id", "sessionId"] {
        if let Some(id) = payload.get(key).and_then(Value::as_str) {
            if !id.is_empty() {
                return Some(id.to_string());
            }
        }
    }
    None
}

fn claude_status(event: &str, payload: &Value) -> Option<SessionStatus> {
    match event {
        "SessionStart" => Some(SessionStatus::Idle),
        "UserPromptSubmit" | "PostToolUse" | "PostToolUseFailure" | "PostToolBatch" => {
            Some(SessionStatus::Working)
        }
        // The tool name is in the payload rather than the event: Claude Code's own
        // ask-the-user tool is a question waiting on a person, every other tool is work.
        "PreToolUse" => match payload.get("tool_name").and_then(Value::as_str) {
            Some("AskUserQuestion") => Some(SessionStatus::WaitingUser),
            _ => Some(SessionStatus::Working),
        },
        "PermissionRequest" => Some(SessionStatus::WaitingUser),
        "Notification" => match payload.get("notification_type").and_then(Value::as_str) {
            Some("idle_prompt") => Some(SessionStatus::Idle),
            _ => None,
        },
        "Stop" => {
            let paused = payload
                .get("background_tasks")
                .and_then(Value::as_array)
                .map(|tasks| !tasks.is_empty())
                .unwrap_or(false);
            Some(if paused {
                SessionStatus::Working
            } else {
                SessionStatus::Idle
            })
        }
        "StopFailure" => Some(SessionStatus::Idle),
        _ => None,
    }
}

fn codex_status(event: &str) -> Option<SessionStatus> {
    match event {
        "SessionStart" => Some(SessionStatus::Idle),
        "UserPromptSubmit" | "PreToolUse" | "PostToolUse" => Some(SessionStatus::Working),
        "PermissionRequest" => Some(SessionStatus::WaitingUser),
        // Mutually exclusive with `Stop`, and the only cancellation signal any of the three agents
        // gives. A declined approval fires it too.
        "Interrupt" => Some(SessionStatus::Idle),
        "Stop" => Some(SessionStatus::Idle),
        _ => None,
    }
}

fn grok_status(event: &str, payload: &Value) -> Option<SessionStatus> {
    match event {
        "SessionStart" => Some(SessionStatus::Idle),
        "UserPromptSubmit" | "PostToolUse" | "PostToolUseFailure" | "PermissionDenied" => {
            Some(SessionStatus::Working)
        }
        "PreToolUse" => match grok_field(payload, "toolName", "tool_name") {
            Some("ask_user_question") => Some(SessionStatus::WaitingUser),
            _ => Some(SessionStatus::Working),
        },
        // Only the per-turn fire means the turn ended. The teardown fire carries
        // `reason: "shutdown"` and is the session going away, which the process exit reports.
        "Stop" => match payload.get("reason").and_then(Value::as_str) {
            Some("shutdown") => None,
            _ => Some(SessionStatus::Idle),
        },
        "StopFailure" | "StopCancelled" => Some(SessionStatus::Idle),
        // Matched on the type, never on `message`, which is display text that changes between
        // releases.
        "Notification" => match grok_field(payload, "notificationType", "notification_type") {
            Some("idle_prompt") => Some(SessionStatus::Idle),
            Some("permission_prompt") | Some("elicitation_dialog") => {
                Some(SessionStatus::WaitingUser)
            }
            _ => None,
        },
        _ => None,
    }
}

/// Grok writes every payload field in both camelCase and snake_case; read whichever is present.
fn grok_field<'a>(payload: &'a Value, camel: &str, snake: &str) -> Option<&'a str> {
    payload
        .get(camel)
        .or_else(|| payload.get(snake))
        .and_then(Value::as_str)
}

/// Whether this payload belongs to a subagent rather than to the session itself.
///
/// Subagent events reach Octoboard's hooks because its hooks are registered for the whole agent,
/// not per turn — and reading them as the session's own is destructive, not just noisy: a Grok
/// subagent's `SessionEnd` carries the *child's* session id, so storing it would overwrite the id
/// a resume needs and point the next resume at a conversation the user never had. A subagent's
/// `StopCancelled` would likewise report the session as finished while it is still working.
pub fn is_subagent(payload: &Value) -> bool {
    ["subagentType", "subagent_type", "agent_type"]
        .iter()
        .filter_map(|key| payload.get(*key))
        .any(|value| match value {
            Value::String(text) => !text.is_empty(),
            Value::Null => false,
            _ => true,
        })
}

/// Whether this event means the session now has a conversation the agent could resume. A session
/// nobody has typed into has none, and resuming it by id fails rather than reopening anything.
///
/// It is the prompt submission that creates the record, not the process starting — which is also
/// where Codex's own session id comes from, since its thread is created lazily on that first
/// submission.
pub fn starts_conversation(event: &str) -> bool {
    event == "UserPromptSubmit"
}

/// The event name as the payload itself reports it. This is the only source: the hook script is
/// argument-free and the callback URL carries the session, not the event.
pub fn event_name(payload: &Value) -> Option<String> {
    for key in ["hook_event_name", "hookEventName"] {
        if let Some(name) = payload.get(key).and_then(Value::as_str) {
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}
