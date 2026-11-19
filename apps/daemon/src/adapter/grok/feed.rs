//! How Grok Build's conversation reads as the live conversation feed
//! (`crate::records::feed`): `chat_history.jsonl` is the record, and `updates.jsonl` beside it is
//! read for what the record does not say.
//!
//! Grok writes both files when a model call ends, not as the call streams (measured on 1.0.89,
//! "When Grok writes its conversation" in `docs/agent-cli-reference.md`): the call's assistant
//! line, with the tool calls it made, lands in `chat_history.jsonl` some tens of milliseconds
//! before its one `agent_message_chunk` and `tool_call` updates land in `updates.jsonl`, and a
//! tool's result is recorded when the tool ends. A tool call therefore reads as running from the
//! record alone, and in the measured runs the log showed nothing ahead of the record. What it
//! adds there is the `failed` status of a call, which the record's `tool_result` does not carry.
//! The pending path (text and calls the log shows that the record does not hold yet) is kept for
//! an order the measurement did not show, the log ahead of the record, in the TUI or in another
//! version.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::Value;

use super::conversation::{call_entry, entries};
use super::records::GrokRecords;
use crate::protocol::{ToolCategory, ToolStatus};
use crate::records::conversation::{joined_text, text_of, Entry};
use crate::records::feed::{at_of, FeedOut, FeedReader, Pending, ToolCall, ToolOutcome};
use crate::records::RecordStore;

/// The model call an `agent_message_chunk` belongs to: `_meta.promptId` and `_meta.streamStartMs`.
type StreamKey = (String, i64);

/// The text streamed so far by one model call, by `_meta.chunkId`.
#[derive(Default)]
struct Stream {
    chunks: BTreeMap<i64, String>,
}

impl Stream {
    fn text(&self) -> String {
        self.chunks.values().map(String::as_str).collect()
    }
}

/// A tool call the updates announced, which the record may not hold yet.
struct PendingCall {
    call: ToolCall,
    /// The updates said it ended.
    ended: bool,
}

/// Reads one pass over a Grok conversation: the lines of `chat_history.jsonl`, then those of
/// `updates.jsonl`.
#[derive(Default)]
pub(super) struct GrokFeedReader {
    /// The assistant texts the record holds since the user's last message.
    turn_texts: Vec<String>,
    /// The ids of the tool calls the record holds.
    recorded_calls: HashSet<String>,
    /// The ids of the calls whose tool is `run_terminal_command`, whose result opens with its exit
    /// code.
    commands: HashSet<String>,
    /// The ids of the calls the updates say failed.
    failed: HashSet<String>,
    /// The streams since the last `turn_completed`, in the order they began.
    stream_order: Vec<StreamKey>,
    streams: HashMap<StreamKey, Stream>,
    /// The calls announced since the last `turn_completed`, in the order they were.
    call_order: Vec<String>,
    calls: HashMap<String, PendingCall>,
}

impl FeedReader for GrokFeedReader {
    fn line(&mut self, line: &Value, out: &mut FeedOut) {
        let at = at_of(line);
        match line.get("type").and_then(Value::as_str) {
            Some("user") => {
                for entry in entries(line) {
                    match entry {
                        Entry::User(text) => {
                            if let Some(text) = GrokRecords.user_text(&text) {
                                self.turn_texts.clear();
                                out.user(&text, at);
                            }
                        }
                        Entry::Summary(text) => out.compaction(Some(text), at),
                        _ => {}
                    }
                }
            }
            Some("assistant") => self.assistant(line, at, out),
            Some("backend_tool_call") => self.backend_call(line, at, out),
            Some("tool_result") => self.tool_result(line, out),
            _ => {}
        }
        self.publish(out);
    }

    fn companion_line(&mut self, line: &Value, out: &mut FeedOut) {
        let Some(params) = line.get("params") else {
            return;
        };
        let Some(update) = params.get("update") else {
            return;
        };
        let meta = params.get("_meta");
        match update.get("sessionUpdate").and_then(Value::as_str) {
            Some("agent_message_chunk") => self.chunk(update, meta),
            Some("tool_call") | Some("tool_call_update") => self.announced(update, out),
            // A turn that was cancelled has no `turn_completed`: the next prompt ends it. A
            // compaction rewrites the record without the model calls before it, which the log
            // still holds.
            Some("turn_completed")
            | Some("user_message_chunk")
            | Some("auto_compact_completed") => self.clear_pending(),
            _ => {}
        }
        self.publish(out);
    }
}

