//! How a Codex rollout reads as the live conversation feed (`crate::records::feed`).
//!
//! What was said, and the plan the model keeps, are the common entries
//! ([`super::conversation::entries`]). Beyond them the rollout holds each tool call twice over,
//! in two shapes that are told apart by the id:
//!
//! - an `item_completed` event of a `CommandExecution`, `FileChange`, `McpToolCall`,
//!   `CollabAgentToolCall` or `ImageView` item, written when the call is over and so already
//!   carrying how it ended;
//! - a `function_call` or `custom_tool_call` response item, written when the model calls the tool,
//!   with its `*_output` item after it. This is how the calls that are no item of their own appear
//!   (code-mode `exec`, `spawn_agent`, `wait`, ...). A few of them also have an item whose id is
//!   the call's id (`wait_agent`'s `CollabAgentToolCall`); the item comes after the call and
//!   replaces its entry, and the output of a call an item settled is not applied over it.
//!
//! The rollout has no item for a call that is still running and no text before a message is
//! complete, so nothing here is pending. A call that no output has come for is settled by the
//! end of its turn: failed by `turn_aborted` or a `task_complete` that carries an error, and
//! completed by the next `task_started` or a clean `task_complete` (the turn ended and the
//! rollout holds no mark that the call went wrong).
//!
//! Checked against real rollouts (0.163.0 to 0.167.1, `testdata/record-captures.md`): the
//! `item_completed` items, `exec`, `wait`, the collaboration calls, `sleep`,
//! `request_user_input_async` and `compaction` of the current shape. Not checked against a real
//! line: the `shell`, `exec_command` and `shell_command` calls of older Codex (their `command`
//! or `cmd` argument is read as the existing captures and the documentation have it), a `js`
//! call, a `FileChange` that was declined, and an item whose `status` is none of `completed`,
//! `failed` and `declined`.

use std::collections::HashSet;

use serde_json::{json, Value};

use crate::protocol::{ToolCategory, ToolStatus};
use crate::records::conversation::{joined_text, text_of, Entry};
use crate::records::feed::{at_of, FeedOut, FeedReader, ToolCall, ToolOutcome};
use crate::records::RecordStore;

use super::conversation::{change_paths, entries, patch_paths};
use super::records::CodexRecords;

/// The text kinds of the content items a tool's output is a list of.
const OUTPUT_TEXT_KINDS: &[&str] = &["input_text", "output_text", "text"];

/// Reads one pass over a Codex rollout.
#[derive(Default)]
pub(super) struct CodexFeedReader {
    /// Ids of the calls made by a response item that no output has come for.
    awaiting: Vec<String>,
    /// Ids of the calls an item settled.
    settled: HashSet<String>,
    /// Ids of every call a response item made.
    calls: HashSet<String>,
    /// How many entries there were after the last compaction, so one that the rollout records
    /// more than once in a row is shown once.
    after_compaction: Option<usize>,
    /// Counts the items that carry no id, to give each its own.
    anonymous: usize,
}

impl FeedReader for CodexFeedReader {
    fn line(&mut self, line: &Value, out: &mut FeedOut) {
        let at = at_of(line);
        // `request_user_input_async` also writes its questions as an `AgentMessage` item with the
        // call's id; the call is kept, since it carries the answer.
        let question = line
            .get("payload")
            .and_then(|payload| payload.get("item"))
            .filter(|item| item.get("type").and_then(Value::as_str) == Some("AgentMessage"))
            .and_then(|item| text_of(item, "id"))
            .is_some_and(|id| self.calls.contains(&id));
        for entry in entries(line) {
            match entry {
                Entry::User(text) => {
                    if let Some(text) = CodexRecords.user_text(&text) {
                        out.user(&text, at);
                    }
                }
                Entry::Assistant(_) if question => {}
                Entry::Assistant(text) => out.assistant(&text, at),
                Entry::Todos { items, merge } => out.todos(items, merge, at),
                Entry::Summary(_) | Entry::Changed(_) => {}
            }
        }
        let Some(payload) = line.get("payload") else {
            return;
        };
        let kind = payload.get("type").and_then(Value::as_str);
        match (line.get("type").and_then(Value::as_str), kind) {
            // A compaction is written as a `compacted` line (whose history is encrypted) and a
            // `ContextCompaction` item, or as a `compaction` response item, depending on how
            // Codex compacted. The line and the response item are read, the item is not: the
            // response item and the line never share a rollout in the ones inspected, and
            // reading the item as well would count the first kind twice.
            (Some("compacted"), _) | (Some("response_item"), Some("compaction")) => {
                self.compaction(out, at)
            }
            (Some("event_msg"), Some("item_completed")) => {
                if let Some(item) = payload.get("item") {
                    self.item(item, out, at);
                }
            }
            // The turn ended before the calls it was waiting on gave anything back, and the rollout
            // never says more of them.
            (Some("event_msg"), Some("turn_aborted")) => self.settle(ToolStatus::Failed, out),
            // A turn that ends or is followed by another without an error mark ended the calls
            // that gave nothing back; one that ends in an error failed them.
            (Some("event_msg"), Some("task_started")) => self.settle(ToolStatus::Completed, out),
            (Some("event_msg"), Some("task_complete")) => {
                let failed = payload.get("error").is_some_and(|error| !error.is_null());
                self.settle(
                    if failed {
                        ToolStatus::Failed
                    } else {
                        ToolStatus::Completed
                    },
                    out,
                )
            }
            (Some("response_item"), Some("function_call")) => {
                self.call(payload, "arguments", out, at)
            }
            (Some("response_item"), Some("custom_tool_call")) => {
                self.call(payload, "input", out, at)
            }
            (Some("response_item"), Some("function_call_output" | "custom_tool_call_output")) => {
                self.output(payload, out)
            }
            _ => {}
        }
    }
}

