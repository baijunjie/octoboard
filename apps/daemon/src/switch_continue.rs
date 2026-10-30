//! Whether a session an account switch relaunches is sent a prompt to carry on with its turn:
//! only when the switch cut a turn off mid-way, or the turn had already stopped on the plan's usage
//! limit — never when the user had cancelled the turn or it had finished. The agent's own
//! auto-resume after a usage limit (Claude Code's "continuing automatically at …") dies with the
//! process the switch ends, so without this nothing would carry on.
//!
//! The decision is read from the agent's conversation record after the process has gone, because
//! hooks do not tell the cases apart: a Claude Code Esc fires none, and neither does a Grok Build
//! Ctrl+C before the turn's first output. For the rules per agent see
//! `docs/product/switching-session-accounts.md`, section "Continuing after a switch".
//!
//! Measured against a local stub API: Claude Code 2.1.310 (and the real API), codex-cli 0.163.0
//! and Grok Build 1.0.70. The records are the agents' own and change between versions, so anything
//! this does not recognise reads as "do not continue": sending a prompt the user did not want is
//! the worse of the two mistakes. The one case none of them can tell apart — a turn ended before
//! any output, which a cancel at that moment leaves looking the same as the switch's ending of it
//! (Claude Code, Grok Build) — is therefore not continued.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde_json::Value;

use crate::line_file;
use crate::protocol::Agent;

/// What a continued session is told to do, in Claude Code's own words for its auto-resume. A macro
/// so that [`CONTINUE_PROMPT`] can be built on it with `concat!`.
macro_rules! continue_words {
    () => {
        "Continue the task you were working on when it was interrupted; do not repeat work that \
         is already complete."
    };
}

/// The prompt a continued session is sent: a notice from Octoboard, so that the session's history
/// tells it from what the user typed.
pub const CONTINUE_PROMPT: &str = concat!(
    crate::reporting::notice_prefix!(),
    ": your session was restarted. ",
    continue_words!()
);

/// What follows a notice the continued session is sent with, in place of [`CONTINUE_PROMPT`].
pub const CONTINUE_AFTER_NOTICE: &str = continue_words!();

/// What Claude Code writes as the user's turn when an Esc interrupts the model, or a tool.
const INTERRUPT_MARKERS: [&str; 2] = [
    "[Request interrupted by user]",
    "[Request interrupted by user for tool use]",
];

/// The start of the `tool_result` text Claude Code gives a tool call the user declined.
const REJECTED_TOOL_USE_PREFIX: &str = "The user doesn't want to proceed with this tool use";

/// The notice Claude Code logs when the user presses Esc on its own usage-limit auto-resume.
const AUTO_CONTINUE_CANCELLED_PREFIX: &str = "Automatic continue cancelled";

/// How much of the end of the transcript is read first; doubled until the last turn entry is in it.
/// A single entry can run to hundreds of KB (a large tool result).
const FIRST_WINDOW: u64 = 256 * 1024;

/// Whether the session whose conversation record is `record` should be sent [`CONTINUE_PROMPT`]
/// once relaunched: the transcript for Claude Code, the head rollout for Codex, the session
/// directory for Grok Build. Read after its process has ended. `false` for a record that cannot be
/// read.
pub fn should_continue(agent: Agent, record: &Path) -> bool {
    verdict(agent, record, false)
}

/// Whether the last turn of the conversation `record` (as for [`should_continue`]) stopped because
/// the plan's usage limit was reached: the one case of [`should_continue`] that is not a turn cut
/// off mid-way, which a running session's record shows all the time. What Octoboard's own account
/// switch is triggered by (`crate::auto_switch`). `false` for a record that cannot be read.
pub fn stopped_on_usage_limit(agent: Agent, record: &Path) -> bool {
    verdict(agent, record, true)
}

/// [`should_continue`], or with `usage_limit_only` the part of it that a usage limit stopped.
fn verdict(agent: Agent, record: &Path, usage_limit_only: bool) -> bool {
    let decided = match agent {
        Agent::Claude => {
            claude_tail(record).map(|entries| claude_should_continue(&entries, usage_limit_only))
        }
        Agent::Codex => codex_should_continue(record, usage_limit_only),
        Agent::Grok => grok_should_continue(record, usage_limit_only),
    };
    decided.unwrap_or_else(|err| {
        tracing::warn!(record = %record.display(), %err, "reading the conversation record failed");
        false
    })
}