impl GrokFeedReader {
    /// An assistant line: its text, then the tool calls it made, each running until its result.
    fn assistant(&mut self, line: &Value, at: Option<i64>, out: &mut FeedOut) {
        if let Some(text) = line
            .get("content")
            .and_then(|content| joined_text(content, &["text"]))
        {
            self.turn_texts.push(text.clone());
            out.assistant(&text, at);
        }
        for call in line
            .get("tool_calls")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let (Some(id), Some(name)) = (
                text_of(call, "id"),
                call.get("name").and_then(Value::as_str),
            ) else {
                continue;
            };
            let arguments = call
                .get("arguments")
                .and_then(Value::as_str)
                .and_then(|arguments| serde_json::from_str::<Value>(arguments).ok())
                .unwrap_or(Value::Null);
            self.recorded_calls.insert(id.clone());
            if name == "todo_write" {
                if let Some(Entry::Todos { items, merge }) = call_entry(name, &arguments) {
                    out.todos(items, merge, at);
                }
                continue;
            }
            if name == "run_terminal_command" {
                self.commands.insert(id.clone());
            }
            out.tool_call(tool_call(id, name, arguments), at);
        }
    }

    /// A `backend_tool_call` line: a tool Grok's server ran (a web search), recorded with how it
    /// ended and no result line of its own.
    fn backend_call(&mut self, line: &Value, at: Option<i64>, out: &mut FeedOut) {
        let Some(kind) = line.get("kind") else {
            return;
        };
        let (Some(id), Some(tool_type)) = (text_of(kind, "id"), text_of(kind, "tool_type")) else {
            return;
        };
        let action = kind.get("action").cloned().unwrap_or(Value::Null);
        let (category, title, paths) = describe(&tool_type, &action);
        let status = match kind.get("status").and_then(Value::as_str) {
            Some("failed") => ToolStatus::Failed,
            Some("completed") => ToolStatus::Completed,
            _ => ToolStatus::Running,
        };
        self.recorded_calls.insert(id.clone());
        let mut call = tool_call(id, &tool_type, action);
        (call.category, call.title, call.paths, call.status) = (category, title, paths, status);
        out.tool_call(call, at);
    }

    /// A `tool_result` line: the call's output, and the exit code of a command.
    fn tool_result(&mut self, line: &Value, out: &mut FeedOut) {
        let Some(id) = text_of(line, "tool_call_id") else {
            return;
        };
        let output = line
            .get("content")
            .and_then(|content| match content {
                Value::String(text) => Some(text.clone()),
                other => joined_text(other, &["text"]),
            })
            .filter(|text| !text.is_empty());
        let exit_code = output
            .as_deref()
            .filter(|_| self.commands.contains(&id))
            .and_then(exit_code);
        let status = if self.failed.contains(&id) {
            ToolStatus::Failed
        } else {
            ToolStatus::Completed
        };
        out.tool_result(
            &id,
            ToolOutcome {
                status,
                output,
                exit_code,
            },
        );
    }

    /// An `agent_message_chunk`: some text of the model call its `_meta` names.
    fn chunk(&mut self, update: &Value, meta: Option<&Value>) {
        let Some(meta) = meta else {
            return;
        };
        let (Some(prompt), Some(start), Some(text)) = (
            meta.get("promptId").and_then(Value::as_str),
            meta.get("streamStartMs").and_then(Value::as_i64),
            update
                .get("content")
                .and_then(|content| content.get("text"))
                .and_then(Value::as_str),
        ) else {
            return;
        };
        let key = (prompt.to_owned(), start);
        if !self.streams.contains_key(&key) {
            self.stream_order.push(key.clone());
        }
        let id = meta.get("chunkId").and_then(Value::as_i64).unwrap_or(0);
        self.streams
            .entry(key)
            .or_default()
            .chunks
            .insert(id, text.to_owned());
    }

    /// A `tool_call` or `tool_call_update`: the call is announced, filled in, or ended.
    fn announced(&mut self, update: &Value, out: &mut FeedOut) {
        let Some(id) = text_of(update, "toolCallId") else {
            return;
        };
        match update.get("status").and_then(Value::as_str) {
            Some("failed") => {
                // The record's result does not say a call failed; one that came before this
                // line is marked now, one that comes after is marked when it does.
                self.failed.insert(id.clone());
                out.tool_result(
                    &id,
                    ToolOutcome {
                        status: ToolStatus::Failed,
                        output: None,
                        exit_code: None,
                    },
                );
                self.end(&id);
                return;
            }
            Some("completed") => {
                self.end(&id);
                return;
            }
            _ => {}
        }
        // A tool Grok's server runs has no `x.ai/tool` name; it is marked `_meta.backend`.
        let backend = update
            .get("_meta")
            .and_then(|meta| meta.get("backend"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let tool = if backend {
            Some("web_search".to_owned())
        } else {
            update
                .get("_meta")
                .and_then(|meta| meta.get("x.ai/tool"))
                .and_then(|tool| text_of(tool, "name"))
        };
        let kind = text_of(update, "kind");
        let title = text_of(update, "title");
        let known = self.calls.contains_key(&id);
        let entry = self.calls.entry(id.clone()).or_insert_with(|| PendingCall {
            call: ToolCall {
                call_id: id.clone(),
                name: tool.clone().or(title.clone()).unwrap_or_default(),
                category: ToolCategory::Other,
                title: None,
                input: Value::Null,
                paths: Vec::new(),
                status: ToolStatus::Running,
                output: None,
                exit_code: None,
            },
            ended: false,
        });
        if !known {
            self.call_order.push(id);
        }
        if let Some(tool) = tool {
            entry.call.name = tool;
        }
        if let Some(raw) = update.get("rawInput").filter(|raw| !raw.is_null()) {
            entry.call.input = raw.clone();
        }
        let (category, derived, paths) = describe(&entry.call.name, &entry.call.input);
        entry.call.category = match (category, kind.as_deref()) {
            (ToolCategory::Other, Some(kind)) => category_of_kind(kind),
            (category, _) => category,
        };
        // A web search's title is "Web search:" and the query once it has one.
        let title = title
            .map(|title| title.trim_start_matches("Web search:").trim().to_owned())
            .filter(|title| !title.is_empty());
        entry.call.title = derived.or(title);
        entry.call.paths = paths;
    }

    /// Nothing is pending: the turn ended, or was cancelled.
    fn clear_pending(&mut self) {
        self.stream_order.clear();
        self.streams.clear();
        self.call_order.clear();
        self.calls.clear();
    }

    /// The updates say the call `id` ended: the record's result is what shows it.
    fn end(&mut self, id: &str) {
        if let Some(entry) = self.calls.get_mut(id) {
            entry.ended = true;
        }
    }

    /// What the updates show that the record does not hold yet, whole.
    fn publish(&self, out: &mut FeedOut) {
        let mut pending = Vec::new();
        for key in &self.stream_order {
            let text = self.streams[key].text();
            let text = text.trim();
            if !text.is_empty() && !self.turn_texts.iter().any(|held| held == text) {
                pending.push(Pending::Assistant(text.to_owned()));
            }
        }
        for id in &self.call_order {
            let entry = &self.calls[id];
            if !entry.ended && !self.recorded_calls.contains(id) && entry.call.name != "todo_write"
            {
                pending.push(Pending::ToolCall(entry.call.clone()));
            }
        }
        out.set_pending(pending);
    }
}

/// A call of the record: its tool by name, with the arguments the model gave it.
fn tool_call(call_id: String, name: &str, input: Value) -> ToolCall {
    let (category, derived, paths) = describe(name, &input);
    ToolCall {
        call_id,
        name: name.to_owned(),
        category,
        title: derived,
        input,
        paths,
        status: ToolStatus::Running,
        output: None,
        exit_code: None,
    }
}

/// The category, one-line title and changed files of a call of tool `name` with `input`.
fn describe(name: &str, input: &Value) -> (ToolCategory, Option<String>, Vec<String>) {
    let field = |key: &str| text_of(input, key);
    match name {
        "read_file" => (ToolCategory::FileRead, field("target_file"), Vec::new()),
        "list_dir" => (ToolCategory::Search, field("target_directory"), Vec::new()),
        "grep" => (ToolCategory::Search, field("pattern"), Vec::new()),
        "search_tool" => (ToolCategory::Search, field("query"), Vec::new()),
        "run_terminal_command" | "monitor" => (ToolCategory::Command, field("command"), Vec::new()),
        "search_replace" | "write" => {
            let path = field("file_path");
            let paths = path.iter().cloned().collect();
            (ToolCategory::FileEdit, path, paths)
        }
        "web_fetch" => (ToolCategory::Web, field("url"), Vec::new()),
        // Run by Grok's server: its input is the search action, with the query in it.
        "web_search" => (ToolCategory::Web, field("query"), Vec::new()),
        "spawn_subagent" => (ToolCategory::Subagent, field("description"), Vec::new()),
        "get_command_or_subagent_output"
        | "kill_command_or_subagent"
        | "wait_commands_or_subagents" => (ToolCategory::Subagent, None, Vec::new()),
        // An MCP tool is reached through `use_tool`, named `<server>__<tool>`.
        "use_tool" => {
            let tool = field("tool_name");
            let category = if tool.as_deref().is_some_and(|tool| tool.contains("__")) {
                ToolCategory::Mcp
            } else {
                ToolCategory::Other
            };
            (category, tool, Vec::new())
        }
        _ => (ToolCategory::Other, None, Vec::new()),
    }
}

/// The category for the `kind` an update gives a call of a tool not known by name.
fn category_of_kind(kind: &str) -> ToolCategory {
    match kind {
        "read" => ToolCategory::FileRead,
        "edit" => ToolCategory::FileEdit,
        "execute" => ToolCategory::Command,
        "search" => ToolCategory::Search,
        "fetch" => ToolCategory::Web,
        _ => ToolCategory::Other,
    }
}

/// The exit code a command's result opens with (`exit: 1`).
fn exit_code(output: &str) -> Option<i64> {
    output
        .lines()
        .next()?
        .strip_prefix("exit: ")?
        .trim()
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ConversationBody;

    const RECORD: &str = include_str!("../../../testdata/feed_grok_1.0.89_chat_history.jsonl");
    const UPDATES: &str = include_str!("../../../testdata/feed_grok_1.0.89_updates.jsonl");

    fn lines(text: &str) -> Vec<Value> {
        text.lines()
            .map(|line| serde_json::from_str(line).expect("fixture line"))
            .collect()
    }

    /// A reader and what it has produced, fed from the two fixtures a range of lines at a time.
    struct Feed {
        reader: GrokFeedReader,
        out: FeedOut,
        record: Vec<Value>,
        updates: Vec<Value>,
    }

    impl Feed {
        fn new() -> Self {
            Self {
                reader: GrokFeedReader::default(),
                out: FeedOut::default(),
                record: lines(RECORD),
                updates: lines(UPDATES),
            }
        }

        fn record(&mut self, range: std::ops::Range<usize>) {
            for line in &self.record[range] {
                self.reader.line(line, &mut self.out);
            }
        }

        fn updates(&mut self, range: std::ops::Range<usize>) {
            for line in &self.updates[range] {
                self.reader.companion_line(line, &mut self.out);
            }
        }

        fn bodies(&self) -> Vec<&ConversationBody> {
            self.out.entries().iter().map(|e| &e.body).collect()
        }

        fn pending(&self) -> Vec<&ConversationBody> {
            self.out.pending().iter().map(|entry| &entry.body).collect()
        }
    }

    fn call(body: &ConversationBody) -> &crate::protocol::ToolCallEntry {
        match body {
            ConversationBody::ToolCall(call) => call,
            other => panic!("not a tool call: {other:?}"),
        }
    }

    fn assistant(body: &ConversationBody) -> &str {
        match body {
            ConversationBody::Assistant { text } => text,
            other => panic!("not assistant text: {other:?}"),
        }
    }

    // The fixtures are five stretches of real sessions, each a range of lines in both files:
    // a compacted session cut off mid-turn (record 0..27, updates 0..176), a turn with
    // a cancelled command (27..35, 176..183), a turn with failed edits (35..51, 183..203), two
    // `todo_write` calls (51..60, 203..209) and a web search (60..62, 209..211).
    const COMPACTED: (usize, usize) = (0, 27);
    const COMPACTED_LOG: (usize, usize) = (0, 176);

    fn kinds(feed: &Feed) -> Vec<&'static str> {
        feed.bodies()
            .iter()
            .map(|body| match body {
                ConversationBody::User { .. } => "user",
                ConversationBody::Octoboard { .. } => "octoboard",
                ConversationBody::Assistant { .. } => "assistant",
                ConversationBody::ToolCall(_) => "call",
                ConversationBody::Todos { .. } => "todos",
                ConversationBody::Compaction { .. } => "compaction",
            })
            .collect()
    }

    #[test]
    fn a_compacted_session_reads_as_entries_with_calls_matched_to_their_results() {
        let mut feed = Feed::new();
        feed.record(COMPACTED.0..COMPACTED.1);
        assert_eq!(
            kinds(&feed),
            [
                "user",
                "compaction",
                "assistant",
                "call",
                "assistant",
                "call",
                "call",
                "call",
                "call",
                "call",
                "call",
                "call"
            ]
        );
        let bodies = feed.bodies();
        assert!(
            matches!(bodies[0], ConversationBody::User { text } if text == "Invented request 1.")
        );
        assert!(matches!(
            bodies[1],
            ConversationBody::Compaction { summary: Some(summary) } if summary.starts_with("This session is being continued")
        ));
        let command = call(bodies[3]);
        assert_eq!(command.category, ToolCategory::Command);
        assert_eq!(command.exit_code, Some(0));
        assert_eq!(command.status, ToolStatus::Completed);
        assert_eq!(
            command.output.as_deref(),
            Some("exit: 0\ninvented output\n")
        );
        let read = call(bodies[5]);
        assert_eq!(read.category, ToolCategory::FileRead);
        assert_eq!(
            read.output.as_deref(),
            Some("1→invented line\n2→invented line\n")
        );
        let edit = call(bodies[9]);
        assert_eq!(edit.name, "search_replace");
        assert_eq!(edit.category, ToolCategory::FileEdit);
        assert_eq!(edit.paths.len(), 1);
        assert_eq!(call(bodies[10]).category, ToolCategory::Search);
        let mcp = call(bodies[11]);
        assert_eq!(mcp.category, ToolCategory::Mcp);
        assert_eq!(mcp.title.as_deref(), Some("docs__search"));
        assert!(feed.pending().is_empty());
    }

    #[test]
    fn a_compaction_drops_what_the_log_still_shows_of_the_calls_before_it() {
        let mut feed = Feed::new();
        feed.record(COMPACTED.0..COMPACTED.1);
        // Before the compaction the log holds model calls the rewritten record does not.
        feed.updates(0..125);
        assert!(!feed.pending().is_empty());
        feed.updates(125..COMPACTED_LOG.1);
        assert!(feed.pending().is_empty());
    }

    #[test]
    fn a_turn_cut_off_mid_turn_is_over_when_the_next_prompt_arrives() {
        let mut feed = Feed::new();
        // The last call of the turn is running, and the turn never completes: the log's own
        // update that ends the call (174) is left out, so the prompt is what clears it.
        feed.record(0..25);
        feed.updates(0..174);
        let pending = feed.pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(call(pending[0]).category, ToolCategory::Mcp);
        assert_eq!(call(pending[0]).status, ToolStatus::Running);

        // The next prompt is the next stretch's first line of the log.
        feed.updates(176..177);
        assert!(feed.pending().is_empty());
    }

    #[test]
    fn what_the_log_shows_is_pending_until_the_record_holds_it() {
        let mut feed = Feed::new();
        feed.record(27..33);
        feed.updates(176..180);
        // The model call's text and command are in the log; the record does not hold them yet.
        let pending = feed.pending();
        assert_eq!(pending.len(), 2);
        assert_eq!(assistant(pending[0]), "Invented reply 7.");
        let command = call(pending[1]);
        assert_eq!(command.category, ToolCategory::Command);
        assert_eq!(command.status, ToolStatus::Running);
        assert_eq!(kinds(&feed), ["user"]);

        // The record's line commits both.
        feed.record(33..34);
        assert!(feed.pending().is_empty());
        assert_eq!(kinds(&feed), ["user", "assistant", "call"]);
    }

    #[test]
    fn a_call_the_log_says_failed_is_marked_whichever_comes_first() {
        // The record first, and its result line says nothing of a failure.
        let mut feed = Feed::new();
        feed.record(27..35);
        assert_eq!(call(feed.bodies()[2]).status, ToolStatus::Completed);
        feed.updates(176..183);
        let command = call(feed.bodies()[2]);
        assert_eq!(command.status, ToolStatus::Failed);
        assert_eq!(command.output.as_deref(), Some("Invented result."));
        assert!(feed.pending().is_empty());

        // The log first: the result, when the record has it, does not clear the mark.
        let mut feed = Feed::new();
        feed.record(27..34);
        feed.updates(176..183);
        feed.record(34..35);
        assert_eq!(call(feed.bodies()[2]).status, ToolStatus::Failed);
    }

    #[test]
    fn only_the_edits_the_log_says_failed_are_marked() {
        let mut feed = Feed::new();
        feed.record(35..51);
        feed.updates(183..203);
        let statuses: Vec<ToolStatus> = feed
            .bodies()
            .into_iter()
            .filter_map(|body| match body {
                ConversationBody::ToolCall(call) => Some(call.status),
                _ => None,
            })
            .collect();
        assert_eq!(
            statuses,
            [
                ToolStatus::Completed,
                ToolStatus::Failed,
                ToolStatus::Failed
            ]
        );
        assert!(feed.pending().is_empty());
    }

    #[test]
    fn todo_writes_fold_into_one_list_and_are_no_calls() {
        let mut feed = Feed::new();
        feed.record(51..60);
        feed.updates(203..209);
        assert_eq!(
            kinds(&feed),
            ["call", "todos", "call", "todos"],
            "the synthetic user line starts no turn"
        );
        let ConversationBody::Todos { items } = feed.bodies()[3] else {
            panic!("not todos");
        };
        assert_eq!(items.len(), 5);
        assert_eq!(
            items
                .iter()
                .filter(|item| item.status == "completed")
                .count(),
            5
        );
        assert_eq!(call(feed.bodies()[0]).category, ToolCategory::Subagent);
        // A `todo_write` the log announces is not pending either.
        assert!(feed.pending().is_empty());
    }

    #[test]
    fn a_web_search_is_a_web_call_from_the_record_and_from_the_log() {
        // The log announces it before the record has the line.
        let mut feed = Feed::new();
        feed.record(60..61);
        feed.updates(209..210);
        let pending = feed.pending();
        assert_eq!(pending.len(), 1);
        let search = call(pending[0]);
        assert_eq!(
            (search.name.as_str(), search.category),
            ("web_search", ToolCategory::Web)
        );
        assert_eq!(search.title, None);
        assert_eq!(search.status, ToolStatus::Running);

        // The record's line, with the query, takes its place and says how it ended.
        feed.record(61..62);
        assert!(feed.pending().is_empty());
        let search = call(feed.bodies()[1]);
        assert_eq!(
            (search.name.as_str(), search.category),
            ("web_search", ToolCategory::Web)
        );
        assert_eq!(search.title.as_deref(), Some("invented query"));
        assert_eq!(search.status, ToolStatus::Completed);
        feed.updates(210..211);
        assert!(feed.pending().is_empty());

        // A search that failed.
        let mut failed = feed.record[61].clone();
        failed["kind"]["status"] = "failed".into();
        let mut feed = Feed::new();
        feed.reader.line(&failed, &mut feed.out);
        assert_eq!(call(feed.bodies()[0]).status, ToolStatus::Failed);
    }

    #[test]
    fn a_rebuild_reads_the_whole_record_then_the_whole_log() {
        let mut feed = Feed::new();
        let (record, updates) = (feed.record.len(), feed.updates.len());
        feed.record(0..record);
        feed.updates(0..updates);
        assert_eq!(feed.out.entries().len(), 28);
        assert!(feed.pending().is_empty());
    }

    #[test]
    fn lines_that_are_not_conversation_are_passed_over() {
        let mut feed = Feed::new();
        for line in [
            serde_json::json!({"type": "reasoning", "summary": []}),
            serde_json::json!({"type": "tool_result", "content": "no id"}),
            serde_json::json!({"type": "assistant", "tool_calls": [{"id": "c", "name": "grep", "arguments": "{"}]}),
            serde_json::json!({"type": "backend_tool_call", "kind": {"tool_type": "web_search"}}),
        ] {
            feed.reader.line(&line, &mut feed.out);
        }
        for line in [
            serde_json::json!({"method": "session/update"}),
            serde_json::json!({"params": {"update": {"sessionUpdate": "agent_message_chunk"}}}),
            serde_json::json!({"params": {"update": {"sessionUpdate": "tool_call"}}}),
        ] {
            feed.reader.companion_line(&line, &mut feed.out);
        }
        assert_eq!(
            feed.out.entries().len(),
            1,
            "the call with unreadable arguments stays"
        );
        assert!(feed.pending().is_empty());
    }

    fn user_line(text: &str) -> Value {
        serde_json::json!({"type": "user", "content": [{"type": "text", "text": text}]})
    }

    fn chunk(prompt: &str, start: i64, id: i64, text: &str) -> Value {
        serde_json::json!({"params": {
            "update": {"sessionUpdate": "agent_message_chunk",
                       "content": {"type": "text", "text": text}},
            "_meta": {"promptId": prompt, "streamStartMs": start, "chunkId": id}}})
    }

    fn announce(id: &str, status: Option<&str>, tool: &str, input: Value) -> Value {
        let mut update = serde_json::json!({
            "sessionUpdate": "tool_call", "toolCallId": id, "title": tool,
            "rawInput": input, "_meta": {"x.ai/tool": {"name": tool}}});
        if let Some(status) = status {
            update["sessionUpdate"] = "tool_call_update".into();
            update["status"] = status.into();
        }
        serde_json::json!({"params": {"update": update}})
    }

    fn done(reader: &mut GrokFeedReader, out: &mut FeedOut, record: &[Value], updates: &[Value]) {
        for line in record {
            reader.line(line, out);
        }
        for line in updates {
            reader.companion_line(line, out);
        }
    }

    /// The updates log sits beside the chat history in the same directory.
    #[test]
    fn the_updates_log_beside_the_record_is_its_companion() {
        assert_eq!(
            GrokRecords.feed_companion(std::path::Path::new(
                "/home/sessions/abc/chat_history.jsonl"
            )),
            Some(std::path::PathBuf::from("/home/sessions/abc/updates.jsonl"))
        );
    }

    /// Text streamed by one model call is joined in the order of its chunk ids whatever order they
    /// were written in, and each model call's text is its own pending entry, in the order they
    /// began.
    #[test]
    fn streamed_text_is_grouped_by_model_call_and_ordered_by_chunk() {
        let mut reader = GrokFeedReader::default();
        let mut out = FeedOut::default();
        done(
            &mut reader,
            &mut out,
            &[user_line("go")],
            &[
                chunk("p1", 1, 2, "world"),
                chunk("p1", 1, 1, "hello "),
                chunk("p2", 9, 1, "second call"),
            ],
        );
        let pending: Vec<&str> = out
            .pending()
            .iter()
            .map(|entry| assistant(&entry.body))
            .collect();
        assert_eq!(pending, ["hello world", "second call"]);
        assert!(out.pending().iter().all(|entry| entry.turn == 1));
        assert_eq!(out.entries().len(), 1);
    }

    /// A call the updates announced is pending, until they say it ended or the turn is over: a
    /// finished call shows once its result is in the record, not before as a running one.
    #[test]
    fn an_announced_call_is_pending_until_the_updates_say_it_ended() {
        let mut reader = GrokFeedReader::default();
        let mut out = FeedOut::default();
        let read = serde_json::json!({"target_file": "a.rs"});
        done(
            &mut reader,
            &mut out,
            &[user_line("go")],
            &[
                announce("c1", None, "read_file", read.clone()),
                announce("c2", None, "grep", serde_json::json!({"pattern": "x"})),
                announce("c3", None, "web_fetch", serde_json::json!({"url": "u"})),
            ],
        );
        assert_eq!(out.pending().len(), 3);
        done(
            &mut reader,
            &mut out,
            &[],
            &[
                announce("c1", Some("completed"), "read_file", read),
                announce("c2", Some("failed"), "grep", Value::Null),
            ],
        );
        let pending: Vec<_> = out
            .pending()
            .iter()
            .map(|entry| call(&entry.body).name.clone())
            .collect();
        assert_eq!(pending, ["web_fetch"]);
        // A call the updates failed that the record gets later is a failed entry.
        done(
            &mut reader,
            &mut out,
            &[
                serde_json::json!({"type": "assistant", "tool_calls": [
                    {"id": "c2", "name": "grep", "arguments": "{\"pattern\": \"x\"}"}]}),
                serde_json::json!({"type": "tool_result", "tool_call_id": "c2", "content": "no match"}),
            ],
            &[],
        );
        let entry = call(&out.entries().last().unwrap().body);
        assert_eq!(
            (entry.status, entry.output.as_deref()),
            (ToolStatus::Failed, Some("no match"))
        );
        done(
            &mut reader,
            &mut out,
            &[],
            &[serde_json::json!({"params": {"update": {"sessionUpdate": "turn_completed"}}})],
        );
        assert!(out.pending().is_empty());
    }

    /// What the updates show of a turn is dropped when the turn is over: it completed, the user's
    /// next prompt arrived (the turn was cancelled, which writes no `turn_completed`), or Grok
    /// compacted the conversation (the record no longer holds the model calls before it).
    #[test]
    fn every_end_of_a_turn_clears_what_is_pending() {
        for end in [
            "turn_completed",
            "user_message_chunk",
            "auto_compact_completed",
        ] {
            let mut reader = GrokFeedReader::default();
            let mut out = FeedOut::default();
            done(
                &mut reader,
                &mut out,
                &[user_line("go")],
                &[
                    chunk("p1", 1, 1, "typing"),
                    announce("c1", None, "grep", serde_json::json!({"pattern": "x"})),
                ],
            );
            assert_eq!(out.pending().len(), 2, "{end}");
            done(
                &mut reader,
                &mut out,
                &[],
                &[serde_json::json!({"params": {"update": {"sessionUpdate": end}}})],
            );
            assert!(out.pending().is_empty(), "{end}");
        }
    }

    /// A call the server runs (marked `_meta.backend`, with no tool name) is pending as a web call,
    /// titled by the query once the update gives one.
    #[test]
    fn a_pending_backend_call_is_a_web_call() {
        let mut reader = GrokFeedReader::default();
        let mut out = FeedOut::default();
        let update = |title: &str, input: Value| {
            serde_json::json!({"params": {"update": {
                "sessionUpdate": "tool_call", "toolCallId": "w", "title": title,
                "rawInput": input, "_meta": {"backend": true}}}})
        };
        done(
            &mut reader,
            &mut out,
            &[user_line("go")],
            &[update("Web search:", Value::Null)],
        );
        let searching = call(&out.pending()[0].body);
        assert_eq!(
            (searching.category, searching.title.as_deref()),
            (ToolCategory::Web, None)
        );
        done(
            &mut reader,
            &mut out,
            &[],
            &[update(
                "Web search: rust news",
                serde_json::json!({"query": "rust news"}),
            )],
        );
        let searching = call(&out.pending()[0].body);
        assert_eq!(
            (searching.category, searching.title.as_deref()),
            (ToolCategory::Web, Some("rust news"))
        );
    }

    /// A command's result opens with its exit code, which only a command's result is read for.
    #[test]
    fn a_commands_result_gives_its_exit_code_and_no_other_does() {
        let mut reader = GrokFeedReader::default();
        let mut out = FeedOut::default();
        done(
            &mut reader,
            &mut out,
            &[
                serde_json::json!({"type": "assistant", "tool_calls": [
                    {"id": "cmd", "name": "run_terminal_command", "arguments": "{\"command\": \"false\"}"},
                    {"id": "read", "name": "read_file", "arguments": "{\"target_file\": \"a\"}"}]}),
                serde_json::json!({"type": "tool_result", "tool_call_id": "cmd", "content": "exit: 2\nboom"}),
                serde_json::json!({"type": "tool_result", "tool_call_id": "read", "content": "exit: 2\nnot a command"}),
            ],
            &[],
        );
        assert_eq!(call(&out.entries()[0].body).exit_code, Some(2));
        assert_eq!(call(&out.entries()[1].body).exit_code, None);
        assert_eq!(
            call(&out.entries()[0].body).output.as_deref(),
            Some("exit: 2\nboom")
        );
    }

    /// A tool reached through `use_tool` is an MCP tool when its name has a server in it, and the
    /// record's `todo_write` merge updates the list by id.
    #[test]
    fn mcp_tools_and_todo_merges_read_from_the_record() {
        let mut reader = GrokFeedReader::default();
        let mut out = FeedOut::default();
        let todos = |merge: bool, items: Value| {
            serde_json::json!({"type": "assistant", "tool_calls": [{
                "id": format!("t{merge}"), "name": "todo_write",
                "arguments": serde_json::json!({"merge": merge, "todos": items}).to_string()}]})
        };
        done(
            &mut reader,
            &mut out,
            &[
                serde_json::json!({"type": "assistant", "tool_calls": [
                    {"id": "m", "name": "use_tool", "arguments": "{\"tool_name\": \"docs__search\"}"},
                    {"id": "o", "name": "use_tool", "arguments": "{\"tool_name\": \"plain\"}"}]}),
                todos(
                    false,
                    serde_json::json!([
                    {"id": "1", "content": "a", "status": "pending"},
                    {"id": "2", "content": "b", "status": "pending"}]),
                ),
                todos(
                    true,
                    serde_json::json!([{"id": "2", "content": "b", "status": "completed"}]),
                ),
            ],
            &[],
        );
        let bodies = out.entries();
        assert_eq!(call(&bodies[0].body).category, ToolCategory::Mcp);
        assert_eq!(call(&bodies[1].body).category, ToolCategory::Other);
        let ConversationBody::Todos { items } = &bodies[3].body else {
            panic!("todos");
        };
        let shown: Vec<String> = items
            .iter()
            .map(|i| format!("{}:{}", i.text, i.status))
            .collect();
        assert_eq!(shown, ["a:pending", "b:completed"]);
        assert!(bodies.iter().all(|entry| !matches!(&entry.body,
            ConversationBody::ToolCall(call) if call.name == "todo_write")));
    }
}