impl CodexFeedReader {
    /// Ends the calls still waiting for an output as `status`, with no output.
    fn settle(&mut self, status: ToolStatus, out: &mut FeedOut) {
        for id in std::mem::take(&mut self.awaiting) {
            out.tool_result(
                &id,
                ToolOutcome {
                    status,
                    output: None,
                    exit_code: None,
                },
            );
        }
    }

    fn compaction(&mut self, out: &mut FeedOut, at: Option<i64>) {
        if self.after_compaction == Some(out.len()) {
            return;
        }
        out.compaction(None, at);
        self.after_compaction = Some(out.len());
    }

    /// A call a response item makes, which stays running until its output.
    fn call(&mut self, payload: &Value, field: &str, out: &mut FeedOut, at: Option<i64>) {
        let (Some(name), Some(call_id)) = (text_of(payload, "name"), text_of(payload, "call_id"))
        else {
            return;
        };
        // The plan is a todo list, which the common entries read.
        if name == "update_plan" {
            return;
        }
        let input = match payload.get(field) {
            Some(Value::String(text)) if field == "arguments" => {
                serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.clone()))
            }
            Some(other) => other.clone(),
            None => Value::Null,
        };
        let category = tool_category(&name);
        let (title, paths) = match name.as_str() {
            "exec" => (source_title(&input), Vec::new()),
            "js" => (text_of(&input, "title"), Vec::new()),
            "shell" | "exec_command" | "shell_command" => (command_title(&input), Vec::new()),
            "apply_patch" => {
                let paths = input.as_str().map(patch_paths).unwrap_or_default();
                (paths.first().cloned(), paths)
            }
            _ => (None, Vec::new()),
        };
        self.calls.insert(call_id.clone());
        self.awaiting.push(call_id.clone());
        out.tool_call(
            ToolCall {
                call_id,
                name,
                category,
                title,
                input,
                paths,
                status: ToolStatus::Running,
                output: None,
                exit_code: None,
            },
            at,
        );
    }

    /// What a call a response item made gave back.
    fn output(&mut self, payload: &Value, out: &mut FeedOut) {
        let Some(call_id) = text_of(payload, "call_id") else {
            return;
        };
        if self.settled.contains(&call_id) {
            return;
        }
        self.awaiting.retain(|id| *id != call_id);
        out.tool_result(
            &call_id,
            ToolOutcome {
                status: ToolStatus::Completed,
                output: payload.get("output").and_then(output_text),
                exit_code: None,
            },
        );
    }

    /// A completed item: a call, whole, or nothing the feed shows.
    fn item(&mut self, item: &Value, out: &mut FeedOut, at: Option<i64>) {
        let call_id = text_of(item, "id").unwrap_or_else(|| {
            self.anonymous += 1;
            format!("item-{}", self.anonymous)
        });
        let status = item_status(item);
        let call = match item.get("type").and_then(Value::as_str) {
            Some("CommandExecution") => {
                let command = command_text(item.get("command"));
                ToolCall {
                    call_id,
                    name: "exec_command".to_owned(),
                    category: ToolCategory::Command,
                    title: command.clone(),
                    input: json!({ "command": command, "cwd": item.get("cwd") }),
                    paths: Vec::new(),
                    status: status_of(status, item.get("aggregated_output").is_some()),
                    output: text_of(item, "aggregated_output"),
                    exit_code: item.get("exit_code").and_then(Value::as_i64),
                }
            }
            Some("FileChange") => {
                let status = status_of(status, true);
                // A change that did not go through changed nothing.
                let named = change_paths(item);
                let paths = if status == ToolStatus::Completed {
                    named.clone()
                } else {
                    Vec::new()
                };
                let title = match item.get("changes").and_then(Value::as_object) {
                    Some(changes) if changes.len() > 1 => Some(format!("{} files", changes.len())),
                    _ => named.first().cloned(),
                };
                let output = [text_of(item, "stdout"), text_of(item, "stderr")]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join("\n");
                ToolCall {
                    call_id,
                    name: "apply_patch".to_owned(),
                    category: ToolCategory::FileEdit,
                    title,
                    input: item.get("changes").cloned().unwrap_or(Value::Null),
                    paths,
                    status,
                    output: (!output.is_empty()).then_some(output),
                    exit_code: None,
                }
            }
            Some("McpToolCall") => {
                let name = match (text_of(item, "server"), text_of(item, "tool")) {
                    (Some(server), Some(tool)) => format!("{server}/{tool}"),
                    (None, Some(tool)) => tool,
                    _ => "mcp".to_owned(),
                };
                let result = item.get("result");
                let failed = result
                    .and_then(|result| result.get("isError"))
                    .and_then(Value::as_bool)
                    == Some(true);
                let mut status = status_of(status, result.is_some());
                if failed && status == ToolStatus::Completed {
                    status = ToolStatus::Failed;
                }
                ToolCall {
                    call_id,
                    title: Some(name.clone()),
                    name,
                    category: ToolCategory::Mcp,
                    input: item.get("arguments").cloned().unwrap_or(Value::Null),
                    paths: Vec::new(),
                    status,
                    output: result.and_then(mcp_output).or_else(|| {
                        item.get("error")
                            .and_then(|error| text_of(error, "message"))
                    }),
                    exit_code: None,
                }
            }
            Some("CollabAgentToolCall") => {
                let name = text_of(item, "tool").unwrap_or_else(|| "agent".to_owned());
                ToolCall {
                    call_id,
                    title: Some(name.clone()),
                    name,
                    category: ToolCategory::Subagent,
                    input: json!({ "receiver_thread_ids": item.get("receiver_thread_ids") }),
                    paths: Vec::new(),
                    status: status_of(status, true),
                    output: None,
                    exit_code: None,
                }
            }
            Some("ImageView") => ToolCall {
                call_id,
                name: "view_image".to_owned(),
                category: ToolCategory::FileRead,
                title: text_of(item, "path"),
                input: json!({ "path": item.get("path") }),
                paths: Vec::new(),
                status: ToolStatus::Completed,
                output: None,
                exit_code: None,
            },
            Some("Extension") if item.get("kind").and_then(Value::as_str) == Some("web.search") => {
                ToolCall {
                    call_id,
                    name: "web_search".to_owned(),
                    category: ToolCategory::Web,
                    title: text_of(item, "query"),
                    input: json!({ "query": item.get("query"), "action": item.get("action") }),
                    paths: Vec::new(),
                    status: ToolStatus::Completed,
                    output: None,
                    exit_code: None,
                }
            }
            // Reasoning, a subagent's activity and the other extensions (`clock.sleep` is the
            // `sleep` call's own item) are not shown; an answer's text is read with the common
            // entries.
            _ => return,
        };
        self.settled.insert(call.call_id.clone());
        self.awaiting.retain(|id| *id != call.call_id);
        out.tool_call(call, at);
    }
}

