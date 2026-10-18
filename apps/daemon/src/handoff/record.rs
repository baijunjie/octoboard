//! Reads an agent's conversation record into one common sequence, whichever agent wrote it: what
//! the user said, what the model replied, the todo list as it stood, and the files its edit and
//! write tool calls named. Tool results, file contents read, command output and the model's
//! reasoning are dropped; so is whatever the agent injects into the conversation on its own
//! (system reminders, project instructions, task notifications). The summary an agent writes when
//! it compacts its context is kept as a summary where the agent rewrote its history (Grok Build);
//! where the earlier messages stay in the record (Claude Code, Codex) it is dropped.
//!
//! Nothing is written, and nothing is taken from any file but the record itself. A line that does
//! not parse is skipped, so a record still being written, or one in a shape this does not know,
//! yields what could be read rather than a failure.

use std::io::BufRead;
use std::path::Path;

use serde_json::Value;

use crate::protocol::Agent;

/// One item of a todo list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Todo {
    /// Present for an agent whose list is updated item by item.
    pub id: Option<String>,
    pub text: String,
    /// The agent's own word: `pending`, `in_progress`, `completed` or `cancelled`.
    pub status: String,
}

/// One thing a record says, in the order it says it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    User(String),
    Assistant(String),
    /// The summary an agent wrote of the conversation before it compacted its history.
    Summary(String),
    /// A todo list written by the agent. `merge` says the items update those of the list before
    /// them, matched by id, instead of replacing it.
    Todos {
        items: Vec<Todo>,
        merge: bool,
    },
    /// Files an edit or write tool call named.
    Changed(Vec<String>),
}

/// Who said a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speaker {
    User,
    Assistant,
    /// Not said by either: the agent's own summary of the part of the conversation it compacted.
    Summary,
}

/// What a record comes to once its entries are folded: the whole conversation in order, the todo
/// list as it ended, and the files changed in the order they were first named.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Conversation {
    pub messages: Vec<(Speaker, String)>,
    pub todos: Option<Vec<Todo>>,
    pub changed: Vec<String>,
}

impl Conversation {
    /// Whether the agent compacted its history, so that the messages here start at the summary.
    pub fn compacted(&self) -> bool {
        self.messages
            .iter()
            .any(|(speaker, _)| *speaker == Speaker::Summary)
    }

    fn fold(&mut self, entry: Entry) {
        match entry {
            Entry::User(text) => self.messages.push((Speaker::User, text)),
            Entry::Assistant(text) => self.messages.push((Speaker::Assistant, text)),
            Entry::Summary(text) => self.messages.push((Speaker::Summary, text)),
            Entry::Todos { items, merge } => match self.todos.as_mut().filter(|_| merge) {
                Some(list) => {
                    for item in items {
                        let known = item
                            .id
                            .as_ref()
                            .and_then(|id| list.iter_mut().find(|old| old.id.as_ref() == Some(id)));
                        match known {
                            Some(old) => *old = item,
                            None => list.push(item),
                        }
                    }
                }
                None => self.todos = Some(items),
            },
            Entry::Changed(paths) => {
                for path in paths {
                    if !self.changed.contains(&path) {
                        self.changed.push(path);
                    }
                }
            }
        }
    }
}

/// Reads the conversation held in the record file at `path`.
pub fn read(agent: Agent, path: &Path) -> std::io::Result<Conversation> {
    let mut reader = std::io::BufReader::new(std::fs::File::open(path)?);
    let mut conversation = Conversation::default();
    let mut line = Vec::new();
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 {
            return Ok(conversation);
        }
        let Ok(value) = serde_json::from_slice::<Value>(&line) else {
            continue;
        };
        for entry in line_entries(agent, &value) {
            conversation.fold(entry);
        }
    }
}

/// The entries one line of a record holds.
pub fn line_entries(agent: Agent, line: &Value) -> Vec<Entry> {
    match agent {
        Agent::Claude => claude(line),
        Agent::Codex => codex(line),
        Agent::Grok => grok(line),
    }
}

