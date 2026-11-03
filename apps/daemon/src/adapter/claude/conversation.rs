//! How a line of Claude Code's transcript reads as the common conversation
//! (`crate::records::conversation`, `RecordStore::entries`).

use std::borrow::Cow;

use serde_json::Value;

use crate::records::conversation::{joined_text, text_of, todos, Entry};

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

/// The entries of one line of Claude Code's transcript: one JSON object a line, `type` `user` or
/// `assistant`, the message under `message`. A user line carrying a tool result is not something
/// the user said, and a subagent's lines (`isSidechain`) are another conversation.
pub(super) fn entries(line: &Value) -> Vec<Entry> {
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

#[cfg(test)]
mod tests {
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
}