/// What kind of tool `name` is, as a rollout's `function_call` and `custom_tool_call` items name
/// it. The prompts of `super::interaction` classify the calls they belong to with it (after
/// naming the tools only the hooks name), so a prompt and the feed's entry for the same call agree.
pub(super) fn tool_category(name: &str) -> ToolCategory {
    match name {
        // `wait` resumes a yielded `exec` cell, so it is no subagent call.
        "exec" | "js" | "shell" | "exec_command" | "shell_command" | "wait" => {
            ToolCategory::Command
        }
        "apply_patch" => ToolCategory::FileEdit,
        "spawn_agent" | "send_message" | "wait_agent" | "followup_task" | "list_agents"
        | "interrupt_agent" => ToolCategory::Subagent,
        _ => ToolCategory::Other,
    }
}

/// The word an item gives for how it ended, lowercase and without separators.
fn item_status(item: &Value) -> Option<String> {
    item.get("status")
        .and_then(Value::as_str)
        .map(|status| status.replace(['_', '-'], "").to_ascii_lowercase())
}

/// Where a completed item stands: what it says, or, when it says nothing known, completed if it
/// holds how it ended (`has_result`) and still running if not.
fn status_of(status: Option<String>, has_result: bool) -> ToolStatus {
    match status.as_deref() {
        Some("completed") => ToolStatus::Completed,
        Some("failed") => ToolStatus::Failed,
        Some("declined") => ToolStatus::Declined,
        _ if has_result => ToolStatus::Completed,
        _ => ToolStatus::Running,
    }
}

