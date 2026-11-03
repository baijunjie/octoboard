//! Whether a session an account switch relaunches is sent a prompt to carry on with its turn:
//! only when the switch cut a turn off mid-way, or the turn had already stopped on the plan's usage
//! limit — never when the user had cancelled the turn or it had finished. The agent's own
//! auto-resume after a usage limit (Claude Code's "continuing automatically at …") dies with the
//! process the switch ends, so without this nothing would carry on.
//!
//! The decision is read from the agent's conversation record after the process has gone, because
//! hooks do not tell the cases apart: a Claude Code Esc fires none, and neither does a Grok Build
//! Ctrl+C before the turn's first output. How a provider's record says it is its record store's
//! (`RecordStore::should_continue`). For the rules per agent see
//! `docs/product/switching-session-accounts.md`, section "Continuing after a switch".
//!
//! Measured against a local stub API: Claude Code 2.1.310 (and the real API), codex-cli 0.163.0
//! and Grok Build 1.0.70. The records are the agents' own and change between versions, so anything
//! this does not recognise reads as "do not continue": sending a prompt the user did not want is
//! the worse of the two mistakes. The one case none of them can tell apart — a turn ended before
//! any output, which a cancel at that moment leaves looking the same as the switch's ending of it
//! (Claude Code, Grok Build) — is therefore not continued.

use std::path::Path;

use crate::adapter::provider;
use crate::protocol::AgentId;

/// Whether the session whose conversation record is `record` should be sent
/// [`crate::messaging::texts::CONTINUE_PROMPT`] once relaunched: the transcript for Claude Code,
/// the head rollout for Codex, the session directory for Grok Build. Read after its process has
/// ended. `false` for a record that cannot be read.
pub fn should_continue(agent: AgentId, record: &Path) -> bool {
    verdict(agent, record, false)
}

/// Whether the last turn of the conversation `record` (as for [`should_continue`]) stopped because
/// the plan's usage limit was reached: the one case of [`should_continue`] that is not a turn cut
/// off mid-way, which a running session's record shows all the time. What Octoboard's own account
/// switch is triggered by (`crate::coordinator::auto_switch`). `false` for a record that cannot be
/// read.
pub fn stopped_on_usage_limit(agent: AgentId, record: &Path) -> bool {
    verdict(agent, record, true)
}

