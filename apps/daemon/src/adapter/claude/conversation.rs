//! How a line of Claude Code's transcript reads as the common conversation
//! (`crate::records::conversation`, `RecordStore::entries`).

use std::borrow::Cow;

use serde_json::Value;

use crate::records::conversation::{joined_text, text_of, todos, Entry, Todo};

/// A user line Claude Code writes that starts no turn: the note left when a turn was interrupted.
const NOT_TYPED: [&str; 1] = ["[Request interrupted by user"];

/// What the user typed, from the text of a user entry (`RecordStore::user_text`): a paste's
/// wrapper taken off, a slash command read as typed ([`slash_command`]), and `None` for a line
/// Claude Code writes that starts no turn, or a command line it cannot be read from.
pub(super) fn user_text(text: &str) -> Option<Cow<'_, str>> {
    let text = crate::text::without_paste_wrapper(text).trim();
    if text.starts_with("<command-") {
        return slash_command(text).map(Cow::Owned);
    }
    (!NOT_TYPED.iter().any(|lead| text.starts_with(lead))).then_some(Cow::Borrowed(text))
}

/// The slash command the user invoked, as typed (`/name args`), from the line Claude Code writes
/// for it (`<command-message>…</command-message>\n<command-name>/x</command-name>\n
/// <command-args>…</command-args>`). The prompt it expands to is a separate line the reader drops.
fn slash_command(text: &str) -> Option<String> {
    let name = tagged(text, "command-name")?;
    let args = tagged(text, "command-args").unwrap_or_default();
    Some(format!("{name} {args}").trim().to_owned())
}

/// What sits between `<tag>` and `</tag>` in `text`, trimmed, when that is not nothing.
fn tagged<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let after = &text[text.find(&format!("<{tag}>"))? + tag.len() + 2..];
    let inner = after[..after.find(&format!("</{tag}>"))?].trim();
    (!inner.is_empty()).then_some(inner)
}

/// Whether `line` has `key` set to true.
pub(super) fn flagged(line: &Value, key: &str) -> bool {
    line.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// The entries of one line of Claude Code's transcript: one JSON object a line, `type` `user` or
/// `assistant`, the message under `message`. A user line carrying a tool result is not something
/// the user said, and a subagent's lines (`isSidechain`) are another conversation.
pub(super) fn entries(line: &Value) -> Vec<Entry> {
    if let Some(text) = user_message(line) {
        return vec![Entry::User(text)];
    }
    let mut entries = Vec::new();
    for block in assistant_blocks(line) {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => entries.extend(text_of(block, "text").map(Entry::Assistant)),
            Some("tool_use") => entries.extend(claude_tool(block)),
            _ => {}
        }
    }
    entries
}

/// What a line of the main conversation says the user sent, before [`user_text`] reads what they
/// typed from it: a user line a person wrote (not a tool result, a compaction summary or one
/// Claude Code wrote itself) or a `queued_command` attachment.
pub(super) fn user_message(line: &Value) -> Option<String> {
    if flagged(line, "isSidechain") {
        return None;
    }
    match line.get("type").and_then(Value::as_str) {
        Some("attachment") => claude_queued(line),
        Some("user")
            if !flagged(line, "isMeta")
                && !flagged(line, "isCompactSummary")
                && from_user(line) =>
        {
            joined_text(line.get("message")?.get("content")?, &["text"]).filter(|text| {
                !text.starts_with("<local-command-") && !text.starts_with("<task-notification>")
            })
        }
        _ => None,
    }
}

