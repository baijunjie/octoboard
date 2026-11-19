//! Claude Code's transcript read as the live conversation feed (`crate::records::feed`): what the
//! user typed, what the model wrote, each tool call with how it ended, the todo list and the
//! compactions, in the order the transcript says them. How a line is told apart as the user's or
//! the model's is shared with [`super::conversation`].
//!
//! Claude Code keeps what it abandoned in the file. A user who goes back to an earlier point, or
//! resubmits an interrupted prompt, makes a new prompt whose `parentUuid` points back at an
//! earlier line rather than at the line before it, and the lines after the point stay. A tool's
//! result lines branch off the same tree when calls run in parallel, so the live branch cannot be
//! found by walking back from the last line; a fork is recognised at the new prompt instead: its
//! ancestors do not include the prompt before it.

use std::collections::HashMap;

use serde_json::Value;

use crate::protocol::{ToolCategory, ToolStatus};
use crate::records::conversation::text_of;
use crate::records::feed::{at_of, FeedOut, FeedReader, ToolCall, ToolOutcome};

use super::conversation::{assistant_blocks, flagged, todo_items, user_message, user_text};

/// The `toolDenialKind` values of a tool result that say the call was not allowed to run: the
/// user declined it, or a permission rule or the auto mode did. A call the user interrupted
/// (`interrupted`) carries none of these marks and reads as `Failed` where the result is an error.
const DECLINED: [&str; 4] = [
    "user-rejected",
    "permission-rule",
    "automode-blocked",
    "automode-unavailable",
];

/// Reads Claude Code's transcript line by line into the feed.
#[derive(Default)]
pub(super) struct ClaudeFeedReader {
    /// The line each line follows, by `uuid`.
    parents: HashMap<String, Option<String>>,
    /// How many entries the feed held once each line was read, by `uuid`.
    lengths: HashMap<String, usize>,
    /// The `uuid` of the last prompt the user gave.
    prompt: Option<String>,
}

impl FeedReader for ClaudeFeedReader {
    fn line(&mut self, line: &Value, out: &mut FeedOut) {
        if flagged(line, "isSidechain") {
            return;
        }
        let uuid = text_of(line, "uuid");
        let at = at_of(line);
        let prompt =
            user_message(line).and_then(|text| user_text(&text).map(|text| text.into_owned()));
        if let Some(uuid) = &uuid {
            self.parents.insert(uuid.clone(), parent_of(line));
        }
        if prompt.is_some() {
            if let Some(parent) = self.forked_from(line) {
                out.truncate(parent);
            }
            self.prompt = uuid.clone();
        }
        match line.get("type").and_then(Value::as_str) {
            Some("user") => read_user(line, prompt, at, out),
            Some("assistant") => read_assistant(line, at, out),
            Some("attachment") => {
                if let Some(text) = prompt {
                    out.user(&text, at);
                }
            }
            _ => {}
        }
        if let Some(uuid) = uuid {
            self.lengths.insert(uuid, out.len());
        }
    }
}

impl ClaudeFeedReader {
    /// When the new prompt `line` is not a continuation of the prompt before it, how many entries
    /// the conversation held at the line it goes back to. A parent that is not known says nothing.
    fn forked_from(&self, line: &Value) -> Option<usize> {
        let previous = self.prompt.as_deref()?;
        let parent = parent_of(line)?;
        let length = *self.lengths.get(&parent)?;
        let mut at = Some(parent);
        // A transcript that loops back on itself is cut off after as many steps as it has lines.
        for _ in 0..=self.parents.len() {
            let Some(uuid) = at else { break };
            if uuid == previous {
                return None;
            }
            at = self.parents.get(&uuid).cloned().flatten();
        }
        Some(length)
    }
}

/// The line `line` follows: its `parentUuid`, or where a compaction boundary (which has none)
/// picks up, `logicalParentUuid`.
fn parent_of(line: &Value) -> Option<String> {
    text_of(line, "parentUuid").or_else(|| text_of(line, "logicalParentUuid"))
}