/// The entries of the transcript that bear on the decision, oldest first, from the last user or
/// assistant entry on (or all of them, when the file has none): user, assistant and system entries
/// of the main conversation. Reads backwards from the end in growing windows, since a transcript
/// can be tens of MB and only its last turn matters.
fn claude_tail(record: &Path) -> std::io::Result<Vec<Value>> {
    let mut file = File::open(record)?;
    let len = file.metadata()?.len();
    let mut window = FIRST_WINDOW;
    loop {
        let start = len.saturating_sub(window);
        file.seek(SeekFrom::Start(start))?;
        let mut bytes = Vec::with_capacity((len - start) as usize);
        file.read_to_end(&mut bytes)?;
        let mut lines: Vec<&[u8]> = bytes.split(|&byte| byte == b'\n').collect();
        if start > 0 {
            // Cut mid-line.
            lines.remove(0);
        }
        let entries: Vec<Value> = lines
            .into_iter()
            .filter_map(|line| serde_json::from_slice::<Value>(line).ok())
            .filter(is_conversation_entry)
            .collect();
        if let Some(last_turn) = entries.iter().rposition(is_turn_entry) {
            return Ok(entries[last_turn..].to_vec());
        }
        if start == 0 {
            return Ok(entries);
        }
        window = window.saturating_mul(2);
    }
}

/// A user, assistant or system entry of the main conversation. Subagent entries (`isSidechain`)
/// and the bookkeeping entries Claude Code appends (titles, modes, costs, hooks) are not.
fn is_conversation_entry(entry: &Value) -> bool {
    entry["isSidechain"].as_bool() != Some(true)
        && matches!(
            entry["type"].as_str(),
            Some("user" | "assistant" | "system")
        )
}

/// A user or assistant entry, except an interrupt marker Claude Code writes as it shuts down
/// (`interruptedByShutdown`), which says only that the process ended — the switch's own doing.
fn is_turn_entry(entry: &Value) -> bool {
    matches!(entry["type"].as_str(), Some("user" | "assistant"))
        && entry["interruptedByShutdown"].as_bool() != Some(true)
}

/// The decision on the transcript's tail: `entries` from the last turn entry on, oldest first.
fn claude_should_continue(entries: &[Value], usage_limit_only: bool) -> bool {
    let Some((last, after)) = entries.split_first() else {
        return false;
    };
    if !is_turn_entry(last) {
        return false;
    }
    let blocks = content_blocks(last);
    match last["type"].as_str() {
        Some("assistant") => {
            if last["isApiErrorMessage"].as_bool() == Some(true) {
                // A transient 429 carries the same `error`; only `apiError` names the plan's limit.
                // The user pressing Esc on Claude Code's own auto-resume is their word against
                // carrying on.
                return last["error"] == "rate_limit"
                    && last["apiError"] == "usage_limit_reached"
                    && !after.iter().any(cancels_auto_continue);
            }
            if usage_limit_only {
                return false;
            }
            match last["message"]["stop_reason"].as_str() {
                // Waiting on a tool call that never got its result.
                Some("tool_use") => true,
                // Cut off while the model was still writing: the entry is written as its block
                // streams, before any stop reason.
                None => true,
                // `end_turn` is a finished turn; anything else is not recognised.
                Some(_) => false,
            }
        }
        Some("user") => {
            if usage_limit_only || blocks.iter().any(is_interrupt_marker) {
                return false;
            }
            let results: Vec<&Value> = blocks
                .iter()
                .filter(|block| block["type"] == "tool_result")
                .collect();
            // A user entry with no tool result is a prompt (the user's, or a task notification)
            // with nothing after it: either the switch ended the process while the model was
            // still thinking, or the user pressed Esc before any output, which writes no marker.
            // The two cannot be told apart, so it is not continued.
            !results.is_empty()
                && last["toolDenialKind"] != "user-rejected"
                && !results.iter().any(|block| {
                    tool_result_text(block)
                        .is_some_and(|text| text.starts_with(REJECTED_TOOL_USE_PREFIX))
                })
        }
        _ => false,
    }
}

/// The Codex decision on the rollout at `record` (compressed or not): the last turn's ending.
/// Codex records an Esc or a declined approval as `turn_aborted`, every turn that ends otherwise as
/// `task_complete` (with an `error` when it failed), and writes nothing when its process is ended
/// mid-turn. A turn with no ending is continued only when it holds the user's message: a manual
/// `/compact` ended mid-way leaves a turn with none.
fn codex_should_continue(record: &Path, usage_limit_only: bool) -> std::io::Result<bool> {
    // What the last turn has shown so far: `None` before any turn.
    let mut last: Option<CodexTurn> = None;
    line_file::read_to(record, u64::MAX, |line| {
        // Most of a rollout is model traffic; only its events bear on this.
        if !line.contains(r#""type":"event_msg""#) {
            return;
        }
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            return;
        };
        if entry["type"] != "event_msg" {
            return;
        }
        let payload = &entry["payload"];
        match payload["type"].as_str() {
            Some("task_started") => last = Some(CodexTurn::Running { prompted: false }),
            Some("item_completed") if payload["item"]["type"] == "UserMessage" => {
                if let Some(CodexTurn::Running { prompted }) = &mut last {
                    *prompted = true;
                }
            }
            Some("turn_aborted") => last = Some(CodexTurn::Ended { carry_on: false }),
            Some("task_complete") => {
                // The `error` is a string for most failures but an object for some, so only the
                // one string is matched.
                let carry_on = payload["error"]["codex_error_info"] == "usage_limit_exceeded";
                last = Some(CodexTurn::Ended { carry_on });
            }
            _ => {}
        }
    })?;
    Ok(match last {
        Some(CodexTurn::Running { prompted }) => prompted && !usage_limit_only,
        Some(CodexTurn::Ended { carry_on }) => carry_on,
        None => false,
    })
}

