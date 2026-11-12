//! How Grok Build's hook events are read.
//!
//! `Stop` fires twice per session — once per turn with `reason: "end_turn"` and again at teardown
//! with `reason: "shutdown"` — so an unfiltered mapping ends every session with a phantom finished
//! turn. Payload keys exist in both camelCase and snake_case. Only a named subagent's `agent_type`
//! marks a subagent's payload (the default). Its `Stop` also carries a `backgroundTasks`, but
//! nothing records how it is shaped or whether the work outlives the turn, so it is not read.
//!
//! A post event says only that its own call ended, and one response's calls run side by side
//! (1.0.77): each call fires `PreToolUse` and gets its permission decision in turn, and then they
//! all run at once, so a shell command or a subagent started beside an `ask_user_question` can
//! end while the question waits on the user. So a hand raised for a question stays up until the
//! question's own call ends, which takes the calls told apart
//! ([`HookInterpreter::tool_call_change`]).
//!
//! A turn cancelled before its first token fires nothing at all, so its terminal title is watched
//! from its `UserPromptSubmit`, and again from each event that brings it back from waiting on the
//! user (`crate::session::title_interrupt`).

use serde_json::Value;

use crate::hooks::{
    text_field, CallShape, HookInterpreter, RecordWatch, ToolCallChange, TurnEnd, TurnOutcome,
};
use crate::protocol::{AgentId, SessionStatus};

pub(super) struct GrokHooks;