/// A user line: the prompt it holds, a compaction summary, or the results of tool calls.
fn read_user(line: &Value, prompt: Option<String>, at: Option<i64>, out: &mut FeedOut) {
    if let Some(text) = prompt {
        out.user(&text, at);
        return;
    }
    let Some(content) = line
        .get("message")
        .and_then(|message| message.get("content"))
    else {
        return;
    };
    if flagged(line, "isCompactSummary") {
        let summary = crate::records::conversation::joined_text(content, &["text"]);
        out.compaction(summary, at);
        return;
    }
    let declined = line
        .get("toolDenialKind")
        .and_then(Value::as_str)
        .is_some_and(|kind| DECLINED.contains(&kind));
    for block in content.as_array().into_iter().flatten() {
        if block.get("type").and_then(Value::as_str) != Some("tool_result") {
            continue;
        }
        let Some(id) = block.get("tool_use_id").and_then(Value::as_str) else {
            continue;
        };
        let status = if declined {
            ToolStatus::Declined
        } else if flagged(block, "is_error") {
            ToolStatus::Failed
        } else {
            ToolStatus::Completed
        };
        let output = block.get("content").and_then(result_text);
        out.tool_result(
            id,
            ToolOutcome {
                status,
                output,
                exit_code: None,
            },
        );
    }
}

/// A tool result's content as text: the string it is, or its blocks joined, one a line: text as
/// it is, an image as `[image]` and a tool the result refers to (a `ToolSearch` result) by name.
fn result_text(content: &Value) -> Option<String> {
    let text = match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| match block.get("type").and_then(Value::as_str) {
                Some("text") => block.get("text").and_then(Value::as_str).map(str::to_owned),
                Some("image") => Some("[image]".to_owned()),
                Some("tool_reference") => block
                    .get("tool_name")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => return None,
    };
    (!text.is_empty()).then_some(text)
}

fn read_assistant(line: &Value, at: Option<i64>, out: &mut FeedOut) {
    for block in assistant_blocks(line) {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    out.assistant(text, at);
                }
            }
            Some("tool_use") => read_tool_use(block, at, out),
            _ => {}
        }
    }
}

