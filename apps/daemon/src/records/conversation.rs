//! Reads an agent's conversation record into one common sequence, whichever agent wrote it: what
//! the user said, what the model replied, the todo list as it stood, and the files its edit and
//! write tool calls named. Tool results, file contents read, command output and the model's
//! reasoning are dropped; so is whatever the agent injects into the conversation on its own
//! (system reminders, project instructions, task notifications). The summary an agent writes when
//! it compacts its context is kept as a summary where the agent rewrote its history (Grok Build);
//! where the earlier messages stay in the record (Claude Code, Codex) it is dropped.
//!
//! How one provider's lines say each of these is its record store's (`RecordStore::entries`);
//! what is common to them is here.
//!
//! Nothing is written, and nothing is taken from any file but the record itself. A line that does
//! not parse is skipped, so a record still being written, or one in a shape this does not know,
//! yields what could be read rather than a failure.

use std::ops::ControlFlow;

use serde_json::Value;

use crate::adapter::provider;
use crate::protocol::AgentId;
use crate::records::line_file;
use crate::records::Stretch;

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

/// Reads the conversation made of `stretches` in order, see [`for_each_line`], as `agent`'s store
/// reads each line (`RecordStore::entries`).
pub fn read_stretches(agent: AgentId, stretches: &[Stretch]) -> std::io::Result<Conversation> {
    let store = provider(agent).records();
    let mut conversation = Conversation::default();
    for_each_line(stretches, |value| {
        for entry in store.entries(value) {
            conversation.fold(entry);
        }
    })?;
    Ok(conversation)
}

/// Calls `visit` with each line of `stretches` that parses, in order: each file, and of it the
/// first `cut` bytes when there is one (a Codex thread's earlier segments end where the next picks
/// up), decompressing a Codex rollout that Codex has compressed. A line past the line limit is
/// skipped, like one that does not parse. A stretch that fails part-way (a compressed record cut
/// short) yields what was read before it; one that fails before a line of it was read is the
/// error.
pub fn for_each_line(stretches: &[Stretch], mut visit: impl FnMut(&Value)) -> std::io::Result<()> {
    for Stretch { path, cut } in stretches {
        let read = line_file::read_whole_until(path, *cut, |line| {
            if let Ok(value) = serde_json::from_str::<Value>(line) {
                visit(&value);
            }
            ControlFlow::Continue(())
        });
        if let Err(failure) = read {
            if !failure.read_a_line {
                return Err(failure.error);
            }
        }
    }
    Ok(())
}