impl HookInterpreter for GrokHooks {
    fn status(&self, event: &str, payload: &Value) -> Option<SessionStatus> {
        match event {
            "SessionStart" => Some(SessionStatus::Idle),
            "UserPromptSubmit" | "PostToolUse" | "PostToolUseFailure" | "PermissionDenied" => {
                Some(SessionStatus::Working)
            }
            "PreToolUse" => {
                if AgentId::GROK.descriptor().is_ask_question_tool(field(
                    payload,
                    "toolName",
                    "tool_name",
                )) {
                    Some(SessionStatus::WaitingUser)
                } else {
                    Some(SessionStatus::Working)
                }
            }
            // Only the per-turn fire means the turn ended. The teardown fire carries
            // `reason: "shutdown"` and is the session going away, which the process exit reports.
            "Stop" if is_shutdown(payload) => None,
            "Stop" => Some(SessionStatus::Idle),
            "StopFailure" | "StopCancelled" => Some(SessionStatus::Idle),
            // Matched on the type, never on `message`, which is display text that changes between
            // releases.
            "Notification" => match notification_type(payload) {
                Some("idle_prompt") => Some(SessionStatus::Idle),
                Some("permission_prompt") | Some("elicitation_dialog") => {
                    Some(SessionStatus::WaitingUser)
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// Its `promptId` (also spelled `prompt_id`).
    fn turn_key(&self, payload: &Value) -> Option<String> {
        let key = field(payload, "promptId", "prompt_id")?;
        (!key.is_empty()).then(|| key.to_string())
    }

    // The default `submitted_prompt`: Grok Build (1.0.70) supplies the pasted text verbatim.

    fn turn_cancelled(&self, event: &str) -> bool {
        event == "StopCancelled"
    }

    /// The teardown `Stop` and the `idle_prompt` backstop, which also fires after an ordinary
    /// `Stop`, say nothing of how a turn ended.
    fn turn_outcome(&self, event: &str, payload: &Value) -> Option<TurnOutcome> {
        match event {
            "StopCancelled" | "StopFailure" => Some(TurnOutcome::Abnormal),
            "Stop" if !is_shutdown(payload) => Some(TurnOutcome::Normal),
            _ => None,
        }
    }

    fn turn_end(&self, event: &str, payload: &Value) -> Option<TurnEnd> {
        match event {
            // The teardown fire is the session going away, not a turn ending.
            "Stop" if is_shutdown(payload) => None,
            "Stop" => Some(TurnEnd {
                last_assistant_message: last_assistant_message(payload),
                failed: false,
                backstop: false,
            }),
            "StopFailure" => Some(TurnEnd {
                last_assistant_message: last_assistant_message(payload),
                failed: true,
                backstop: false,
            }),
            // The backstop for the turns Grok reports no stop event for at all. It carries no turn
            // id and no last message, and it also fires after an ordinary `Stop` — which is why
            // acting on a turn end is gated on a turn still being open.
            "Notification" if notification_type(payload) == Some("idle_prompt") => Some(TurnEnd {
                last_assistant_message: None,
                failed: false,
                backstop: true,
            }),
            _ => None,
        }
    }

    /// After a usage-limit error it fires `Notification` (type `agent_error`) and `StopFailure`
    /// with a modal on screen, while Claude Code's and Codex's errors leave their prompts as they
    /// were.
    fn stopped_on_agent_error(&self, event: &str, payload: &Value) -> bool {
        match event {
            "StopFailure" => true,
            "Notification" => notification_type(payload) == Some("agent_error"),
            _ => false,
        }
    }

    /// `PreToolUse`, both post events and `PermissionDenied` name their call by `toolUseId` (also
    /// `tool_use_id`). Only the question's own call waits on the user: a permission prompt
    /// (`Notification` `permission_prompt`, which names no call) holds back every other call of its
    /// response until it is answered (1.0.77), so no other call ends while it is up. A prompt, a
    /// turn's end and a session's start leave no call open.
    fn tool_call_change<'a>(&self, event: &str, payload: &'a Value) -> Option<ToolCallChange<'a>> {
        let id = || field(payload, "toolUseId", "tool_use_id");
        match event {
            "PreToolUse" => {
                // No input: a call's shape serves only to match a permission prompt to it, and a
                // Grok Build prompt is never matched to a call (see above).
                let call = CallShape {
                    tool: field(payload, "toolName", "tool_name"),
                    input: None,
                };
                let waits_on_user = AgentId::GROK.descriptor().is_ask_question_tool(call.tool);
                Some(ToolCallChange::Started {
                    id: id(),
                    call,
                    waits_on_user,
                })
            }
            // A denied call never runs, so no post event follows it.
            "PostToolUse" | "PostToolUseFailure" | "PermissionDenied" => {
                Some(ToolCallChange::Ended { id: id() })
            }
            "UserPromptSubmit" | "Stop" | "StopFailure" | "StopCancelled" | "SessionStart" => {
                Some(ToolCallChange::AllEnded)
            }
            _ => None,
        }
    }

    fn watch_reads_previous_status(&self) -> bool {
        true
    }

    /// From the prompt, and again each time the hand comes down: whatever the title showed while a
    /// card waited on the user must not end the turn that carries on after it.
    fn record_watch<'a>(
        &self,
        event: &str,
        _payload: &'a Value,
        previous: Option<SessionStatus>,
        status: SessionStatus,
    ) -> Option<RecordWatch<'a>> {
        (status == SessionStatus::Working
            && (self.starts_conversation(event) || previous == Some(SessionStatus::WaitingUser)))
        .then_some(RecordWatch::Interrupt {
            record: None,
            turn_id: None,
        })
    }
}

fn is_shutdown(payload: &Value) -> bool {
    payload.get("reason").and_then(Value::as_str) == Some("shutdown")
}

fn notification_type(payload: &Value) -> Option<&str> {
    field(payload, "notificationType", "notification_type")
}

fn last_assistant_message(payload: &Value) -> Option<String> {
    text_field(payload, "lastAssistantMessage")
        .or_else(|| text_field(payload, "last_assistant_message"))
}

/// Grok writes every payload field in both camelCase and snake_case; read whichever is present.
fn field<'a>(payload: &'a Value, camel: &str, snake: &str) -> Option<&'a str> {
    payload
        .get(camel)
        .or_else(|| payload.get(snake))
        .and_then(Value::as_str)
}