fn read_tool_use(block: &Value, at: Option<i64>, out: &mut FeedOut) {
    let (Some(id), Some(name)) = (
        block.get("id").and_then(Value::as_str),
        block.get("name").and_then(Value::as_str),
    ) else {
        return;
    };
    let input = block.get("input").cloned().unwrap_or(Value::Null);
    if name == "TodoWrite" {
        if let Some(items) = todo_items(&input) {
            out.todos(items, false, at);
        }
        return;
    }
    let (category, title, paths) = describe(name, &input);
    out.tool_call(
        ToolCall {
            call_id: id.to_owned(),
            name: name.to_owned(),
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

/// What a tool call is, a line to show for it and the files it changes, from its name and input.
fn describe(name: &str, input: &Value) -> (ToolCategory, Option<String>, Vec<String>) {
    let field = |key| text_of(input, key);
    match name {
        "Bash" => (
            ToolCategory::Command,
            field("command").or_else(|| field("description")),
            Vec::new(),
        ),
        "Edit" | "MultiEdit" | "Write" => edit(field("file_path")),
        "NotebookEdit" => edit(field("notebook_path")),
        "Read" => (ToolCategory::FileRead, field("file_path"), Vec::new()),
        "Grep" | "Glob" => (ToolCategory::Search, field("pattern"), Vec::new()),
        "WebFetch" => (ToolCategory::Web, field("url"), Vec::new()),
        "WebSearch" => (ToolCategory::Web, field("query"), Vec::new()),
        "Task" | "Agent" => (ToolCategory::Subagent, field("description"), Vec::new()),
        "SendMessage" => (
            ToolCategory::Subagent,
            field("to").or_else(|| field("summary")),
            Vec::new(),
        ),
        "ListAgents" | "TaskStop" => (ToolCategory::Subagent, None, Vec::new()),
        "Monitor" => (ToolCategory::Command, field("command"), Vec::new()),
        "ToolSearch" => (ToolCategory::Search, field("query"), Vec::new()),
        "Skill" => (ToolCategory::Other, field("skill"), Vec::new()),
        _ if name.starts_with("mcp__") => (ToolCategory::Mcp, mcp_title(name), Vec::new()),
        _ => (ToolCategory::Other, None, Vec::new()),
    }
}

/// `server/tool` from an MCP tool's name, `mcp__server__tool`.
fn mcp_title(name: &str) -> Option<String> {
    let rest = name.strip_prefix("mcp__")?;
    match rest.split_once("__") {
        Some((server, tool)) => Some(format!("{server}/{tool}")),
        None => Some(rest.to_owned()).filter(|rest| !rest.is_empty()),
    }
}

fn edit(path: Option<String>) -> (ToolCategory, Option<String>, Vec<String>) {
    let paths = path.iter().cloned().collect();
    (ToolCategory::FileEdit, path, paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ConversationBody, ConversationEntry};
    use serde_json::json;

    const CAPTURE: &str = include_str!("../../../testdata/feed_claude_2.1.328.jsonl");

    fn lines() -> Vec<Value> {
        CAPTURE
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn feed(lines: &[Value], reader: &mut ClaudeFeedReader, out: &mut FeedOut) {
        for line in lines {
            reader.line(line, out);
        }
    }

    /// The text of an entry that has some, with the turn and what says its kind.
    fn shown(entry: &ConversationEntry) -> String {
        let turn = entry.turn;
        match &entry.body {
            ConversationBody::User { text } => format!("{turn} user {text}"),
            ConversationBody::Octoboard { text } => format!("{turn} octoboard {text}"),
            ConversationBody::Assistant { text } => format!("{turn} assistant {text}"),
            ConversationBody::ToolCall(call) => format!(
                "{turn} tool {} {:?} {:?}",
                call.call_id, call.status, call.output
            ),
            ConversationBody::Todos { items } => format!(
                "{turn} todos {}",
                items
                    .iter()
                    .map(|item| format!("{}={}", item.text, item.status))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            ConversationBody::Compaction { summary } => format!("{turn} compaction {summary:?}"),
        }
    }

    fn call(out: &FeedOut, id: &str) -> crate::protocol::ToolCallEntry {
        out.entries()
            .iter()
            .find_map(|entry| match &entry.body {
                ConversationBody::ToolCall(call) if call.call_id == id => Some(call.clone()),
                _ => None,
            })
            .unwrap()
    }

    /// The lines of the capture up to and including the first that holds `marker`.
    fn through(marker: &str) -> Vec<Value> {
        let lines = lines();
        let end = lines
            .iter()
            .position(|line| line.to_string().contains(marker))
            .unwrap();
        lines[..=end].to_vec()
    }

    /// The prompts, replies and tool calls with their results of the capture up to the reply
    /// before the compact boundary: what a message of Octoboard's, a queued one and a slash
    /// command read as, the calls made together, and nothing of the subagent or of the lines
    /// Claude Code writes for itself.
    #[test]
    fn a_conversation_reads_as_entries() {
        let mut out = FeedOut::default();
        feed(
            &through("Noted, reviewing now."),
            &mut ClaudeFeedReader::default(),
            &mut out,
        );
        let shown: Vec<String> = out.entries().iter().map(shown).collect();
        assert_eq!(
            shown,
            [
                "1 user Add a retry to the fetch helper",
                "1 assistant I'll read the helper and look for callers.",
                "1 tool toolu_read Completed Some(\"1\\texport async function fetchJson() {}\")",
                "1 tool toolu_search Completed Some(\"Skill\")",
                "1 tool toolu_mcp Completed Some(\"{\\\"id\\\":\\\"s1\\\",\\\"state\\\":\\\"idle\\\"}\")",
                "1 tool toolu_bash1 Failed Some(\"Error: exit code 1\\n1 test failed\")",
                "1 tool toolu_edit Declined Some(\"The user doesn't want to proceed with this tool use.\")",
                "1 tool toolu_write Declined Some(\"Permission to use Write has been denied.\")",
                "1 tool toolu_bash2 Declined Some(\"Blocked by the auto mode classifier.\")",
                "1 tool toolu_bash2b Declined Some(\"Auto mode is unavailable.\")",
                "1 tool toolu_agent Completed Some(\"Three callers found\")",
                "1 tool toolu_send Completed Some(\"Message queued for delivery.\")",
                "1 tool toolu_skill Completed Some(\"Launching skill: review\")",
                "1 tool toolu_stop Completed Some(\"Stopped the task.\")",
                "1 tool toolu_monitor Completed Some(\"Monitor started.\")",
                "1 tool toolu_list Completed Some(\"No agents.\")",
                "1 tool toolu_search2 Completed Some(\"Results for the query.\")",
                "1 tool toolu_fetch Completed Some(\"Fetched the page.\")",
                "1 tool toolu_image Completed Some(\"[image]\")",
                "1 tool toolu_bash3 Failed Some(\"The user interrupted this tool call.\")",
                "2 user Continue, but keep the old helper",
                "2 assistant The retry is in place.",
                "3 user also add a test",
                "4 user /review src",
                "5 octoboard Message from Octoboard (session report): build is green.",
                "5 assistant Noted, reviewing now.",
            ]
        );
        assert_eq!(out.entries()[0].at, Some(1_795_165_206_000));
    }

    /// What each tool of the capture is, the line shown for it and the files it changes.
    #[test]
    fn tool_calls_say_what_they_are() {
        let mut out = FeedOut::default();
        feed(
            &through("Noted, reviewing now."),
            &mut ClaudeFeedReader::default(),
            &mut out,
        );
        let path = "/work/project/src/fetch.ts";
        for (id, name, category, title, paths) in [
            (
                "toolu_read",
                "Read",
                ToolCategory::FileRead,
                Some(path),
                vec![],
            ),
            (
                "toolu_search",
                "ToolSearch",
                ToolCategory::Search,
                Some("select:Skill"),
                vec![],
            ),
            (
                "toolu_mcp",
                "mcp__octoboard__get_session",
                ToolCategory::Mcp,
                Some("octoboard/get_session"),
                vec![],
            ),
            (
                "toolu_bash1",
                "Bash",
                ToolCategory::Command,
                Some("npm test"),
                vec![],
            ),
            (
                "toolu_edit",
                "Edit",
                ToolCategory::FileEdit,
                Some(path),
                vec![path],
            ),
            (
                "toolu_write",
                "Write",
                ToolCategory::FileEdit,
                Some(path),
                vec![path],
            ),
            (
                "toolu_agent",
                "Agent",
                ToolCategory::Subagent,
                Some("Run the tests"),
                vec![],
            ),
            (
                "toolu_send",
                "SendMessage",
                ToolCategory::Subagent,
                Some("helper"),
                vec![],
            ),
            (
                "toolu_skill",
                "Skill",
                ToolCategory::Other,
                Some("review"),
                vec![],
            ),
            (
                "toolu_stop",
                "TaskStop",
                ToolCategory::Subagent,
                None,
                vec![],
            ),
            (
                "toolu_monitor",
                "Monitor",
                ToolCategory::Command,
                Some("npm test"),
                vec![],
            ),
            (
                "toolu_list",
                "ListAgents",
                ToolCategory::Subagent,
                None,
                vec![],
            ),
            (
                "toolu_search2",
                "WebSearch",
                ToolCategory::Web,
                Some("retry backoff"),
                vec![],
            ),
            (
                "toolu_fetch",
                "WebFetch",
                ToolCategory::Web,
                Some("https://example.com/docs"),
                vec![],
            ),
        ] {
            let call = call(&out, id);
            assert_eq!(call.name, name, "{id}");
            assert_eq!(call.category, category, "{id}");
            assert_eq!(call.title.as_deref(), title, "{id}");
            assert_eq!(call.paths, paths, "{id}");
        }
    }

    #[test]
    fn tools_are_categorised_by_name() {
        let describe = |name, input| describe(name, &input);
        assert_eq!(
            describe("Bash", json!({"description": "Run it"})),
            (ToolCategory::Command, Some("Run it".to_owned()), vec![])
        );
        assert_eq!(
            describe("NotebookEdit", json!({"notebook_path": "/a.ipynb"})),
            (
                ToolCategory::FileEdit,
                Some("/a.ipynb".to_owned()),
                vec!["/a.ipynb".to_owned()]
            )
        );
        assert_eq!(
            describe("Glob", json!({"pattern": "*.rs"})),
            (ToolCategory::Search, Some("*.rs".to_owned()), vec![])
        );
        assert_eq!(
            describe("SendMessage", json!({"summary": "Status", "message": "m"})),
            (ToolCategory::Subagent, Some("Status".to_owned()), vec![])
        );
        assert_eq!(
            describe("SendMessage", json!({"to": "a", "summary": "Status"})).1,
            Some("a".to_owned())
        );
        assert_eq!(
            describe("Task", json!({"description": "x"})).0,
            ToolCategory::Subagent
        );
        assert_eq!(describe("Unknown", json!({})).0, ToolCategory::Other);
        assert_eq!(describe("Edit", json!("odd")).2, Vec::<String>::new());
    }

    #[test]
    fn an_mcp_tool_is_titled_by_its_server_and_name() {
        for (name, title) in [
            ("mcp__server__tool", Some("server/tool")),
            (
                "mcp__claude-in-chrome__tabs_context_mcp",
                Some("claude-in-chrome/tabs_context_mcp"),
            ),
            ("mcp__server__a__b", Some("server/a__b")),
            ("mcp__server", Some("server")),
            ("mcp__", None),
        ] {
            let (category, found, _) = describe(name, &json!({}));
            assert_eq!(category, ToolCategory::Mcp, "{name}");
            assert_eq!(found.as_deref(), title, "{name}");
        }
    }

    /// The whole capture: the resubmitted prompt replaces the interrupted one and the last
    /// prompt rewinds to before the compaction, so the conversation ends with it.
    #[test]
    fn a_rewind_drops_what_was_abandoned() {
        let mut out = FeedOut::default();
        feed(&lines(), &mut ClaudeFeedReader::default(), &mut out);
        let shown: Vec<String> = out.entries().iter().map(shown).collect();
        assert_eq!(
            shown[shown.len() - 3..],
            [
                "5 assistant Noted, reviewing now.",
                "6 user Start over with a different approach",
                "6 assistant Starting over.",
            ]
        );
        assert!(!shown.iter().any(|entry| entry.contains("compaction")));
        assert!(!shown.iter().any(|entry| entry.contains("Continue with")));
    }

    #[test]
    fn a_compaction_and_a_resubmitted_prompt() {
        let mut out = FeedOut::default();
        feed(
            &through("Now explain the diff"),
            &mut ClaudeFeedReader::default(),
            &mut out,
        );
        let shown: Vec<String> = out.entries().iter().map(shown).collect();
        let compaction = shown
            .iter()
            .position(|entry| entry.contains("compaction"))
            .unwrap();
        assert_eq!(
            shown[compaction..],
            [
                "5 compaction Some(\"This session adds a retry to the fetch helper.\")",
                "6 user Continue with the unit tests only",
                "6 assistant Running the unit tests.",
                "6 tool toolu_bash5 Completed Some(\"12 passed\")",
                "7 user Now explain the diff",
            ]
        );
        assert!(!shown
            .iter()
            .any(|entry| entry.contains("Continue with the tests")));
    }

    #[test]
    fn a_fork_after_the_changes_were_taken_replaces_the_conversation() {
        let lines = lines();
        let call = lines
            .iter()
            .position(|line| line.to_string().contains("toolu_bash4"))
            .unwrap();
        let mut reader = ClaudeFeedReader::default();
        let mut out = FeedOut::default();
        feed(&lines[..=call], &mut reader, &mut out);
        assert!(!out.take_changes().replaced);
        // The note the interrupt left adds nothing; the resubmitted prompt, a sibling of the one
        // it replaces, takes the conversation back.
        feed(&lines[call + 1..call + 2], &mut reader, &mut out);
        let changes = out.take_changes();
        assert!(!changes.replaced);
        assert!(changes.appended.is_empty());
        feed(&lines[call + 2..call + 3], &mut reader, &mut out);
        let changes = out.take_changes();
        assert!(changes.replaced);
        assert!(changes.appended.is_empty());
        assert_eq!(
            out.entries().last().map(shown).as_deref(),
            Some("6 user Continue with the unit tests only")
        );
    }

    /// Results of calls that ran together hang off side branches; they are no fork.
    #[test]
    fn parallel_results_are_no_fork() {
        let mut reader = ClaudeFeedReader::default();
        let mut out = FeedOut::default();
        let calls = through("toolu_mcp").len();
        feed(&lines()[..calls], &mut reader, &mut out);
        out.take_changes();
        feed(
            &lines()[calls..through("Noted, reviewing now.").len()],
            &mut reader,
            &mut out,
        );
        assert!(!out.take_changes().replaced);
    }

    #[test]
    fn odd_lines_yield_nothing() {
        let mut out = FeedOut::default();
        let mut reader = ClaudeFeedReader::default();
        for line in [
            json!(null),
            json!([]),
            json!({"type": "user"}),
            json!({"type": "user", "message": {"content": 4}}),
            json!({"type": "assistant", "message": {"content": [{"type": "tool_use"}, 3]}}),
            json!({"type": "user", "message": {"content": [{"type": "tool_result"}]}}),
            json!({"type": "assistant", "uuid": "a", "parentUuid": "a",
                   "message": {"content": [{"type": "text", "text": " "}]}}),
            json!({"type": "user", "uuid": "b", "parentUuid": "b",
                   "message": {"content": "hi"}}),
            json!({"type": "attachment", "attachment": {"type": "queued_command"}}),
        ] {
            reader.line(&line, &mut out);
        }
        assert_eq!(out.len(), 1);
    }

    fn user_line(uuid: &str, parent: Option<&str>, content: Value) -> Value {
        json!({"type": "user", "uuid": uuid, "parentUuid": parent,
               "origin": {"kind": "human"}, "message": {"content": content}})
    }

    fn assistant_line(uuid: &str, parent: &str, block: Value) -> Value {
        json!({"type": "assistant", "uuid": uuid, "parentUuid": parent,
               "message": {"model": "m", "content": [block]}})
    }

    fn text_block(text: &str) -> Value {
        json!({"type": "text", "text": text})
    }

    fn read(lines: &[Value]) -> FeedOut {
        let mut out = FeedOut::default();
        feed(lines, &mut ClaudeFeedReader::default(), &mut out);
        out
    }

    fn shown_all(out: &FeedOut) -> Vec<String> {
        out.entries().iter().map(shown).collect()
    }

    /// A paste's wrapper is taken off, a slash command reads as typed, and what Claude Code or
    /// something else wrote on the user's side (the interrupt note, a task's notification, a
    /// message from another session, a queued command no human sent) is no message.
    #[test]
    fn only_what_a_person_typed_is_a_user_entry() {
        let from = |kind: &str| json!({"kind": kind});
        let mut other = user_line("u3", Some("u2"), json!("from a background task"));
        other["origin"] = from("task-notification");
        let mut peer = user_line("u4", Some("u3"), json!("from another session"));
        peer["origin"] = from("peer");
        let queued = |uuid: &str, parent: &str, origin: &str| {
            json!({"type": "attachment", "uuid": uuid, "parentUuid": parent,
                   "attachment": {"type": "queued_command", "prompt": "queued words",
                                  "origin": {"kind": origin}}})
        };
        let out = read(&[
            user_line(
                "u1",
                None,
                json!("<pasted_content id=\"x\">\nbody\n</pasted_content id=\"x\">"),
            ),
            user_line(
                "u2",
                Some("u1"),
                json!([{"type": "text", "text": "[Request interrupted by user]"}]),
            ),
            other,
            peer,
            queued("q1", "u4", "peer"),
            queued("q2", "q1", "human"),
            user_line(
                "u5",
                Some("q2"),
                json!("<local-command-stdout>x</local-command-stdout>"),
            ),
            user_line(
                "u6",
                Some("u5"),
                json!("<command-name>/clear</command-name>\n<command-args></command-args>"),
            ),
        ]);
        assert_eq!(
            shown_all(&out),
            ["1 user body", "2 user queued words", "3 user /clear"]
        );
    }

    /// A subagent's lines are another conversation: its prompt, replies and results add nothing
    /// and its prompt is no rewind of the main one.
    #[test]
    fn sidechain_lines_are_ignored_whatever_they_hold() {
        let sidechain = |line: Value| {
            let mut line = line;
            line["isSidechain"] = json!(true);
            line
        };
        let lines = [
            user_line("u1", None, json!("go")),
            assistant_line(
                "a1",
                "u1",
                json!({"type": "tool_use", "id": "t1", "name": "Task", "input": {}}),
            ),
            sidechain(user_line("s1", Some("u1"), json!("sub prompt"))),
            sidechain(assistant_line("s2", "s1", text_block("sub text"))),
            sidechain(user_line(
                "s3",
                Some("s2"),
                json!([{"type": "tool_result", "tool_use_id": "t1", "content": "wrong"}]),
            )),
            sidechain(
                json!({"type": "attachment", "uuid": "s4", "parentUuid": "s3",
                "attachment": {"type": "queued_command", "prompt": "sub",
                               "origin": {"kind": "human"}}}),
            ),
            user_line(
                "u2",
                Some("a1"),
                json!([{"type": "tool_result", "tool_use_id": "t1", "content": "right"}]),
            ),
        ];
        let mut out = FeedOut::default();
        let mut reader = ClaudeFeedReader::default();
        feed(&lines[..2], &mut reader, &mut out);
        out.take_changes();
        feed(&lines[2..], &mut reader, &mut out);
        let changes = out.take_changes();
        assert!(!changes.replaced && changes.appended.is_empty());
        assert_eq!(
            shown_all(&out),
            ["1 user go", "1 tool t1 Completed Some(\"right\")"]
        );
    }

    /// Results say how a call ended: an error is failed, a refusal by the user is declined
    /// whether or not Claude Code also marks it an error, and the output is the text of its blocks.
    #[test]
    fn results_complete_fail_or_decline_their_calls() {
        let call_line = |uuid: &str, parent: &str, id: &str| {
            assistant_line(
                uuid,
                parent,
                json!({"type": "tool_use", "id": id, "name": "Bash", "input": {"command": "x"}}),
            )
        };
        let mut rejected = user_line(
            "r3",
            Some("c3"),
            json!([{"type": "tool_result", "tool_use_id": "t3", "content": "no",
                    "is_error": false}]),
        );
        rejected["toolDenialKind"] = json!("user-rejected");
        let out = read(&[
            user_line("u", None, json!("go")),
            call_line("c1", "u", "t1"),
            call_line("c2", "c1", "t2"),
            call_line("c3", "c2", "t3"),
            call_line("c4", "c3", "t4"),
            user_line(
                "r1",
                Some("c4"),
                json!([{"type": "tool_result", "tool_use_id": "t1", "is_error": true,
                        "content": [{"type": "text", "text": "a"}, {"type": "text", "text": "b"}]}]),
            ),
            user_line(
                "r2",
                Some("c4"),
                json!([{"type": "tool_result", "tool_use_id": "t2", "content": "fine"},
                       {"type": "tool_result", "tool_use_id": "unknown", "content": "x"}]),
            ),
            rejected,
        ]);
        assert_eq!(
            shown_all(&out)[1..],
            [
                "1 tool t1 Failed Some(\"a\\nb\")",
                "1 tool t2 Completed Some(\"fine\")",
                "1 tool t3 Declined Some(\"no\")",
                "1 tool t4 Running None",
            ]
        );
    }

    /// A text block and a tool call of one line are entries in the order of the blocks; a
    /// `TodoWrite` is only todos; the stand-in line for an API error shows nothing. No transcript
    /// on the machine holds a `TodoWrite` call (the capture has none), so its line is written here
    /// after the tool's documented input.
    #[test]
    fn an_assistant_line_is_its_blocks_in_order() {
        let line = json!({"type": "assistant", "uuid": "a", "parentUuid": null,
            "message": {"model": "m", "content": [
                {"type": "thinking", "thinking": "hmm"},
                {"type": "text", "text": "first"},
                {"type": "tool_use", "id": "t", "name": "Read", "input": {"file_path": "/f"}},
                {"type": "text", "text": "second"},
                {"type": "tool_use", "id": "w", "name": "TodoWrite",
                 "input": {"todos": [{"content": "x", "status": "pending"}]}}]}});
        let synthetic = json!({"type": "assistant", "uuid": "b", "parentUuid": "a",
            "message": {"model": "<synthetic>", "content": [text_block("API Error")]}});
        let out = read(&[line, synthetic]);
        assert_eq!(
            shown_all(&out),
            [
                "0 assistant first",
                "0 tool t Running None",
                "0 assistant second",
                "0 todos x=pending",
            ]
        );
        assert!(out.entries().iter().all(|entry| !matches!(
            &entry.body,
            ConversationBody::ToolCall(call) if call.name == "TodoWrite"
        )));
    }

    /// A prompt that names a parent the reader has not seen, or no parent, says nothing about a
    /// rewind, and a transcript whose lines loop back on themselves is not walked forever.
    #[test]
    fn a_prompt_with_an_unknown_parent_or_a_looping_ancestry_rewinds_nothing() {
        let out = read(&[
            user_line("u1", None, json!("one")),
            assistant_line("a1", "u1", text_block("reply")),
            user_line("u2", Some("never-seen"), json!("two")),
        ]);
        assert_eq!(
            shown_all(&out),
            ["1 user one", "1 assistant reply", "2 user two"]
        );
        let out = read(&[
            user_line("u1", None, json!("one")),
            assistant_line("a1", "a2", text_block("reply")),
            assistant_line("a2", "a1", text_block("loop")),
            user_line("u2", Some("a2"), json!("two")),
        ]);
        assert_eq!(out.len(), 4);
    }

    /// A prompt that continues from a reply of the turn before it (its ancestry holds the
    /// previous prompt) drops nothing, however many lines hang between them.
    #[test]
    fn a_prompt_continuing_the_conversation_drops_nothing() {
        let mut out = FeedOut::default();
        let mut reader = ClaudeFeedReader::default();
        feed(
            &[
                user_line("u1", None, json!("one")),
                assistant_line("a1", "u1", text_block("r1")),
                assistant_line("a2", "a1", text_block("r2")),
            ],
            &mut reader,
            &mut out,
        );
        out.take_changes();
        feed(
            &[user_line("u2", Some("a2"), json!("two"))],
            &mut reader,
            &mut out,
        );
        let changes = out.take_changes();
        assert!(!changes.replaced);
        assert_eq!(changes.appended.len(), 1);
        assert_eq!(out.len(), 4);
    }
}