/// The content blocks of a main-conversation assistant line. A subagent's lines are another
/// conversation, and Claude Code writes a stand-in `<synthetic>` assistant line for an API error
/// or an empty turn.
pub(super) fn assistant_blocks(line: &Value) -> impl Iterator<Item = &Value> {
    let message = line.get("message").filter(|message| {
        line.get("type").and_then(Value::as_str) == Some("assistant")
            && !flagged(line, "isSidechain")
            && message.get("model").and_then(Value::as_str) != Some("<synthetic>")
    });
    message
        .and_then(|message| message.get("content")?.as_array())
        .into_iter()
        .flatten()
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
fn claude_queued(line: &Value) -> Option<String> {
    let attachment = line.get("attachment")?;
    if attachment.get("type")?.as_str()? != "queued_command" {
        return None;
    }
    // An attachment with no origin is no proof of a person; only an explicit `human` counts.
    (attachment.get("origin")?.get("kind")?.as_str()? == "human")
        .then(|| joined_text(attachment.get("prompt")?, &["text"]))
        .flatten()
}

/// The items of a `TodoWrite` call's `input`.
pub(super) fn todo_items(input: &Value) -> Option<Vec<Todo>> {
    Some(todos(input.get("todos")?, None, "content").collect())
}

fn claude_tool(block: &Value) -> Option<Entry> {
    let input = block.get("input")?;
    match block.get("name")?.as_str()? {
        "TodoWrite" => Some(Entry::Todos {
            items: todo_items(input)?,
            merge: false,
        }),
        "Edit" | "MultiEdit" | "Write" => Some(Entry::Changed(vec![text_of(input, "file_path")?])),
        "NotebookEdit" => Some(Entry::Changed(vec![text_of(input, "notebook_path")?])),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_slash_command_reads_as_typed() {
        assert_eq!(
            slash_command(
                "<command-message>x</command-message>\n<command-name>/x</command-name>\n\
                 <command-args>a b</command-args>"
            )
            .as_deref(),
            Some("/x a b")
        );
        assert_eq!(
            slash_command("<command-name>/clear</command-name>\n<command-args></command-args>")
                .as_deref(),
            Some("/clear")
        );
        assert_eq!(slash_command("<command-message>x</command-message>"), None);
    }

    /// The note an interrupted turn leaves, and a command line with no command in it, are not
    /// what the user typed; a paste is, without its wrapper.
    #[test]
    fn only_what_the_user_typed_reads_as_their_text() {
        assert_eq!(
            user_text("[Request interrupted by user for tool use]"),
            None
        );
        assert_eq!(user_text("<command-message>x</command-message>"), None);
        assert_eq!(
            user_text("<pasted_content id=\"a\">\nbody\n</pasted_content id=\"a\">").as_deref(),
            Some("body")
        );
        assert_eq!(user_text("  typed  ").as_deref(), Some("typed"));
    }

    fn user(content: Value, extra: Value) -> Value {
        let mut line = json!({"type": "user", "message": {"content": content}});
        for (key, value) in extra.as_object().into_iter().flatten() {
            line[key] = value.clone();
        }
        line
    }

    /// What the common conversation (the history's) reads from a line: a user line a person wrote,
    /// a queued command a human sent, the text and the plan and edits of the model's own lines;
    /// nothing from a subagent, a tool's result, a compaction summary, a meta line, a line Claude
    /// Code wrote itself or a stand-in reply.
    #[test]
    fn the_common_entries_of_a_line() {
        let human = json!({"origin": {"kind": "human"}});
        assert_eq!(
            entries(&user(json!("typed"), human.clone())),
            [Entry::User("typed".to_owned())]
        );
        assert_eq!(
            entries(&user(
                json!([{"type": "text", "text": "a"}, {"type": "text", "text": "b"}]),
                json!({})
            )),
            [Entry::User("a\n\nb".to_owned())]
        );
        for line in [
            user(json!("x"), json!({"isSidechain": true})),
            user(json!("x"), json!({"isMeta": true})),
            user(json!("x"), json!({"isCompactSummary": true})),
            user(json!("x"), json!({"origin": {"kind": "task-notification"}})),
            user(
                json!("<local-command-stdout>x</local-command-stdout>"),
                json!({}),
            ),
            user(json!("<task-notification>x</task-notification>"), json!({})),
            user(
                json!([{"type": "tool_result", "tool_use_id": "t", "content": "x"}]),
                json!({}),
            ),
            json!({"type": "user"}),
            json!({"type": "system", "message": {"content": "x"}}),
        ] {
            assert!(entries(&line).is_empty(), "{line}");
        }

        let queued = |origin: &str, sidechain: bool| {
            json!({"type": "attachment", "isSidechain": sidechain, "attachment": {
                "type": "queued_command", "prompt": "later", "origin": {"kind": origin}}})
        };
        assert_eq!(
            entries(&queued("human", false)),
            [Entry::User("later".to_owned())]
        );
        assert!(entries(&queued("peer", false)).is_empty());
        assert!(entries(&queued("human", true)).is_empty());

        let assistant = |model: &str, sidechain: bool| {
            json!({"type": "assistant", "isSidechain": sidechain, "message": {
                "model": model, "content": [
                    {"type": "text", "text": "said"},
                    {"type": "tool_use", "name": "Edit", "input": {"file_path": "/a"}},
                    {"type": "tool_use", "name": "MultiEdit", "input": {"file_path": "/b"}},
                    {"type": "tool_use", "name": "NotebookEdit", "input": {"notebook_path": "/c"}},
                    {"type": "tool_use", "name": "TodoWrite", "input": {"todos": [
                        {"content": "x", "status": "pending"}]}},
                    {"type": "tool_use", "name": "Bash", "input": {"command": "ls"}}]}})
        };
        let read = entries(&assistant("claude", false));
        assert_eq!(read.len(), 5);
        assert_eq!(read[0], Entry::Assistant("said".to_owned()));
        assert_eq!(read[1], Entry::Changed(vec!["/a".to_owned()]));
        assert_eq!(read[2], Entry::Changed(vec!["/b".to_owned()]));
        assert_eq!(read[3], Entry::Changed(vec!["/c".to_owned()]));
        assert!(matches!(&read[4], Entry::Todos { items, merge: false } if items.len() == 1));
        assert!(entries(&assistant("<synthetic>", false)).is_empty());
        assert!(entries(&assistant("claude", true)).is_empty());
    }
}
