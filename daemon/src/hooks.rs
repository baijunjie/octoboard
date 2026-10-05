//! Turning one agent's hook event into a session status.
//!
//! Each agent names its events and shapes its payloads differently, and each has traps that make a
//! naive mapping wrong in a way nothing reports. What is handled here:
//!
//! - **Claude Code**: `Stop` and `StopFailure` are mutually exclusive, so both mean "turn over"; a
//!   `Stop` carrying a non-empty `background_tasks` means paused on background work, not finished;
//!   and `Notification` carries the state in `notification_type`, of which `idle_prompt` is the only
//!   one worth mapping: its `permission_prompt` does fire, but a fixed six seconds after the
//!   `PermissionRequest` that already raised the hand, so it says nothing new. `idle_prompt` is no
//!   use as a general backstop either, being armed by `Stop` — a turn that fires no `Stop` never
//!   produces one.
//! - **Grok**: `Stop` fires twice per session — once per turn with `reason: "end_turn"` and again
//!   at teardown with `reason: "shutdown"` — so an unfiltered mapping ends every session with a
//!   phantom finished turn. Payload keys exist in both camelCase and snake_case.
//! - **Codex**: `Interrupt` replaces `Stop` for a cancelled turn, and `PermissionRequest` fires
//!   before the modal reaches the user.
//!
//! Three gaps are accepted rather than worked around, all of them settled during validation: a turn
//! that ends with a plain-text question to the user is indistinguishable from a finished turn on
//! every agent; a user interrupt on Claude Code produces no event at all, so the session reads as
//! working until the next `UserPromptSubmit`; and Grok's bash mode (`!`) fires
//! no tool or turn events (only its `idle_prompt` backstop follows). None is recoverable by matching
//! terminal text, which would need a VT emulator in the daemon and differs between an agent's own
//! renderers.
//!
//! The one case not left as a gap is a Claude Code session that is *waiting on the user* when such an
//! interrupt lands — declining a permission prompt or its `AskUserQuestion` is silent in exactly the
//! same way, and leaving it would strand the session's raised hand with nothing to lower it. That one
//! is recovered outside this mapping, from the agent's own transcript; see `crate::transcript`.
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

/// How long an open turn must have been quiet before a clock-attributed turn end may be read as
/// belonging to it. Grok's `idle_prompt` backstop fires about 60 s after a turn ends and carries no
/// turn id, so a turn still producing events cannot be the one it is talking about. Set under that
/// delay so jitter does not make a legitimate backstop unusable.
pub const BACKSTOP_QUIET: std::time::Duration = std::time::Duration::from_secs(45);

/// A turn that has just ended, as far as a report can be built from it.
pub struct TurnEnd {
    /// The turn's final assistant text, where the agent reports one. Absent on a turn-end signal
    /// that carries no payload of its own — Grok's `idle_prompt` backstop is the case that matters.
    pub last_assistant_message: Option<String>,
    /// The turn ended in an error rather than with a result. `last_assistant_message` is then the
    /// user-facing error text, not model output, which anything reading it as the turn's outcome
    /// has to account for.
    pub failed: bool,
    /// This end carries no turn id of its own and can only be attributed by session and clock —
    /// Grok's `idle_prompt` is the one such signal. It also fires after an ordinary `Stop`, so it
    /// may belong to a turn that has already been accounted for, or to one that ended before the
    /// turn now open began.
    pub backstop: bool,
}

/// Whether this event means the user cancelled the turn.
///
/// Separate from [`turn_end`] because the two differ in what they owe: a cancelled turn produces no
/// report — the user did it and knows — but it still has to be *closed*, or a later clock-attributed
/// end would find it open and invent a report for it.
pub fn turn_cancelled(agent: Agent, event: &str) -> bool {
    match agent {
        // The only cancellation signal any of the three agents gives. A declined approval aborts
        // the turn and fires it too.
        Agent::Codex => event == "Interrupt",
        Agent::Grok => event == "StopCancelled",
        // Claude Code emits nothing at all when the user interrupts. The turn stays open until the
        // next `UserPromptSubmit` re-arms it, which is stale rather than wrong.
        Agent::Claude => false,
    }
}