/// [`should_continue`], or with `usage_limit_only` the part of it that a usage limit stopped, as
/// the provider's store reads the record (`RecordStore::should_continue`).
fn verdict(agent: AgentId, record: &Path, usage_limit_only: bool) -> bool {
    let decided = provider(agent)
        .records()
        .should_continue(record, usage_limit_only);
    decided.unwrap_or_else(|err| {
        tracing::warn!(record = %record.display(), %err, "reading the conversation record failed");
        false
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::claude::switch_continue::FIRST_WINDOW;
    use crate::records::line_file;
    use crate::test_support::ScratchDir;
    use serde_json::{json, Value};

    fn prompt(text: &str) -> Value {
        json!({"type": "user", "isSidechain": false, "message": {"role": "user", "content": text}})
    }

    fn assistant(stop_reason: Option<&str>, content: Value) -> Value {
        json!({"type": "assistant", "isSidechain": false,
               "message": {"role": "assistant", "content": content, "stop_reason": stop_reason}})
    }

    fn tool_use() -> Value {
        assistant(
            Some("tool_use"),
            json!([{"type": "tool_use", "id": "toolu_1", "name": "Bash", "input": {}}]),
        )
    }

    fn tool_result(is_error: bool, text: &str) -> Value {
        json!({"type": "user", "isSidechain": false, "message": {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "toolu_1", "is_error": is_error, "content": text}
        ]}})
    }

    fn interrupt(marker: &str) -> Value {
        json!({"type": "user", "isSidechain": false, "interruptedMessageId": "msg_1",
               "message": {"role": "user", "content": [{"type": "text", "text": marker}]}})
    }

    /// The error entry Claude Code 2.1.310 writes for a usage limit, trimmed.
    fn usage_limit() -> Value {
        json!({"type": "assistant", "isSidechain": false, "isApiErrorMessage": true,
               "error": "rate_limit", "apiError": "usage_limit_reached",
               "message": {"role": "assistant", "stop_reason": "stop_sequence",
                           "content": [{"type": "text", "text": "You've hit your session limit"}]}})
    }

    fn notice(text: &str) -> Value {
        json!({"type": "system", "isSidechain": false, "subtype": "informational",
               "level": "notice", "content": text})
    }

    fn turn_duration() -> Value {
        json!({"type": "system", "isSidechain": false, "subtype": "turn_duration"})
    }

    /// Writes `entries` as a transcript, with the bookkeeping lines Claude Code appends at exit.
    fn decide(entries: &[Value]) -> bool {
        let dir = ScratchDir::new("switch-continue");
        let path = dir.join("t.jsonl");
        let mut text: String = entries.iter().map(|entry| format!("{entry}\n")).collect();
        text.push_str("{\"type\":\"last-prompt\",\"lastPrompt\":\"x\"}\n");
        text.push_str("{\"type\":\"permission-mode\",\"permissionMode\":\"default\"}\n");
        std::fs::write(&path, text).unwrap();
        should_continue(AgentId::CLAUDE, &path)
    }

    #[test]
    fn an_esc_while_the_model_writes_is_not_continued() {
        let written = assistant(None, json!([{"type": "text", "text": "word0 word1"}]));
        assert!(!decide(&[
            prompt("write a lot"),
            written,
            interrupt("[Request interrupted by user]")
        ]));
    }

    #[test]
    fn an_esc_on_a_running_tool_is_not_continued() {
        let mut rejected = tool_result(true, "User rejected tool use");
        rejected["toolDenialKind"] = json!("user-rejected");
        assert!(!decide(&[
            prompt("run it"),
            tool_use(),
            rejected,
            interrupt("[Request interrupted by user for tool use]")
        ]));
    }

    #[test]
    fn a_declined_prompt_is_not_continued() {
        let mut rejected = tool_result(true, "User rejected tool use");
        rejected["toolDenialKind"] = json!("user-rejected");
        assert!(!decide(&[prompt("run it"), tool_use(), rejected]));
        let declined = tool_result(
            true,
            "The user doesn't want to proceed with this tool use. The tool use was rejected",
        );
        assert!(!decide(&[prompt("run it"), tool_use(), declined]));
    }

    #[test]
    fn a_usage_limit_is_continued() {
        assert!(decide(&[
            prompt("hello"),
            usage_limit(),
            notice("Usage limit reached · continuing automatically at 11:44pm · esc to cancel"),
            turn_duration()
        ]));
    }

    #[test]
    fn a_usage_limit_whose_auto_continue_was_cancelled_is_not_continued() {
        assert!(!decide(&[
            prompt("hello"),
            usage_limit(),
            notice("Usage limit reached · continuing automatically at 11:44pm · esc to cancel"),
            turn_duration(),
            notice("Automatic continue cancelled · /rate-limit-options to re-arm")
        ]));
    }

    #[test]
    fn other_api_errors_are_not_continued() {
        let mut transient = usage_limit();
        transient.as_object_mut().unwrap().remove("apiError");
        assert!(!decide(&[prompt("hello"), transient, turn_duration()]));
        let mut overloaded = usage_limit();
        overloaded["error"] = json!("overloaded");
        overloaded.as_object_mut().unwrap().remove("apiError");
        assert!(!decide(&[prompt("hello"), overloaded]));
    }

    #[test]
    fn a_finished_turn_is_not_continued() {
        let done = assistant(Some("end_turn"), json!([{"type": "text", "text": "Done."}]));
        assert!(!decide(&[prompt("hello"), done, turn_duration()]));
    }

    #[test]
    fn a_turn_cut_off_on_a_tool_call_is_continued() {
        assert!(decide(&[prompt("run it"), tool_use()]));
    }

    #[test]
    fn a_turn_cut_off_after_a_tool_result_is_continued() {
        assert!(decide(&[
            prompt("run it"),
            tool_use(),
            tool_result(false, "ok")
        ]));
        // A tool that failed on its own is still part of a running turn.
        assert!(decide(&[
            prompt("run it"),
            tool_use(),
            tool_result(true, "exit code 1")
        ]));
    }

    #[test]
    fn a_turn_cut_off_while_the_model_writes_is_continued() {
        let written = assistant(None, json!([{"type": "text", "text": "word0 word1"}]));
        assert!(decide(&[prompt("write a lot"), written]));
    }

    #[test]
    fn a_tail_that_is_the_prompt_is_not_continued() {
        assert!(!decide(&[prompt("think hard")]));
        let done = assistant(Some("end_turn"), json!([{"type": "text", "text": "Done."}]));
        assert!(!decide(&[
            prompt("hello"),
            done,
            turn_duration(),
            prompt("think hard")
        ]));
    }

    #[test]
    fn a_shutdown_interrupt_is_passed_over() {
        let mut shutdown = interrupt("[Request interrupted by user for tool use]");
        shutdown["interruptedByShutdown"] = json!(true);
        assert!(decide(&[prompt("run it"), tool_use(), shutdown]));
    }

    #[test]
    fn subagent_entries_are_passed_over() {
        let mut sidechain = assistant(Some("end_turn"), json!([{"type": "text", "text": "sub"}]));
        sidechain["isSidechain"] = json!(true);
        assert!(decide(&[prompt("run it"), tool_use(), sidechain]));
    }

    #[test]
    fn a_tail_beyond_the_first_window_is_found() {
        let big = "x".repeat(FIRST_WINDOW as usize * 3);
        let filler =
            json!({"type": "system", "isSidechain": false, "subtype": "x", "content": big});
        assert!(decide(&[prompt("run it"), tool_use(), filler]));
    }

    #[test]
    fn a_line_past_the_reader_limit_is_skipped() {
        let big = "x".repeat(line_file::LINE_LIMIT + 1);
        let oversized = assistant(Some("end_turn"), json!([{"type": "text", "text": big}]));
        // Were the oversized line held, the turn would have finished on it and not be continued;
        // skipped, the turn stays cut off on the tool call before it.
        assert!(decide(&[prompt("run it"), tool_use(), oversized]));
    }

    #[test]
    fn an_empty_or_missing_transcript_is_not_continued() {
        assert!(!decide(&[]));
        assert!(!should_continue(
            AgentId::CLAUDE,
            Path::new("/nonexistent/switch-continue.jsonl")
        ));
    }

    /// A Codex rollout of `events` (payloads of `event_msg` lines), with a model message between
    /// each, as Codex 0.163.0 writes them; compressed when `compressed`.
    fn codex_decides(events: &[Value], compressed: bool) -> bool {
        let dir = ScratchDir::new("switch-continue-codex");
        let mut text = String::from("{\"type\":\"session_meta\",\"payload\":{\"id\":\"t\"}}\n");
        for event in events {
            text.push_str(&format!(
                "{}\n",
                json!({"type": "event_msg", "payload": event})
            ));
            text.push_str("{\"type\":\"response_item\",\"payload\":{\"type\":\"message\"}}\n");
        }
        let path = if compressed {
            let path = dir.join("rollout.jsonl.zst");
            std::fs::write(&path, zstd::encode_all(text.as_bytes(), 0).unwrap()).unwrap();
            path
        } else {
            let path = dir.join("rollout.jsonl");
            std::fs::write(&path, text).unwrap();
            path
        };
        should_continue(AgentId::CODEX, &path)
    }

    fn task_started() -> Value {
        json!({"type": "task_started", "turn_id": "turn-1"})
    }

    fn user_message() -> Value {
        json!({"type": "item_completed", "turn_id": "turn-1", "item": {"type": "UserMessage"}})
    }

    fn task_complete(error: Option<Value>) -> Value {
        let mut event = json!({"type": "task_complete", "turn_id": "turn-1",
                               "last_agent_message": null});
        if let Some(error) = error {
            event["error"] = error;
        }
        event
    }

    #[test]
    fn a_codex_turn_cut_off_is_continued() {
        assert!(codex_decides(&[task_started(), user_message()], false));
        assert!(codex_decides(&[task_started(), user_message()], true));
    }

    #[test]
    fn a_codex_cancel_is_not_continued() {
        let aborted = json!({"type": "turn_aborted", "turn_id": "turn-1", "reason": "interrupted"});
        assert!(!codex_decides(
            &[task_started(), user_message(), aborted],
            false
        ));
    }

    #[test]
    fn a_codex_usage_limit_is_continued_and_other_failures_are_not() {
        let usage = json!({"message": "You've hit your usage limit.",
                           "codex_error_info": "usage_limit_exceeded"});
        assert!(codex_decides(
            &[task_started(), user_message(), task_complete(Some(usage))],
            false
        ));
        for other in [
            json!({"message": "x", "codex_error_info": "server_overloaded"}),
            json!({"message": "x", "codex_error_info":
                   {"response_too_many_failed_attempts": {"http_status_code": 429}}}),
        ] {
            assert!(!codex_decides(
                &[task_started(), user_message(), task_complete(Some(other))],
                false
            ));
        }
    }

    #[test]
    fn a_finished_codex_turn_is_not_continued() {
        assert!(!codex_decides(
            &[task_started(), user_message(), task_complete(None)],
            false
        ));
        // An earlier failure does not carry over to a later turn cut off.
        let usage = json!({"codex_error_info": "usage_limit_exceeded"});
        assert!(!codex_decides(
            &[
                task_started(),
                user_message(),
                task_complete(Some(usage)),
                task_complete(None)
            ],
            false
        ));
    }

    #[test]
    fn a_codex_turn_with_no_user_message_is_not_continued() {
        // A manual `/compact` ended mid-way.
        let compaction = json!({"type": "item_completed", "item": {"type": "ContextCompaction"}});
        assert!(!codex_decides(
            &[
                task_started(),
                user_message(),
                task_complete(None),
                task_started(),
                compaction
            ],
            false
        ));
        assert!(!codex_decides(&[], false));
    }

    /// A Grok Build session directory whose `updates.jsonl` holds `updates`, as Grok 1.0.70 writes
    /// them, with the hook records it writes between them.
    fn grok_decides(updates: &[Value]) -> bool {
        let dir = ScratchDir::new("switch-continue-grok");
        let mut text = String::new();
        for update in updates {
            text.push_str(&format!(
                "{}\n",
                json!({"method": "session/update", "params": {"sessionId": "g", "update": update}})
            ));
            text.push_str(&format!(
                "{}\n",
                json!({"method": "_x.ai/session/update",
                       "params": {"update": {"sessionUpdate": "hook_execution"}}})
            ));
        }
        std::fs::write(dir.join("updates.jsonl"), text).unwrap();
        should_continue(AgentId::GROK, &dir)
    }

    fn grok(kind: &str) -> Value {
        json!({"sessionUpdate": kind})
    }

    fn turn_completed(stop_reason: &str, agent_result: Option<&str>) -> Value {
        let mut update = json!({"sessionUpdate": "turn_completed", "prompt_id": "p",
                                "stop_reason": stop_reason});
        if let Some(text) = agent_result {
            update["agent_result"] = json!(text);
        }
        update
    }

    #[test]
    fn a_grok_turn_cut_off_after_output_is_continued() {
        for output in ["agent_thought_chunk", "agent_message_chunk", "tool_call"] {
            assert!(
                grok_decides(&[grok("user_message_chunk"), grok(output)]),
                "{output}"
            );
        }
    }

    #[test]
    fn a_grok_turn_with_no_output_is_not_continued() {
        // A Ctrl+C before the first output leaves exactly this.
        assert!(!grok_decides(&[grok("user_message_chunk")]));
        assert!(!grok_decides(&[
            grok("user_message_chunk"),
            grok("agent_message_chunk"),
            turn_completed("end_turn", None),
            grok("user_message_chunk")
        ]));
    }

    #[test]
    fn a_grok_cancel_or_finished_turn_is_not_continued() {
        for stop_reason in ["cancelled", "end_turn", "rate_limit"] {
            assert!(
                !grok_decides(&[
                    grok("user_message_chunk"),
                    grok("agent_message_chunk"),
                    turn_completed(stop_reason, None)
                ]),
                "{stop_reason}"
            );
        }
    }

    #[test]
    fn a_grok_usage_limit_is_continued_and_other_errors_are_not() {
        let usage = "API error (status 402 Payment Required): Grok Build usage balance exhausted";
        assert!(grok_decides(&[
            grok("user_message_chunk"),
            grok("retry_state"),
            turn_completed("error", Some(usage))
        ]));
        let bad = "API error (status 400 Bad Request): invalid";
        assert!(!grok_decides(&[
            grok("user_message_chunk"),
            turn_completed("error", Some(bad))
        ]));
    }

    #[test]
    fn a_grok_background_turn_after_a_finished_one_is_not_continued() {
        // A turn a background task started has no prompt of its own.
        assert!(!grok_decides(&[
            grok("user_message_chunk"),
            turn_completed("end_turn", None),
            grok("task_completed"),
            grok("agent_message_chunk")
        ]));
    }

    /// A turn cut off mid-way is something to continue but not a stop on the usage limit, which a
    /// running session's record shows all the time; only the limit is the trigger of the
    /// automatic switch.
    #[test]
    fn only_a_usage_limit_is_a_stop_on_the_limit() {
        let dir = ScratchDir::new("switch-continue-limit");
        let write = |name: &str, lines: &[Value]| {
            let path = dir.join(name);
            let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
            std::fs::write(&path, text).unwrap();
            path
        };
        let both = |agent, record: &Path, cut_off: bool| {
            assert!(should_continue(agent, record));
            assert_eq!(stopped_on_usage_limit(agent, record), !cut_off);
        };

        both(
            AgentId::CLAUDE,
            &write("claude-cut.jsonl", &[prompt("run it"), tool_use()]),
            true,
        );
        both(
            AgentId::CLAUDE,
            &write("claude-limit.jsonl", &[prompt("hello"), usage_limit()]),
            false,
        );

        let codex = |events: &[Value]| {
            let lines: Vec<Value> = events
                .iter()
                .map(|event| json!({"type": "event_msg", "payload": event}))
                .collect();
            write("rollout.jsonl", &lines)
        };
        both(
            AgentId::CODEX,
            &codex(&[task_started(), user_message()]),
            true,
        );
        let usage = json!({"codex_error_info": "usage_limit_exceeded"});
        both(
            AgentId::CODEX,
            &codex(&[task_started(), user_message(), task_complete(Some(usage))]),
            false,
        );

        let grok_record = |updates: &[Value]| {
            let lines: Vec<Value> = updates
                .iter()
                .map(|update| json!({"params": {"update": update}}))
                .collect();
            write("updates.jsonl", &lines);
            dir.to_path_buf()
        };
        both(
            AgentId::GROK,
            &grok_record(&[grok("user_message_chunk"), grok("tool_call")]),
            true,
        );
        both(
            AgentId::GROK,
            &grok_record(&[
                grok("user_message_chunk"),
                turn_completed("error", Some("API error (status 402 Payment Required)")),
            ]),
            false,
        );
    }
}