/// A command from its `command` field: the string, the script a shell wrapper
/// (`[<shell>, "-lc" or "-c", script]`, where the program is a shell by its name) runs, or the
/// words of any other argument vector joined.
fn command_text(command: Option<&Value>) -> Option<String> {
    let text = match command? {
        Value::String(text) => text.clone(),
        Value::Array(words) => match words.as_slice() {
            [program, flag, Value::String(script)]
                if matches!(flag.as_str(), Some("-lc" | "-c"))
                    && program.as_str().is_some_and(is_shell) =>
            {
                script.clone()
            }
            _ => words
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" "),
        },
        _ => return None,
    };
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Whether the program at `path` is a shell, by the name it is run under.
fn is_shell(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let name = name.strip_suffix(".exe").unwrap_or(name);
    matches!(name, "sh" | "bash" | "zsh" | "dash" | "fish" | "ksh")
}

/// The command a shell call's arguments run: `command` (older Codex's `shell`) or `cmd`
/// (`exec_command`).
fn command_title(input: &Value) -> Option<String> {
    command_text(input.get("command")).or_else(|| command_text(input.get("cmd")))
}

/// The first line of an `exec` call's source that is code, not blank or the `// @exec:` pragma.
fn source_title(input: &Value) -> Option<String> {
    input
        .as_str()?
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("// @exec:"))
        .map(str::to_owned)
}

/// What a response item's `output` says: the string itself, or the text items of a list.
fn output_text(output: &Value) -> Option<String> {
    joined_text(output, OUTPUT_TEXT_KINDS)
}