fn text_of(value: &Value, key: &str) -> Option<String> {
    let text = value.get(key)?.as_str()?.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// The text blocks of a message's `content` joined, or the string itself when it is one.
fn joined_text(content: &Value, kinds: &[&str]) -> Option<String> {
    let text = match content {
        Value::String(text) => text.trim().to_owned(),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|block| {
                block
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| kinds.contains(&kind))
            })
            .filter_map(|block| block.get("text")?.as_str())
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n"),
        _ => return None,
    };
    (!text.is_empty()).then_some(text)
}

fn todos<'a>(
    list: &'a Value,
    id: Option<&'a str>,
    text: &'a str,
) -> impl Iterator<Item = Todo> + 'a {
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(move |item| {
            Some(Todo {
                id: id.and_then(|key| text_of(item, key)),
                text: text_of(item, text)?,
                status: text_of(item, "status").unwrap_or_else(|| "pending".to_owned()),
            })
        })
}

// -- Claude Code ---------------------------------------------------------------

/// Claude Code's transcript: one JSON object a line, `type` `user` or `assistant`, the message
/// under `message`. A user line carrying a tool result is not something the user said, and a
/// subagent's lines (`isSidechain`) are another conversation.
fn claude(line: &Value) -> Vec<Entry> {
    let flag = |key| line.get(key).and_then(Value::as_bool).unwrap_or(false);
    if flag("isSidechain") {
        return Vec::new();
    }
    if line.get("type").and_then(Value::as_str) == Some("attachment") {
        return claude_queued(line).into_iter().collect();
    }
    let Some(message) = line.get("message") else {
        return Vec::new();
    };
    match line.get("type").and_then(Value::as_str) {
        Some("user") if !flag("isMeta") && !flag("isCompactSummary") && from_user(line) => {
            let text = message
                .get("content")
                .and_then(|content| joined_text(content, &["text"]))
                .filter(|text| {
                    !text.starts_with("<local-command-") && !text.starts_with("<task-notification>")
                });
            text.map(Entry::User).into_iter().collect()
        }
        // Claude Code writes a stand-in assistant line for an API error or an empty turn.
        Some("assistant")
            if message.get("model").and_then(Value::as_str) != Some("<synthetic>") =>
        {
            let blocks = message
                .get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten();
            let mut entries = Vec::new();
            for block in blocks {
                match block.get("type").and_then(Value::as_str) {
                    Some("text") => entries.extend(text_of(block, "text").map(Entry::Assistant)),
                    Some("tool_use") => entries.extend(claude_tool(block)),
                    _ => {}
                }
            }
            entries
        }
        _ => Vec::new(),
    }
}

/// Whether a line says it carries what a person typed. Claude Code 2.1.295 and later mark who
/// produced a user line in `origin.kind`: `human` for the person, otherwise a background task's
/// notification, another session's message and the like. Earlier lines carry no origin.
fn from_user(line: &Value) -> bool {
    line.get("origin")
        .and_then(|origin| origin.get("kind"))
        .and_then(Value::as_str)
        .is_none_or(|kind| kind == "human")
}

/// A message typed while the model was working. Claude Code 2.1.295 to 2.1.302 records it only as
/// a `queued_command` attachment, with no user line of its own.
fn claude_queued(line: &Value) -> Option<Entry> {
    let attachment = line.get("attachment")?;
    if attachment.get("type")?.as_str()? != "queued_command" {
        return None;
    }
    // An attachment with no origin is no proof of a person; only an explicit `human` counts.
    (attachment.get("origin")?.get("kind")?.as_str()? == "human")
        .then(|| joined_text(attachment.get("prompt")?, &["text"]))
        .flatten()
        .map(Entry::User)
}

fn claude_tool(block: &Value) -> Option<Entry> {
    let input = block.get("input")?;
    match block.get("name")?.as_str()? {
        "TodoWrite" => Some(Entry::Todos {
            items: todos(input.get("todos")?, None, "content").collect(),
            merge: false,
        }),
        "Edit" | "MultiEdit" | "Write" => Some(Entry::Changed(vec![text_of(input, "file_path")?])),
        "NotebookEdit" => Some(Entry::Changed(vec![text_of(input, "notebook_path")?])),
        _ => None,
    }
}

