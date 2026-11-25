//! How Codex's hook events are read.
//!
//! `Interrupt` replaces `Stop` for a cancelled turn, and `PermissionRequest` fires before the modal
//! reaches the user — in the same instant as the same call's `PreToolUse`, and the two are injected
//! as asynchronous hooks, so they arrive in either order; that is why no status is read from a
//! tool call's pre-event, a question's excepted (it raises no `PermissionRequest`, see
//! [`CodexHooks::status`]). A turn that ends in an error fires neither `Stop` nor `Interrupt`: only its
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
    /// Codex runs no hook around its trust confirmation, and none before the first prompt.
    fn past_startup(&self, _event: &str, _payload: &Value) -> bool {
        true
    }

    fn starts_conversation(&self, event: &str, _payload: &Value) -> bool {
        event == "UserPromptSubmit"
    }

    fn edit_targets(&self, payload: &Value) -> (Vec<String>, Option<std::path::PathBuf>) {
        let cwd = payload
            .get("cwd")
            .and_then(Value::as_str)
            .map(std::path::PathBuf::from);
        let input = payload.get("tool_input");
        let Some(input) = input else {
            return (Vec::new(), cwd);
        };
        let paths = input
            .get("command")
            .and_then(Value::as_str)
            .map(|patch| {
                const HEADERS: [&str; 4] = [
                    "*** Update File: ",
                    "*** Add File: ",
                    "*** Delete File: ",
                    "*** Move to: ",
                ];
                patch
                    .lines()
                    .filter_map(|line| {
                        HEADERS
                            .iter()
                            .find_map(|header| line.trim_start().strip_prefix(header))
                    })
                    .map(str::trim)
                    .filter(|path| !path.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        (paths, cwd)
    }

    fn deny_edit(&self, reason: &str) -> Option<Value> {
        Some(serde_json::json!({"hookSpecificOutput": {
            "hookEventName": "PreToolUse", "permissionDecision": "deny", "permissionDecisionReason": reason
        }}))
    }

    /// Codex runs no hook around its trust confirmation, and defers `SessionStart` to the first
    /// prompt submission (see [`Self::status`]), which still comes only once the session is past
    /// its start-up screens.
    fn announces_session_start(&self, event: &str, _payload: &Value) -> bool {
        event == "SessionStart"
    }

    fn status(&self, event: &str, payload: &Value) -> Option<SessionStatus> {
        match event {
            // No `SessionStart` arm. Measured for its `startup` source only: Codex defers it to the
            // first prompt submission, so it arrives together with that prompt's `UserPromptSubmit`
            // and, handled last, would put a working first turn back to idle. The launch and the
            // resume already set `Idle` themselves. The other sources (`resume`, `clear`,
            // `compact`, `fork`) are unmeasured. This gives up what a `clear` or `fork` one
            // mid-session may have done, correcting a session stuck at working; a `compact` one
            // reporting `Idle` would have been wrong anyway.
            //
            // `PreToolUse` reports nothing for a tool call, and for a different reason than
            // `SessionStart` is left out: it is in a race with the same call's
            // `PermissionRequest` (the module header). A `Working` landing after that request's
            // `WaitingUser` would lower the hand for as long as the modal is up and release
            // whatever is queued for the session into it, where the paste's trailing Enter
            // answers it. Nothing is lost: the turn's own `UserPromptSubmit` has already reported
            // work, and an approval that is granted is followed by the call's `PostToolUse`.
            //
            // The exception is `request_user_input`, which raises no `PermissionRequest`
            // (0.167.1), so its `PreToolUse` is the only sign that the session waits on the user
            // (a Plan-mode question; its `PostToolUse` carries the answers). The same call in
            // Default mode is rejected at once (`PreToolUse`, no `PostToolUse`, then `Stop`), and
            // the payloads cannot tell the modes apart, so the hand is raised there too until the
            // turn's next event lowers it. The call is not tracked as waiting
            // (`HookInterpreter::tool_call_change`), so that the next `Working` of the turn
            // lowers the hand. The accepted costs: another call of the turn that ends while a
            // Plan-mode question is up lowers it too (unmeasured whether Codex runs calls beside
            // a question); and a Default-mode call Codex rejects raises the hand briefly, so a
            // session bound to an owner may send the owner a false "waiting on the user" notice
            // (`tell_owner_it_waits`, at most once per turn: `claim_waiting_notice`).
            "PreToolUse" if is_question(payload) => Some(SessionStatus::WaitingUser),
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

    fn is_subagent(&self, payload: &Value) -> bool {
        crate::hooks::is_named_subagent(payload)
    }

    fn agent_session_id(&self, payload: &Value) -> Option<String> {
        crate::hooks::agent_session_id(payload)
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

/// Whether the payload is a call of Plan mode's question tool.
fn is_question(payload: &Value) -> bool {
    payload.get("tool_name").and_then(Value::as_str) == Some(super::interaction::ASK_TOOL)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real payloads of Codex 0.167.1 (`testdata/codex-interaction-captures.md`).
    fn payloads() -> Vec<(String, Value)> {
        include_str!("../../../testdata/codex_interaction_hooks.jsonl")
            .lines()
            .map(|line| {
                let line: Value = serde_json::from_str(line).unwrap();
                (
                    line["event"].as_str().unwrap().to_string(),
                    line["payload"].clone(),
                )
            })
            .collect()
    }

    fn of_tool<'a>(payloads: &'a [(String, Value)], event: &str, tool: &str) -> &'a Value {
        &payloads
            .iter()
            .find(|(e, p)| e == event && p["tool_name"] == tool)
            .unwrap_or_else(|| panic!("{event} {tool}"))
            .1
    }

    /// The call of the fixture's `PreToolUse` and `PostToolUse` of the question asked in Plan mode.
    fn plan_question(payloads: &[(String, Value)], event: &str) -> Value {
        payloads
            .iter()
            .find(|(e, p)| {
                e == event
                    && p["tool_name"] == "request_user_input"
                    && p["tool_use_id"] == "call_resp_1795104813291"
            })
            .unwrap_or_else(|| panic!("{event}"))
            .1
            .clone()
    }

    #[test]
    fn a_question_waits_on_the_user_from_its_pre_tool_use_to_its_post_tool_use() {
        let payloads = payloads();
        let pre = plan_question(&payloads, "PreToolUse");
        let post = plan_question(&payloads, "PostToolUse");
        assert_eq!(pre["tool_use_id"], post["tool_use_id"]);
        assert_eq!(
            CodexHooks.status("PreToolUse", &pre),
            Some(SessionStatus::WaitingUser)
        );
        assert_eq!(
            CodexHooks.status("PostToolUse", &post),
            Some(SessionStatus::Working)
        );
        // Not tracked as waiting: the turn's next `Working` lowers the hand.
        assert!(CodexHooks.tool_call_change("PreToolUse", &pre).is_none());
    }

    #[test]
    fn no_other_tool_reports_from_its_pre_tool_use() {
        let payloads = payloads();
        for tool in [
            "Bash",
            "apply_patch",
            "mcp__stubmcp__probe_tool",
            "request_user_input_async",
        ] {
            let pre = of_tool(&payloads, "PreToolUse", tool);
            assert_eq!(CodexHooks.status("PreToolUse", pre), None, "{tool}");
        }
        let request = of_tool(&payloads, "PermissionRequest", "Bash");
        assert_eq!(
            CodexHooks.status("PermissionRequest", request),
            Some(SessionStatus::WaitingUser)
        );
    }
}