/// What an MCP tool's `result` says: its text content, else its structured content.
fn mcp_output(result: &Value) -> Option<String> {
    result
        .get("content")
        .and_then(|content| joined_text(content, OUTPUT_TEXT_KINDS))
        .or_else(|| {
            result
                .get("structuredContent")
                .filter(|content| !content.is_null())
                .map(Value::to_string)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ConversationBody, ConversationEntry, ToolCallEntry};

    fn read(capture: &str) -> FeedOut {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join(capture);
        let mut reader = CodexFeedReader::default();
        let mut out = FeedOut::default();
        for line in std::fs::read_to_string(path).unwrap().lines() {
            reader.line(&serde_json::from_str(line).unwrap(), &mut out);
        }
        out
    }

    fn call(entry: &ConversationEntry) -> &ToolCallEntry {
        match &entry.body {
            ConversationBody::ToolCall(call) => call,
            other => panic!("not a tool call: {other:?}"),
        }
    }

    /// The entries of `testdata/feed_codex_0.167.1_rollout.jsonl`, by index: 0 user, 1 commentary,
    /// 2 `sleep`, 3 `exec`, 4 and 5 commands (completed, failed), 6 file change, 7 and 8 MCP calls
    /// (completed, failed), 9 image, 10 `wait_agent`, 11 `spawn_agent`, 12 `wait`,
    /// 13 `request_user_input_async`, 14 web search, 15 `list_agents`, 16 `interrupt_agent`,
    /// 17 compaction, 18 to 22 calls with no output (ended by an abort, the next turn, a clean
    /// `task_complete`, an errored one), 23 compaction, 24 final answer.
    fn capture() -> FeedOut {
        read("feed_codex_0.167.1_rollout.jsonl")
    }

    #[test]
    fn reads_the_messages_and_the_compactions() {
        let out = capture();
        let entries = out.entries();
        assert_eq!(entries.len(), 25);
        assert!(matches!(&entries[0].body,
            ConversationBody::User { text } if text == "Fix the flaky test"));
        assert_eq!(entries[0].turn, 1);
        assert!(entries[0].at.is_some());
        // The commentary shows as ordinary assistant text; the questions that
        // `request_user_input_async` writes as a message do not.
        assert!(matches!(&entries[1].body,
            ConversationBody::Assistant { text } if text == "Looking at the test first."));
        let assistants = entries
            .iter()
            .filter(|entry| matches!(entry.body, ConversationBody::Assistant { .. }))
            .count();
        assert_eq!(assistants, 2);
        // The `compaction` response item counts once, and so do the `compacted` line and the
        // `ContextCompaction` item beside it.
        let compactions: Vec<usize> = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| matches!(entry.body, ConversationBody::Compaction { .. }))
            .map(|(index, _)| index)
            .collect();
        assert_eq!(compactions, [17, 23]);
        assert!(matches!(&entries[20].body,
            ConversationBody::User { text } if text == "Continue"));
        assert_eq!(entries[20].turn, 2);
        assert!(matches!(&entries[24].body,
            ConversationBody::Assistant { text } if text == "It was a race; fixed."));
        assert_eq!(entries[24].turn, 2);
    }

    #[test]
    fn reads_response_item_calls_with_their_outputs() {
        let out = capture();
        let entries = out.entries();

        // Its `clock.sleep` item shows nothing, so the call stays and the output completes it.
        let sleep = call(&entries[2]);
        assert_eq!(
            (sleep.name.as_str(), sleep.category),
            ("sleep", ToolCategory::Other)
        );
        assert_eq!(sleep.status, ToolStatus::Completed);
        assert_eq!(sleep.output.as_deref(), Some("Invented text 1."));
        assert_eq!(sleep.input, json!({ "duration_ms": 20000 }));

        let exec = call(&entries[3]);
        assert_eq!(
            (exec.name.as_str(), exec.category),
            ("exec", ToolCategory::Command)
        );
        assert_eq!(exec.status, ToolStatus::Completed);
        assert_eq!(
            exec.title.as_deref(),
            Some(r#"text(await tools.exec_command({cmd: "git status --short"}));"#)
        );
        assert!(exec
            .output
            .as_deref()
            .unwrap()
            .starts_with("Script completed\nWall time 3.7 seconds"));

        // A wait on a yielded `exec` cell is a command, not a subagent call.
        let wait = call(&entries[12]);
        assert_eq!(
            (wait.name.as_str(), wait.category, wait.title.as_deref()),
            ("wait", ToolCategory::Command, None)
        );
        assert_eq!(wait.status, ToolStatus::Completed);
        assert!(wait.output.is_some());

        for (index, name) in [
            (11, "spawn_agent"),
            (15, "list_agents"),
            (16, "interrupt_agent"),
        ] {
            let subagent = call(&entries[index]);
            assert_eq!(
                (subagent.name.as_str(), subagent.category, subagent.status),
                (name, ToolCategory::Subagent, ToolStatus::Completed)
            );
            assert!(subagent.output.is_some(), "{name}");
        }
    }

    #[test]
    fn a_question_keeps_its_call_and_answer() {
        let out = capture();
        let asked = call(&out.entries()[13]);
        assert_eq!(asked.name, "request_user_input_async");
        assert_eq!(asked.category, ToolCategory::Other);
        assert_eq!(asked.status, ToolStatus::Completed);
        assert_eq!(asked.output.as_deref(), Some(r#"{"accepted":true}"#));
    }

    #[test]
    fn reads_completed_items_as_whole_calls() {
        let out = capture();
        let entries = out.entries();

        let passed = call(&entries[4]);
        assert_eq!(passed.category, ToolCategory::Command);
        assert_eq!(passed.title.as_deref(), Some("pnpm test"));
        assert_eq!(
            (passed.status, passed.exit_code),
            (ToolStatus::Completed, Some(0))
        );
        assert_eq!(passed.output.as_deref(), Some("all passed"));

        let failed = call(&entries[5]);
        assert_eq!(failed.title.as_deref(), Some("pnpm lint"));
        assert_eq!(failed.status, ToolStatus::Failed);
        assert!(failed.exit_code.is_some_and(|code| code != 0));
        assert_eq!(failed.output.as_deref(), Some("2 errors"));

        let edited = call(&entries[6]);
        assert_eq!(edited.category, ToolCategory::FileEdit);
        assert_eq!(edited.title.as_deref(), Some("/work/src/file-1.rs"));
        assert_eq!(edited.status, ToolStatus::Completed);
        assert_eq!(edited.paths, ["/work/src/file-1.rs"]);
        assert!(edited.output.is_some());

        let listed = call(&entries[7]);
        assert_eq!(listed.category, ToolCategory::Mcp);
        assert_eq!(listed.name, "docs/list_project_changes");
        assert_eq!(listed.status, ToolStatus::Completed);
        assert_eq!(listed.output.as_deref(), Some("Invented text 1."));
        let refused = call(&entries[8]);
        assert_eq!(refused.status, ToolStatus::Failed);
        assert!(refused.output.is_some());

        let image = call(&entries[9]);
        assert_eq!(image.category, ToolCategory::FileRead);
        assert_eq!(image.title.as_deref(), Some("/work"));
    }

    #[test]
    fn a_web_search_is_a_completed_web_call_titled_by_its_query() {
        let out = capture();
        let search = call(&out.entries()[14]);
        assert_eq!(search.category, ToolCategory::Web);
        assert_eq!(search.status, ToolStatus::Completed);
        assert_eq!(search.title.as_deref(), Some("Invented text 1."));
        assert_eq!(search.input["query"], "Invented text 1.");
    }

    #[test]
    fn an_item_with_the_calls_id_replaces_it_and_its_output_is_not_applied() {
        let out = capture();
        let waited = call(&out.entries()[10]);
        assert_eq!(
            (waited.name.as_str(), waited.category),
            ("wait", ToolCategory::Subagent)
        );
        assert_eq!(waited.status, ToolStatus::Completed);
        assert_eq!(waited.output, None);
        let with_id = out
            .entries()
            .iter()
            .filter(|entry| {
                matches!(&entry.body,
                    ConversationBody::ToolCall(call) if call.call_id == waited.call_id)
            })
            .count();
        assert_eq!(with_id, 1);
    }

    #[test]
    fn the_end_of_a_turn_settles_the_calls_still_waiting() {
        let out = capture();
        let outcome = |index: usize| {
            let entry = call(&out.entries()[index]);
            assert_eq!(entry.name, "spawn_agent");
            assert_eq!(entry.output, None);
            entry.status
        };
        // `turn_aborted`, the next `task_started`, a `task_complete` and one with an error.
        assert_eq!(outcome(18), ToolStatus::Failed);
        assert_eq!(outcome(19), ToolStatus::Completed);
        assert_eq!(outcome(21), ToolStatus::Completed);
        assert_eq!(outcome(22), ToolStatus::Failed);
        assert!(out.pending().is_empty());
    }

    #[test]
    fn a_compaction_written_twice_in_a_row_is_shown_once() {
        let mut reader = CodexFeedReader::default();
        let mut out = FeedOut::default();
        for line in [
            r#"{"type":"response_item","payload":{"type":"compaction","encrypted_content":"A"}}"#,
            r#"{"type":"compacted","payload":{"message":""}}"#,
            r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"ContextCompaction","id":"x"}}}"#,
        ] {
            reader.line(&serde_json::from_str(line).unwrap(), &mut out);
        }
        assert_eq!(out.len(), 1);
    }

    /// What older Codex wrote that existing real captures hold: the user's `user_message`, the
    /// plan, an `apply_patch` call and the `agent_message` events.
    #[test]
    fn reads_the_older_captures() {
        let out = read("handoff_codex_0.162.1_rollout.jsonl");
        let users: Vec<&str> = out
            .entries()
            .iter()
            .filter_map(|entry| match &entry.body {
                ConversationBody::User { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            users,
            [
                "Fix the flaky test",
                "Also run the linter",
                "And mind the docs"
            ]
        );
        let todos = out
            .entries()
            .iter()
            .filter(|entry| matches!(entry.body, ConversationBody::Todos { .. }))
            .count();
        assert_eq!(todos, 2);
        let patch = out
            .entries()
            .iter()
            .find_map(|entry| match &entry.body {
                ConversationBody::ToolCall(call)
                    if call.name == "apply_patch" && call.call_id == "c2" =>
                {
                    Some(call)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(patch.status, ToolStatus::Running);
        assert_eq!(
            patch.paths,
            ["/work/src/a.rs", "/work/src/b.rs", "/work/src/c.rs"]
        );

        let out = read("history_codex_rollout.jsonl");
        assert!(out.entries().iter().any(|entry| matches!(&entry.body,
            ConversationBody::Assistant { text } if text == "Thanks, closing it out.")));
    }

    #[test]
    fn unknown_and_malformed_lines_yield_nothing() {
        let mut reader = CodexFeedReader::default();
        let mut out = FeedOut::default();
        for line in [
            r#"{}"#,
            r#"{"type":"event_msg"}"#,
            r#"{"type":"event_msg","payload":{"type":"item_completed"}}"#,
            r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"Brand New"}}}"#,
            r#"{"type":"response_item","payload":{"type":"function_call"}}"#,
            r#"{"type":"response_item","payload":{"type":"function_call_output"}}"#,
            r#"{"type":"response_item","payload":{"type":"function_call_output","call_id":"nobody","output":5}}"#,
            r#"{"type":"event_msg","payload":{"type":"turn_aborted"}}"#,
            r#"[1,2]"#,
        ] {
            reader.line(&serde_json::from_str(line).unwrap(), &mut out);
        }
        assert!(out.entries().is_empty());
    }

    fn feed(lines: &[&str]) -> FeedOut {
        let mut reader = CodexFeedReader::default();
        let mut out = FeedOut::default();
        for line in lines {
            reader.line(&serde_json::from_str(line).unwrap(), &mut out);
        }
        out
    }

    /// A call the rollout makes by a response item is running until its output comes, its input is
    /// what the model gave (JSON where it parses), and its output is the text items joined.
    #[test]
    fn a_response_item_call_runs_until_its_output_and_keeps_what_the_model_gave() {
        let calls = [
            r#"{"type":"response_item","payload":{"type":"function_call","name":"exec_command","call_id":"a","arguments":"{\"cmd\":\"ls -la\"}"}}"#,
            r#"{"type":"response_item","payload":{"type":"function_call","name":"mystery","call_id":"b","arguments":"not json"}}"#,
            r#"{"type":"response_item","payload":{"type":"custom_tool_call","name":"apply_patch","call_id":"c","input":"*** Begin Patch\n*** Update File: /w/a.rs\n*** Add File: /w/b.rs\n*** End Patch"}}"#,
        ];
        let mut out = feed(&calls);
        let running: Vec<_> = out
            .entries()
            .iter()
            .map(|entry| call(entry).status)
            .collect();
        assert_eq!(running, [ToolStatus::Running; 3]);
        let (a, b, c) = (
            call(&out.entries()[0]),
            call(&out.entries()[1]),
            call(&out.entries()[2]),
        );
        assert_eq!(
            (a.category, a.title.as_deref()),
            (ToolCategory::Command, Some("ls -la"))
        );
        assert_eq!(b.input, json!("not json"));
        assert_eq!(
            (b.category, b.title.as_deref()),
            (ToolCategory::Other, None)
        );
        assert_eq!(c.category, ToolCategory::FileEdit);
        assert_eq!(c.paths, ["/w/a.rs", "/w/b.rs"]);
        assert_eq!(c.title.as_deref(), Some("/w/a.rs"));

        let mut reader = CodexFeedReader::default();
        out = FeedOut::default();
        for line in calls.iter().chain(&[
            r#"{"type":"response_item","payload":{"type":"function_call_output","call_id":"a","output":[{"type":"input_text","text":"one"},{"type":"output_text","text":"two"}]}}"#,
            r#"{"type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"c","output":"patched"}}"#,
        ]) {
            reader.line(&serde_json::from_str(line).unwrap(), &mut out);
        }
        let statuses: Vec<_> = out
            .entries()
            .iter()
            .map(|entry| call(entry).status)
            .collect();
        assert_eq!(
            statuses,
            [
                ToolStatus::Completed,
                ToolStatus::Running,
                ToolStatus::Completed
            ]
        );
        let joined = call(&out.entries()[0]).output.clone().unwrap();
        assert!(joined.find("one").unwrap() < joined.find("two").unwrap());
        assert_eq!(call(&out.entries()[2]).output.as_deref(), Some("patched"));
    }

    /// The plan is todos only, and no tool call beside them.
    #[test]
    fn the_plan_is_a_todo_list_and_no_call() {
        let out = feed(&[
            r#"{"type":"response_item","payload":{"type":"function_call","name":"update_plan","call_id":"p","arguments":"{\"plan\":[{\"step\":\"a\",\"status\":\"pending\"}]}"}}"#,
            r#"{"type":"response_item","payload":{"type":"function_call_output","call_id":"p","output":"Plan updated"}}"#,
        ]);
        assert!(out
            .entries()
            .iter()
            .all(|entry| !matches!(entry.body, ConversationBody::ToolCall(_))));
    }

    /// An abort fails the calls waiting then, and only those: the ones that ended, and the ones
    /// made after it, are left as they are.
    #[test]
    fn an_abort_fails_only_the_calls_waiting_at_that_point() {
        let out = feed(&[
            r#"{"type":"response_item","payload":{"type":"function_call","name":"sleep","call_id":"a","arguments":"{}"}}"#,
            r#"{"type":"response_item","payload":{"type":"function_call","name":"sleep","call_id":"b","arguments":"{}"}}"#,
            r#"{"type":"response_item","payload":{"type":"function_call_output","call_id":"a","output":"ok"}}"#,
            r#"{"type":"event_msg","payload":{"type":"turn_aborted","reason":"interrupted"}}"#,
            r#"{"type":"response_item","payload":{"type":"function_call","name":"sleep","call_id":"c","arguments":"{}"}}"#,
            r#"{"type":"event_msg","payload":{"type":"turn_aborted"}}"#,
        ]);
        let statuses: Vec<_> = out
            .entries()
            .iter()
            .map(|entry| call(entry).status)
            .collect();
        assert_eq!(
            statuses,
            [
                ToolStatus::Completed,
                ToolStatus::Failed,
                ToolStatus::Failed
            ]
        );
    }

    /// A completed item that says nothing known of its status is completed when it holds how it
    /// ended, and its result is not undone by a response-item output of the same id after it.
    #[test]
    fn an_item_with_an_unknown_status_is_completed_when_it_holds_its_result() {
        let out = feed(&[
            r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","id":"x","command":["ls","-l"],"aggregated_output":"out","exit_code":0,"status":"brand_new"}}}"#,
            r#"{"type":"response_item","payload":{"type":"function_call_output","call_id":"x","output":"late"}}"#,
        ]);
        let first = call(&out.entries()[0]);
        assert_eq!(
            (
                first.status,
                first.title.as_deref(),
                first.output.as_deref()
            ),
            (ToolStatus::Completed, Some("ls -l"), Some("out"))
        );
        assert_eq!(out.len(), 1);
    }

    /// Titles: an `exec` call by the first line of code (not the pragma), a `js` call by its
    /// `title`.
    #[test]
    fn titles_come_from_the_source_or_the_title_field() {
        let out = feed(&[
            r#"{"type":"response_item","payload":{"type":"custom_tool_call","name":"exec","call_id":"a","input":"// @exec: {\"yield_time_ms\": 1000}\n\n  text(1);\ntext(2);"}}"#,
            r#"{"type":"response_item","payload":{"type":"function_call","name":"js","call_id":"b","arguments":"{\"title\":\"Read the plan\",\"code\":\"1\"}"}}"#,
        ]);
        let (a, b) = (call(&out.entries()[0]), call(&out.entries()[1]));
        assert_eq!(a.title.as_deref(), Some("text(1);"));
        assert_eq!(
            (b.category, b.title.as_deref()),
            (ToolCategory::Command, Some("Read the plan"))
        );
    }

    /// A change that was declined or failed changed no file.
    #[test]
    fn a_declined_change_names_no_changed_file() {
        let out = feed(&[
            r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"FileChange","id":"a","changes":{"/w/a.rs":{"type":"update","move_path":null}},"status":"declined"}}}"#,
            r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"FileChange","id":"b","changes":{"/w/a.rs":{"type":"update","move_path":"/w/b.rs"},"/w/c.rs":{"type":"add"}},"status":"failed","stderr":"no"}}}"#,
        ]);
        let (a, b) = (call(&out.entries()[0]), call(&out.entries()[1]));
        assert_eq!((a.status, a.paths.is_empty()), (ToolStatus::Declined, true));
        assert_eq!((b.status, b.paths.is_empty()), (ToolStatus::Failed, true));
        assert_eq!(b.title.as_deref(), Some("2 files"));
        assert_eq!(b.output.as_deref(), Some("no"));
    }

    /// A shell wrapper is titled by its script, any other argument vector by its words.
    #[test]
    fn a_command_is_titled_by_the_script_of_its_shell_wrapper() {
        let item = |command: &str| {
            format!(
                r#"{{"type":"event_msg","payload":{{"type":"item_completed","item":{{"type":"CommandExecution","id":"a","command":{command},"aggregated_output":"","exit_code":0,"status":"completed"}}}}}}"#
            )
        };
        for (command, title) in [
            (
                r#"["/bin/zsh","-lc","git status && ls"]"#,
                "git status && ls",
            ),
            (r#"["bash","-c","make"]"#, "make"),
            (r#"["git","log","-n","1"]"#, "git log -n 1"),
            (r#"["python3","-c","print(1)"]"#, "python3 -c print(1)"),
            (r#"["/bin/zsh","-lc",5]"#, "/bin/zsh -lc"),
            (r#""cargo build""#, "cargo build"),
        ] {
            let out = feed(&[&item(command)]);
            assert_eq!(call(&out.entries()[0]).title.as_deref(), Some(title));
        }
    }
}