/// Whether this event means a turn has ended, and what can be read out of it.
///
/// Distinct from [`status_from_event`] on purpose: several events put a session back to `idle`
/// without a turn having ended (a `SessionStart`, a cancelled turn), and several turn ends are not
/// reportable (a Claude Code `Stop` that is only paused). Reading one from the other would make a
/// phantom report out of each.
pub fn turn_end(agent: Agent, event: &str, payload: &Value) -> Option<TurnEnd> {
    match agent {
        Agent::Claude => match event {
            // A non-empty `background_tasks` means paused on work still running, so the turn is
            // not over and synthesising here would report an unfinished turn as a result.
            "Stop" if claude_paused(payload) => None,
            "Stop" => Some(TurnEnd {
                last_assistant_message: text_field(payload, "last_assistant_message"),
                failed: false,
                backstop: false,
            }),
            "StopFailure" => Some(TurnEnd {
                last_assistant_message: text_field(payload, "last_assistant_message"),
                failed: true,
                backstop: false,
            }),
            _ => None,
        },
        Agent::Codex => match event {
            "Stop" => Some(TurnEnd {
                last_assistant_message: text_field(payload, "last_assistant_message"),
                failed: false,
                backstop: false,
            }),
            _ => None,
        },
        Agent::Grok => match event {
            // The teardown fire is the session going away, not a turn ending.
            "Stop" if payload.get("reason").and_then(Value::as_str) == Some("shutdown") => None,
            "Stop" => Some(TurnEnd {
                last_assistant_message: text_field(payload, "lastAssistantMessage")
                    .or_else(|| text_field(payload, "last_assistant_message")),
                failed: false,
                backstop: false,
            }),
            "StopFailure" => Some(TurnEnd {
                last_assistant_message: text_field(payload, "lastAssistantMessage")
                    .or_else(|| text_field(payload, "last_assistant_message")),
                failed: true,
                backstop: false,
            }),
            // The backstop for the turns Grok reports no stop event for at all. It carries no turn
            // id and no last message, and it also fires after an ordinary `Stop` — which is why
            // acting on a turn end is gated on a turn still being open.
            "Notification"
                if grok_field(payload, "notificationType", "notification_type")
                    == Some("idle_prompt") =>
            {
                Some(TurnEnd {
                    last_assistant_message: None,
                    failed: false,
                    backstop: true,
                })
            }
            _ => None,
        },
    }
}

/// Whether a Claude Code `Stop` is only paused on background work.
fn claude_paused(payload: &Value) -> bool {
    payload
        .get("background_tasks")
        .and_then(Value::as_array)
        .map(|tasks| !tasks.is_empty())
        .unwrap_or(false)
}

fn text_field(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Grok fires `Stop` twice per session and `idle_prompt` after every turn, so the events that
    /// are *not* a reportable turn end are the ones worth pinning.
    #[test]
    fn only_a_real_turn_end_is_reportable() {
        assert!(turn_end(Agent::Grok, "Stop", &json!({"reason": "shutdown"})).is_none());
        assert!(turn_end(
            Agent::Claude,
            "Stop",
            &json!({"background_tasks": [{"id": "t1"}]})
        )
        .is_none());
        // A cancelled turn owes no report, but it is still a turn ending — the distinction is
        // `turn_cancelled`'s, not this function's.
        assert!(turn_end(Agent::Codex, "Interrupt", &json!({})).is_none());
        assert!(turn_end(Agent::Grok, "StopCancelled", &json!({})).is_none());
    }

    /// A cancelled turn left open is what lets Grok's backstop invent a report for it a minute
    /// later, so the cancellation has to be recognised as one.
    #[test]
    fn a_user_cancellation_is_recognised_where_the_agent_reports_one() {
        assert!(turn_cancelled(Agent::Codex, "Interrupt"));
        assert!(turn_cancelled(Agent::Grok, "StopCancelled"));
        assert!(!turn_cancelled(Agent::Codex, "Stop"));
        // Claude Code emits nothing at all on an interrupt.
        assert!(!turn_cancelled(Agent::Claude, "Interrupt"));
    }

    /// Only the id-less signal may be attributed by the clock; everything else names its own turn.
    #[test]
    fn only_the_backstop_is_marked_as_clock_attributed() {
        assert!(
            turn_end(
                Agent::Grok,
                "Notification",
                &json!({"notificationType": "idle_prompt"})
            )
            .expect("a turn end")
            .backstop
        );
        assert!(
            !turn_end(Agent::Grok, "Stop", &json!({}))
                .expect("a turn end")
                .backstop
        );
    }

    #[test]
    fn the_turns_last_message_comes_through_in_either_spelling() {
        let claude = turn_end(
            Agent::Claude,
            "Stop",
            &json!({"last_assistant_message": "finished"}),
        )
        .expect("a turn end");
        assert_eq!(claude.last_assistant_message.as_deref(), Some("finished"));
        assert!(!claude.failed);

        let grok = turn_end(
            Agent::Grok,
            "Stop",
            &json!({"lastAssistantMessage": "finished"}),
        )
        .expect("a turn end");
        assert_eq!(grok.last_assistant_message.as_deref(), Some("finished"));
    }

    /// On `StopFailure` the message field is the user-facing error text rather than model output,
    /// so a caller has to be able to tell the two apart.
    #[test]
    fn a_failed_turn_is_marked_as_one() {
        let end = turn_end(
            Agent::Claude,
            "StopFailure",
            &json!({"last_assistant_message": "API error"}),
        )
        .expect("a turn end");
        assert!(end.failed);
    }

    /// Grok's backstop carries nothing but the fact that the turn is over.
    #[test]
    fn the_idle_prompt_backstop_is_a_turn_end_with_no_message() {
        let end = turn_end(
            Agent::Grok,
            "Notification",
            &json!({"notificationType": "idle_prompt"}),
        )
        .expect("a turn end");
        assert!(end.last_assistant_message.is_none());
    }
}
