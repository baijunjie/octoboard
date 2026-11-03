//! Whether Claude Code's transcript shows a turn an account switch is to carry on
//! (`crate::records::switch_continue`, `RecordStore::should_continue`): the last turn entry and
//! what follows it, read from the end of the transcript.

use std::path::Path;

use serde_json::Value;

use crate::records::line_file;

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
pub(crate) const FIRST_WINDOW: u64 = 256 * 1024;

/// The decision on the transcript at `record`, see `RecordStore::should_continue`.
pub(super) fn should_continue(record: &Path, usage_limit_only: bool) -> std::io::Result<bool> {
    claude_tail(record).map(|entries| claude_should_continue(&entries, usage_limit_only))
}

/// The entries of the transcript that bear on the decision, oldest first, from the last user or
/// assistant entry on (or all of them, when the file has none): user, assistant and system entries
/// of the main conversation. Reads the end in growing windows, since a transcript can be tens of MB
/// and only its last turn matters; a line past the shared reader's limit is skipped, not held.
fn claude_tail(record: &Path) -> std::io::Result<Vec<Value>> {
    let len = std::fs::metadata(record)?.len();
    let mut window = FIRST_WINDOW;
    loop {
        let start = len.saturating_sub(window);
        let mut entries: Vec<Value> = Vec::new();
        // The first line the reader hands over is skipped, as it may be the end of a line the
        // window's start cut. Whenever the skipped one is a whole line (the start fell on a line
        // boundary, or the reader already dropped the partial line itself, over its limit or not
        // UTF-8), the verdict stays right: a window with no turn entry after it is widened until
        // it holds one.
        let mut cut = start > 0;
        line_file::read_from(record, start, |line| {
            if std::mem::take(&mut cut) {
                return;
            }
            if let Ok(entry) = serde_json::from_str::<Value>(line) {
                if is_conversation_entry(&entry) {
                    entries.push(entry);
                }
            }
        })?;
        if let Some(last_turn) = entries.iter().rposition(is_turn_entry) {
            return Ok(entries.split_off(last_turn));
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