// -- Codex ---------------------------------------------------------------------

/// Codex's rollout. What was said comes from the `event_msg` lines, which hold what the user typed
/// and what the model answered (`user_message` and `agent_message` in older Codex, `item_completed`
/// of a `UserMessage` or `AgentMessage` item in newer); the `response_item` messages beside them
/// include the instructions and environment context Codex adds itself. Files changed come from the
/// `FileChange` item of newer Codex, or an `apply_patch` call in the response items of older.
fn codex(line: &Value) -> Vec<Entry> {
    let Some(payload) = line.get("payload") else {
        return Vec::new();
    };
    if let Some(typed) = crate::adapter::codex::typed_text(line) {
        let typed = typed.trim();
        return if typed.is_empty() {
            Vec::new()
        } else {
            vec![Entry::User(typed.to_owned())]
        };
    }
    match line.get("type").and_then(Value::as_str) {
        Some("event_msg") => match payload.get("type").and_then(Value::as_str) {
            Some("agent_message") => text_of(payload, "message")
                .map(Entry::Assistant)
                .into_iter()
                .collect(),
            Some("item_completed") => payload.get("item").map(codex_item).unwrap_or_default(),
            _ => Vec::new(),
        },
        Some("response_item") => match payload.get("type").and_then(Value::as_str) {
            Some("function_call") => codex_call(payload, "arguments"),
            Some("custom_tool_call") => codex_call(payload, "input"),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

fn codex_item(item: &Value) -> Vec<Entry> {
    match item.get("type").and_then(Value::as_str) {
        Some("AgentMessage") => item
            .get("content")
            .and_then(|content| joined_text(content, &["Text", "text"]))
            .map(Entry::Assistant)
            .into_iter()
            .collect(),
        // A change that did not go through changed nothing.
        Some("FileChange")
            if item
                .get("status")
                .and_then(Value::as_str)
                .is_none_or(|status| status == "completed") =>
        {
            let paths: Vec<String> = item
                .get("changes")
                .and_then(Value::as_object)
                .into_iter()
                .flat_map(|changes| {
                    changes.iter().flat_map(|(path, change)| {
                        [Some(path.clone()), text_of(change, "move_path")]
                            .into_iter()
                            .flatten()
                    })
                })
                .collect();
            if paths.is_empty() {
                Vec::new()
            } else {
                vec![Entry::Changed(paths)]
            }
        }
        _ => Vec::new(),
    }
}

/// A tool call of the response items: the plan the model keeps (`update_plan`, whose arguments
/// are a JSON string) and a patch (`apply_patch`, whose input is the patch text).
fn codex_call(call: &Value, field: &str) -> Vec<Entry> {
    let Some(input) = call.get(field).and_then(Value::as_str) else {
        return Vec::new();
    };
    match call.get("name").and_then(Value::as_str) {
        Some("update_plan") => serde_json::from_str::<Value>(input)
            .ok()
            .and_then(|arguments| {
                Some(Entry::Todos {
                    items: todos(arguments.get("plan")?, None, "step").collect(),
                    merge: false,
                })
            })
            .into_iter()
            .collect(),
        Some("apply_patch") => {
            let paths: Vec<String> = input
                .lines()
                .filter_map(|line| {
                    [
                        "*** Add File: ",
                        "*** Update File: ",
                        "*** Delete File: ",
                        "*** Move to: ",
                    ]
                    .iter()
                    .find_map(|prefix| line.strip_prefix(prefix))
                })
                .map(|path| path.trim().to_owned())
                .collect();
            if paths.is_empty() {
                Vec::new()
            } else {
                vec![Entry::Changed(paths)]
            }
        }
        _ => Vec::new(),
    }
}

// -- Grok Build ----------------------------------------------------------------

/// Grok Build's `chat_history.jsonl`: `user` and `assistant` lines, the latter carrying the tool
/// calls the model made. A user line with a `synthetic_reason` is something Grok put in the
/// conversation itself (project instructions, system reminders, a compaction note), and a real
/// prompt is sometimes wrapped in `<user_query>`.
/// How the summary Grok Build writes at a compaction begins.
const COMPACTION_SUMMARY: &str = "This session is being continued from a previous conversation";

fn grok(line: &Value) -> Vec<Entry> {
    match line.get("type").and_then(Value::as_str) {
        Some("user")
            if line.get("synthetic_reason").and_then(Value::as_str) == Some("compaction_meta") =>
        {
            // Grok rewrites its history when it compacts: the summary stands in for what came
            // before. The other compaction line it adds restates the rules and environment.
            line.get("content")
                .and_then(|content| joined_text(content, &["text"]))
                .filter(|text| text.starts_with(COMPACTION_SUMMARY))
                .map(Entry::Summary)
                .into_iter()
                .collect()
        }
        Some("user") if line.get("synthetic_reason").is_none() => {
            let blocks = line.get("content").and_then(Value::as_array);
            let texts: Vec<String> = blocks
                .into_iter()
                .flatten()
                .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|block| block.get("text")?.as_str())
                .filter(|text| {
                    !["<user_info>", "<image_files>", "<system-reminder>"]
                        .iter()
                        .any(|tag| text.trim_start().starts_with(tag))
                })
                .map(|text| {
                    let text = text.trim();
                    text.strip_prefix("<user_query>")
                        .and_then(|inner| inner.strip_suffix("</user_query>"))
                        .unwrap_or(text)
                        .trim()
                        .to_owned()
                })
                .filter(|text| !text.is_empty())
                .collect();
            if texts.is_empty() {
                Vec::new()
            } else {
                vec![Entry::User(texts.join("\n\n"))]
            }
        }
        Some("assistant") => {
            let mut entries = Vec::new();
            entries.extend(
                line.get("content")
                    .and_then(|content| joined_text(content, &["text"]))
                    .map(Entry::Assistant),
            );
            for call in line
                .get("tool_calls")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                entries.extend(grok_call(call));
            }
            entries
        }
        _ => Vec::new(),
    }
}

fn grok_call(call: &Value) -> Option<Entry> {
    let arguments: Value = serde_json::from_str(call.get("arguments")?.as_str()?).ok()?;
    match call.get("name")?.as_str()? {
        "todo_write" => Some(Entry::Todos {
            items: todos(arguments.get("todos")?, Some("id"), "content").collect(),
            merge: arguments
                .get("merge")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        "search_replace" | "write" => Some(Entry::Changed(vec![text_of(&arguments, "file_path")?])),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture(name: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join(name)
    }

    fn say(speaker: Speaker, text: &str) -> (Speaker, String) {
        (speaker, text.to_owned())
    }

    fn todo(id: Option<&str>, text: &str, status: &str) -> Todo {
        Todo {
            id: id.map(str::to_owned),
            text: text.to_owned(),
            status: status.to_owned(),
        }
    }

    /// Nothing the records carry as tool output, reasoning or injected context is in `text`.
    fn assert_clean(conversation: &Conversation) {
        let everything = format!("{conversation:?}");
        for leaked in ["TOOL-OUTPUT", "REASONING", "INJECTED", "SIDECHAIN"] {
            assert!(
                !everything.contains(leaked),
                "{leaked} leaked: {everything}"
            );
        }
        let paths: std::collections::HashSet<_> = conversation.changed.iter().collect();
        assert_eq!(paths.len(), conversation.changed.len());
    }

    #[test]
    fn claude_keeps_what_was_said_and_the_last_todo_list_and_the_files_changed() {
        let conversation = read(Agent::Claude, &capture("handoff_claude_2.1.302.jsonl")).unwrap();
        // The second user message was typed mid-turn, which this version records only as a queued
        // attachment; a background task's notification, as a user line and as an attachment, is
        // no one's message.
        assert_eq!(
            conversation.messages,
            [
                say(Speaker::User, "Fix the flaky test"),
                say(Speaker::Assistant, "I will look at the test first."),
                say(Speaker::User, "Also run the linter"),
                say(Speaker::User, "And mind the docs"),
                say(Speaker::Assistant, "Done: the test and the linter pass."),
            ]
        );
        assert_eq!(
            conversation.todos,
            Some(vec![
                todo(None, "Fix the test", "completed"),
                todo(None, "Run the linter", "in_progress"),
            ])
        );
        assert_eq!(
            conversation.changed,
            ["/work/src/a.rs", "/work/notes.ipynb"]
        );
        assert!(!conversation.compacted());
        assert_clean(&conversation);
    }

    #[test]
    fn codex_keeps_what_was_said_and_the_last_plan_and_the_files_changed() {
        let conversation = read(
            Agent::Codex,
            &capture("handoff_codex_0.162.1_rollout.jsonl"),
        )
        .unwrap();
        // The last user message is in the older `user_message` event. A file change that failed
        // changed nothing, and a move names both ends.
        assert_eq!(
            conversation.messages,
            [
                say(Speaker::User, "Fix the flaky test"),
                say(Speaker::Assistant, "Looking at it now."),
                say(Speaker::User, "Also run the linter"),
                say(Speaker::User, "And mind the docs"),
                say(Speaker::Assistant, "Both pass."),
            ]
        );
        assert_eq!(
            conversation.todos,
            Some(vec![
                todo(None, "Fix the test", "completed"),
                todo(None, "Run the linter", "completed"),
            ])
        );
        assert_eq!(
            conversation.changed,
            [
                "/work/src/a.rs",
                "/work/src/old.rs",
                "/work/src/renamed.rs",
                "/work/src/b.rs",
                "/work/src/c.rs"
            ]
        );
        assert_clean(&conversation);
    }

    #[test]
    fn grok_keeps_what_was_said_and_merges_its_todo_updates_and_lists_the_files_changed() {
        let conversation = read(
            Agent::Grok,
            &capture("handoff_grok_1.0.62_chat_history.jsonl"),
        )
        .unwrap();
        // The compaction line that restates the rules is dropped, the one that summarises the
        // earlier conversation is kept as such.
        assert_eq!(conversation.messages.len(), 5);
        assert_eq!(
            conversation.messages[0],
            say(Speaker::User, "Fix the flaky test")
        );
        assert_eq!(
            conversation.messages[1],
            say(Speaker::Assistant, "Reading the test.")
        );
        assert_eq!(conversation.messages[2].0, Speaker::Summary);
        assert!(conversation.messages[2]
            .1
            .starts_with("This session is being continued from a previous conversation"));
        assert_eq!(
            conversation.messages[3],
            say(Speaker::User, "Also run the linter")
        );
        assert_eq!(
            conversation.messages[4],
            say(Speaker::Assistant, "Both pass.")
        );
        assert!(conversation.compacted());
        assert_eq!(
            conversation.todos,
            Some(vec![
                todo(Some("test"), "Fix the test", "completed"),
                todo(Some("lint"), "Run the linter", "completed"),
            ])
        );
        assert_eq!(
            conversation.changed,
            ["/work/src/a.rs", "/tmp/scratch.swift"]
        );
        assert_clean(&conversation);
    }

    #[test]
    fn a_line_that_does_not_parse_is_skipped_and_a_missing_record_is_an_error() {
        let dir = crate::test_support::ScratchDir::new("handoff-record");
        let path = dir.join("record.jsonl");
        std::fs::write(
            &path,
            "not json\n{\"type\":\"user\",\"message\":{\"content\":\"hello\"}}\n{\"type\":\"user\",\"mess",
        )
        .unwrap();
        let conversation = read(Agent::Claude, &path).unwrap();
        assert_eq!(conversation.messages, [say(Speaker::User, "hello")]);
        assert!(read(Agent::Claude, &dir.join("missing.jsonl")).is_err());
    }
}