enum CodexTurn {
    Running { prompted: bool },
    Ended { carry_on: bool },
}

/// The Grok Build decision on the session directory at `record`, read from its `updates.jsonl`:
/// the last prompt's `turn_completed`, or, with none, whether the turn had shown any output.
///
/// Grok writes no ending for a turn its process is ended in, and none either for a Ctrl+C before
/// the turn's first output (it puts the prompt back in the input box instead), so a turn with no
/// output is not continued. Once there is output, a Ctrl+C or a refused permission is
/// `stop_reason: "cancelled"`. A usage limit is an `error` whose text carries the HTTP status 402,
/// the only thing that tells it from other request errors; a 429 is `rate_limit`, not continued.
fn grok_should_continue(record: &Path, usage_limit_only: bool) -> std::io::Result<bool> {
    // What the last prompt's turn has shown so far: `None` before any prompt.
    let mut last: Option<GrokTurn> = None;
    line_file::read_to(&record.join("updates.jsonl"), u64::MAX, |line| {
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let update = &entry["params"]["update"];
        let Some(kind) = update["sessionUpdate"].as_str() else {
            return;
        };
        match (kind, &last) {
            ("user_message_chunk", _) => last = Some(GrokTurn::Prompted),
            (
                "agent_thought_chunk" | "agent_message_chunk" | "tool_call" | "tool_call_update",
                Some(GrokTurn::Prompted),
            ) => last = Some(GrokTurn::Working),
            ("turn_completed", Some(GrokTurn::Prompted | GrokTurn::Working)) => {
                let carry_on = update["stop_reason"] == "error"
                    && update["agent_result"]
                        .as_str()
                        .is_some_and(|text| text.contains("status 402"));
                last = Some(GrokTurn::Ended { carry_on });
            }
            _ => {}
        }
    })?;
    Ok(match last {
        Some(GrokTurn::Working) => !usage_limit_only,
        Some(GrokTurn::Ended { carry_on }) => carry_on,
        Some(GrokTurn::Prompted) | None => false,
    })
}

enum GrokTurn {
    /// The prompt, and nothing after it yet.
    Prompted,
    /// Output after the prompt, and no ending.
    Working,
    Ended {
        carry_on: bool,
    },
}

/// The content blocks of an entry's message; a plain string content (a typed prompt) has none.
fn content_blocks(entry: &Value) -> &[Value] {
    entry["message"]["content"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn is_interrupt_marker(block: &Value) -> bool {
    block["type"] == "text"
        && block["text"]
            .as_str()
            .is_some_and(|text| INTERRUPT_MARKERS.contains(&text))
}

fn cancels_auto_continue(entry: &Value) -> bool {
    entry["type"] == "system"
        && entry["content"]
            .as_str()
            .is_some_and(|text| text.starts_with(AUTO_CONTINUE_CANCELLED_PREFIX))
}

/// A `tool_result` block's text: a plain string, or text blocks.
fn tool_result_text(block: &Value) -> Option<&str> {
    match &block["content"] {
        Value::String(text) => Some(text),
        Value::Array(parts) => parts.iter().find_map(|part| part["text"].as_str()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;
    use serde_json::json;

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
        should_continue(Agent::Claude, &path)
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
    fn an_empty_or_missing_transcript_is_not_continued() {
        assert!(!decide(&[]));
        assert!(!should_continue(
            Agent::Claude,
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
        should_continue(Agent::Codex, &path)
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
        should_continue(Agent::Grok, &dir)
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
            Agent::Claude,
            &write("claude-cut.jsonl", &[prompt("run it"), tool_use()]),
            true,
        );
        both(
            Agent::Claude,
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
            Agent::Codex,
            &codex(&[task_started(), user_message()]),
            true,
        );
        let usage = json!({"codex_error_info": "usage_limit_exceeded"});
        both(
            Agent::Codex,
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
            Agent::Grok,
            &grok_record(&[grok("user_message_chunk"), grok("tool_call")]),
            true,
        );
        both(
            Agent::Grok,
            &grok_record(&[
                grok("user_message_chunk"),
                turn_completed("error", Some("API error (status 402 Payment Required)")),
            ]),
            false,
        );
    }
}
