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

/// Whether one line of the transcript is a prompt the user sent (`RecordStore::is_user_prompt`),
/// told from the kind of line it is and never from its text, so that a prompt holding only an
/// image, or a pasted block that starts with `<`, counts.
///
/// A prompt is a `user` line of the main conversation that Claude Code did not write itself (no
/// `isMeta`, no `isCompactSummary`, no `origin` other than `human`) and that carries no tool
/// result. Of the lines Claude Code writes in tags, a slash command is a prompt when it was sent
/// as one (the line carries a `promptId`) and a local command such as `/clear` is not; a local
/// command's output, a background task's notification and the note an interrupted turn leaves are
/// not. Shell input and its output (`<bash-input>`, `<bash-stdout>`) are no prompt in a line
/// without a `promptId`; with one, Claude Code 2.1.330 goes on to ask the model about them and
/// earlier versions did not, so such a line cannot be placed. Neither can a line that does not
/// parse, or a `user` line with no content to judge.
///
/// A message typed while the model was working is a prompt too. Claude Code 2.1.295 to 2.1.302
/// records it only as a `queued_command` attachment, which says who queued it in `origin.kind`:
/// `human` is the user, anything else (a background task's notification) is not, and one with no
/// origin cannot be placed. Where the turn it was typed into began from a line that is no prompt
/// (another session's message, a notification), that attachment is the user's only line.
pub(super) fn is_user_prompt(line: &str) -> Option<bool> {
    let line: Value = serde_json::from_str(line).ok()?;
    if flagged(&line, "isSidechain") {
        return Some(false);
    }
    if line.get("type").and_then(Value::as_str) == Some("attachment") {
        let queued = line
            .get("attachment")
            .filter(|attachment| text_of(attachment, "type").as_deref() == Some("queued_command"));
        return match queued {
            None => Some(false),
            Some(queued) => {
                let origin = queued.get("origin")?.get("kind")?.as_str()?;
                Some(origin == "human")
            }
        };
    }
    if line.get("type").and_then(Value::as_str) != Some("user")
        || flagged(&line, "isMeta")
        || flagged(&line, "isCompactSummary")
        || !from_user(&line)
    {
        return Some(false);
    }
    let kind = |block: &Value| block.get("type").and_then(Value::as_str).map(str::to_owned);
    let (text, attached) = match line.get("message")?.get("content")? {
        Value::String(text) => (text.clone(), false),
        Value::Array(blocks) => {
            if blocks
                .iter()
                .any(|block| kind(block).as_deref() == Some("tool_result"))
            {
                return Some(false);
            }
            let text: Vec<&str> = blocks
                .iter()
                .filter(|block| kind(block).as_deref() == Some("text"))
                .filter_map(|block| block.get("text")?.as_str())
                .collect();
            let attached = blocks
                .iter()
                .any(|block| kind(block).is_some_and(|kind| kind != "text"));
            (text.join("\n"), attached)
        }
        _ => return None,
    };
    let text = text.trim();
    let sent_as_prompt = line.get("promptId").is_some();
    if text.starts_with("<local-command-")
        || text.starts_with("<task-notification>")
        || NOT_TYPED.iter().any(|lead| text.starts_with(lead))
    {
        return Some(false);
    }
    if text.starts_with("<command-name>") || text.starts_with("<command-message>") {
        return Some(sent_as_prompt);
    }
    if text.starts_with("<bash-input>")
        || text.starts_with("<bash-stdout>")
        || text.starts_with("<bash-stderr>")
    {
        return if sent_as_prompt { None } else { Some(false) };
    }
    if text.is_empty() && !attached {
        return None;
    }
    Some(true)
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
    /// The lines of `capture` that read as a prompt the user sent, and those that cannot be placed.
    fn placed(capture: &str) -> (Vec<usize>, Vec<usize>) {
        let of = |answer: Option<bool>| -> Vec<usize> {
            capture
                .lines()
                .enumerate()
                .filter(|(_, line)| super::is_user_prompt(line) == answer)
                .map(|(at, _)| at)
                .collect()
        };
        (of(Some(true)), of(None))
    }

    /// Replayed over the committed transcripts: of the user lines, only the typed prompt and the
    /// slash command sent as one are prompts; a local command, its caveat and output, a tool
    /// result, a background task's notification with or without an `origin`, another session's
    /// message, a compaction summary and a subagent's lines are not.
    #[test]
    fn a_prompt_is_told_from_the_lines_claude_code_writes_itself() {
        let command = include_str!("../../../testdata/titles_claude_2.1.308_command.jsonl");
        assert_eq!(placed(command), (vec![2], vec![]));
        // Beside the typed prompt, the two messages typed during a turn, which that version
        // records only as `queued_command` attachments; the notification queued the same way
        // (line 9) is not the user's.
        let handoff = include_str!("../../../testdata/handoff_claude_2.1.302.jsonl");
        assert_eq!(placed(handoff), (vec![1, 8, 14], vec![]));
        let queued = |origin: &str| {
            format!(
                r#"{{"type":"attachment","attachment":{{"type":"queued_command","prompt":"Also run the linter"{origin}}}}}"#
            )
        };
        let answered = |origin: &str| super::is_user_prompt(&queued(origin));
        assert_eq!(answered(r#","origin":{"kind":"human"}"#), Some(true));
        assert_eq!(
            answered(r#","origin":{"kind":"task-notification"}"#),
            Some(false)
        );
        assert_eq!(answered(""), None, "nothing says who queued it");
        let other = r#"{"type":"attachment","attachment":{"type":"file","filename":"a.rs"}}"#;
        assert_eq!(super::is_user_prompt(other), Some(false));
        let subagent = r#"{"type":"attachment","isSidechain":true,"attachment":{"type":"queued_command","prompt":"x","origin":{"kind":"human"}}}"#;
        assert_eq!(super::is_user_prompt(subagent), Some(false));
    }

    /// What Claude Code 2.1.330 wrote with no prompt sent: a `/clear` in a session never prompted
    /// is no prompt on any line, and shell input and its output, which that version goes on to
    /// ask the model about, cannot be placed.
    #[test]
    fn what_is_written_with_no_prompt_sent_is_no_prompt() {
        let clear = include_str!("../../../testdata/unprompted_claude_2.1.330_clear.jsonl");
        assert_eq!(placed(clear), (vec![], vec![]));
        let shell = include_str!("../../../testdata/unprompted_claude_2.1.330_shell.jsonl");
        assert_eq!(placed(shell), (vec![], vec![5, 6]));
        // Shell input in a line with no `promptId` was run and never sent.
        let unsent =
            r#"{"type":"user","message":{"role":"user","content":"<bash-input>ls</bash-input>"}}"#;
        assert_eq!(super::is_user_prompt(unsent), Some(false));
    }

    /// A prompt counts whatever its text: a pasted block of markup, an image alone, an image
    /// beside text. A line with nothing to judge, or one that does not parse, cannot be placed.
    #[test]
    fn a_prompt_counts_whatever_it_holds() {
        let line = |content: &str, prompt_id: bool| {
            let id = if prompt_id { r#""promptId":"p","# } else { "" };
            format!(r#"{{"type":"user",{id}"message":{{"role":"user","content":{content}}}}}"#)
        };
        let image =
            r#"{"type":"image","source":{"type":"base64","media_type":"image/png","data":"AAAA"}}"#;
        for content in [
            r#""<error>\nboom\n</error>""#.to_owned(),
            r#""<html><body>pasted</body></html>""#.to_owned(),
            format!("[{image}]"),
            format!(r#"[{image},{{"type":"text","text":"What is this?"}}]"#),
            r#""Fix the test""#.to_owned(),
        ] {
            for prompt_id in [true, false] {
                assert_eq!(
                    super::is_user_prompt(&line(&content, prompt_id)),
                    Some(true),
                    "{content}"
                );
            }
        }
        assert_eq!(super::is_user_prompt(&line("\"\"", true)), None);
        assert_eq!(super::is_user_prompt(&line("[]", true)), None);
        assert_eq!(super::is_user_prompt(&line("7", true)), None);
        assert_eq!(super::is_user_prompt(r#"{"type":"user","mess"#), None);
        assert_eq!(super::is_user_prompt(r#"{"type":"user"}"#), None);
    }

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
