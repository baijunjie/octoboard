//! How Codex's hook events are read.
//!
//! `Interrupt` replaces `Stop` for a cancelled turn, and `PermissionRequest` fires before the modal
//! reaches the user — in the same instant as the same call's `PreToolUse`, and the two are injected
//! as asynchronous hooks, so they arrive in either order; that is why no status is read from the
//! pre-event. A turn that ends in an error fires neither `Stop` nor `Interrupt`: only its
//! `UserPromptSubmit`, with nothing after it, so that ending is read from Codex's rollout file
//! instead, by the record watch asked for after the prompt
//! (`crate::adapter::codex::rollout_watch`).
//!
//! Codex's schemas give some events an optional `agent_id` without saying the main thread leaves it
//! empty, so only a named subagent's `agent_type` marks a subagent's payload (the default). Codex
//! reports no background tasks.

use serde_json::Value;

use crate::hooks::{text_field, HookInterpreter, RecordWatch, TurnEnd, TurnOutcome};
use crate::protocol::SessionStatus;

pub(super) struct CodexHooks;

impl HookInterpreter for CodexHooks {
    fn status(&self, event: &str, _payload: &Value) -> Option<SessionStatus> {
        match event {
            // No `SessionStart` arm. Measured for its `startup` source only: Codex defers it to the
            // first prompt submission, so it arrives together with that prompt's `UserPromptSubmit`
            // and, handled last, would put a working first turn back to idle. The launch and the
            // resume already set `Idle` themselves. The other sources (`resume`, `clear`,
            // `compact`, `fork`) are unmeasured. This gives up what a `clear` or `fork` one
            // mid-session may have done, correcting a session stuck at working; a `compact` one
            // reporting `Idle` would have been wrong anyway.
            //
            // No `PreToolUse` arm either, and for a different reason: the module header has the
            // race with the same call's `PermissionRequest`. A `Working` landing after that
            // request's `WaitingUser` would lower the hand for as long as the modal is up and
            // release whatever is queued for the session into it, where the paste's trailing Enter
            // answers it. Nothing is lost: the turn's own `UserPromptSubmit` has already reported
            // work, and an approval that is granted is followed by the call's `PostToolUse`.
            "UserPromptSubmit" | "PostToolUse" => Some(SessionStatus::Working),
            "PermissionRequest" => Some(SessionStatus::WaitingUser),
            // Mutually exclusive with `Stop`, and the only cancellation signal Claude Code, Codex
            // or Grok Build gives. A declined approval fires it too.
            "Interrupt" => Some(SessionStatus::Idle),
            "Stop" => Some(SessionStatus::Idle),
            _ => None,
        }
    }

    /// Its `turn_id`, which an automatic compaction after a `Stop` also carries.
    fn turn_key(&self, payload: &Value) -> Option<String> {
        let key = payload.get("turn_id").and_then(Value::as_str)?;
        (!key.is_empty()).then(|| key.to_string())
    }

    /// Codex (0.163.0) trims whitespace at the beginning and end, preserving internal line
    /// breaks; the prompt is read the same way.
    fn submitted_prompt<'a>(&self, payload: &'a Value) -> Option<&'a str> {
        Some(payload.get("prompt")?.as_str()?.trim())
    }

    /// The only cancellation signal Claude Code, Codex or Grok Build gives. A declined approval
    /// aborts the turn and fires it too.
    fn turn_cancelled(&self, event: &str) -> bool {
        event == "Interrupt"
    }

    fn turn_outcome(&self, event: &str, _payload: &Value) -> Option<TurnOutcome> {
        match event {
            "Interrupt" => Some(TurnOutcome::Abnormal),
            "Stop" => Some(TurnOutcome::Normal),
            _ => None,
        }
    }

    /// A failure comes from the rollout, never from a hook event.
    fn turn_end(&self, event: &str, payload: &Value) -> Option<TurnEnd> {
        (event == "Stop").then(|| TurnEnd {
            last_assistant_message: text_field(payload, "last_assistant_message"),
            failed: false,
            backstop: false,
        })
    }

    fn record_watch<'a>(
        &self,
        event: &str,
        payload: &'a Value,
        _previous: Option<SessionStatus>,
        _status: SessionStatus,
    ) -> Option<RecordWatch<'a>> {
        (event == "UserPromptSubmit").then(|| RecordWatch::Failure {
            record: payload.get("transcript_path").and_then(Value::as_str),
            turn_id: payload.get("turn_id").and_then(Value::as_str),
        })
    }
}