/// The text `value` holds at `key`, trimmed, when that is not nothing.
pub(crate) fn text_of(value: &Value, key: &str) -> Option<String> {
    let text = value.get(key)?.as_str()?.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// The text blocks of a message's `content` joined, or the string itself when it is one.
pub(crate) fn joined_text(content: &Value, kinds: &[&str]) -> Option<String> {
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

/// The items of a todo `list`, each with its id at `id` (for a list updated item by item) and its
/// text at `text`.
pub(crate) fn todos<'a>(
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn read(agent: AgentId, path: &Path) -> std::io::Result<Conversation> {
        let path = path.to_path_buf();
        read_stretches(agent, &[Stretch { path, cut: None }])
    }

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
        let conversation = read(AgentId::CLAUDE, &capture("handoff_claude_2.1.302.jsonl")).unwrap();
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
            AgentId::CODEX,
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
            AgentId::GROK,
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
        let conversation = read(AgentId::CLAUDE, &path).unwrap();
        assert_eq!(conversation.messages, [say(Speaker::User, "hello")]);
        assert!(read(AgentId::CLAUDE, &dir.join("missing.jsonl")).is_err());
    }

    /// A compressed record cut short yields the messages before the cut, not a failure.
    #[test]
    fn a_compressed_record_cut_short_yields_what_came_before_the_cut() {
        let dir = crate::test_support::ScratchDir::new("handoff-record-truncated");
        let path = dir.join("record.jsonl.zst");
        let record: String = (0..40_000)
            .map(|n| format!("{{\"type\":\"user\",\"message\":{{\"content\":\"message {n}\"}}}}\n"))
            .collect();
        let mut packed = zstd::encode_all(record.as_bytes(), 0).unwrap();
        packed.truncate(packed.len() / 2);
        std::fs::write(&path, packed).unwrap();

        let conversation = read(AgentId::CLAUDE, &path).unwrap();

        assert!(!conversation.messages.is_empty());
        assert!(conversation.messages.len() < 40_000);
    }

    /// A compressed stretch that fails after reading only lines that are skipped yields nothing and
    /// the other stretches are still read, like a record cut short after lines that parsed.
    #[test]
    fn a_stretch_that_fails_after_only_skipped_lines_yields_nothing_and_the_read_goes_on() {
        let dir = crate::test_support::ScratchDir::new("conversation-skipped-then-failed");
        let broken = dir.join("broken.jsonl.zst");
        std::fs::write(&broken, cut_short_after(b"\xff\xfe\n")).unwrap();
        let whole = dir.join("whole.jsonl");
        std::fs::write(&whole, format!("{}\n", user_line("after"))).unwrap();
        let stretches = [
            Stretch {
                path: broken,
                cut: None,
            },
            Stretch {
                path: whole,
                cut: None,
            },
        ];

        let conversation = read_stretches(AgentId::CLAUDE, &stretches).unwrap();

        assert_eq!(conversation.messages, [say(Speaker::User, "after")]);
    }

    /// A stretch that fails inside its first line, before a line of it was read, is the error.
    #[test]
    fn a_stretch_that_fails_inside_its_first_line_is_the_error() {
        let dir = crate::test_support::ScratchDir::new("conversation-failed-in-first-line");
        let broken = dir.join("broken.jsonl.zst");
        let unfinished = user_line("unfinished");
        std::fs::write(&broken, cut_short_after(unfinished.as_bytes())).unwrap();
        let stretches = [Stretch {
            path: broken,
            cut: None,
        }];

        assert!(read_stretches(AgentId::CLAUDE, &stretches).is_err());
    }

    /// A zstd record that decompresses to `content` and then fails: one whole frame holding
    /// `content`, followed by half of a second one.
    fn cut_short_after(content: &[u8]) -> Vec<u8> {
        let mut packed = zstd::encode_all(content, 0).unwrap();
        let next = zstd::encode_all(&b"more lines\n"[..], 0).unwrap();
        packed.extend_from_slice(&next[..next.len() / 2]);
        packed
    }

    fn user_line(text: &str) -> String {
        format!(r#"{{"type":"user","message":{{"role":"user","content":"{text}"}}}}"#)
    }

    /// Lines that do not parse are skipped, the last line counts without its newline, and a stretch
    /// ends at its cut.
    #[test]
    fn lines_are_read_through_the_cut_and_the_unfinished_last_one() {
        let path = crate::test_support::ScratchFile::new("conversation-lines", "session.jsonl");
        let (one, two, three) = (user_line("one"), user_line("two"), user_line("three"));
        std::fs::write(&path, format!("{one}\nnot json\n{two}\n{three}")).unwrap();
        let said = |cut| {
            let stretches = [Stretch {
                path: path.to_path_buf(),
                cut,
            }];
            let conversation = read_stretches(AgentId::CLAUDE, &stretches).unwrap();
            conversation.messages
        };
        assert_eq!(
            said(None),
            [
                say(Speaker::User, "one"),
                say(Speaker::User, "two"),
                say(Speaker::User, "three")
            ]
        );
        let cut = (one.len() + "\nnot json\n".len() + two.len() + 1) as u64;
        assert_eq!(
            said(Some(cut)),
            [say(Speaker::User, "one"), say(Speaker::User, "two")]
        );
        let missing = path.with_file_name("missing.jsonl");
        let stretches = [Stretch {
            path: missing,
            cut: None,
        }];
        assert!(read_stretches(AgentId::CLAUDE, &stretches).is_err());
    }

    /// A line past the line limit is skipped like one that does not parse, and the lines after it
    /// are still read.
    #[test]
    fn a_line_past_the_limit_is_skipped() {
        let path = crate::test_support::ScratchFile::new("conversation-long", "session.jsonl");
        let long = user_line(&"x".repeat(crate::records::line_file::LINE_LIMIT));
        std::fs::write(
            &path,
            format!("{}\n{long}\n{}\n", user_line("one"), user_line("two")),
        )
        .unwrap();
        let conversation = read(AgentId::CLAUDE, &path).unwrap();
        assert_eq!(
            conversation.messages,
            [say(Speaker::User, "one"), say(Speaker::User, "two")]
        );
    }
}
