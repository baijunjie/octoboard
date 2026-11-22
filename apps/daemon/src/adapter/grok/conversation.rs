//! How a line of a Grok Build session's `chat_history.jsonl` reads as the common conversation
//! (`crate::records::conversation`, `RecordStore::entries`).

use serde_json::Value;

use crate::records::conversation::{joined_text, text_of, todos, Entry};

/// How the summary Grok Build writes at a compaction begins.
const COMPACTION_SUMMARY: &str = "This session is being continued from a previous conversation";

/// The entries of one line of Grok Build's `chat_history.jsonl`: `user` and `assistant` lines, the
/// latter carrying the tool calls the model made. A user line with a `synthetic_reason` is
/// something Grok put in the conversation itself (project instructions, system reminders, a
/// compaction note), and a real prompt is sometimes wrapped in `<user_query>`.
pub(super) fn entries(line: &Value) -> Vec<Entry> {
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

/// What Grok writes into a `user` line beside the user's own words: the environment it reports
/// and the reminders it adds.
const OWN_BLOCKS: [&str; 2] = ["<user_info>", "<system-reminder>"];

/// Whether one line of `chat_history.jsonl` is a prompt the user sent
/// (`RecordStore::is_user_prompt`), told from the kind of line it is: a `user` line with no
/// `synthetic_reason` (Grok marks with one what it puts in the conversation itself) that holds a
/// text block other than the ones Grok adds ([`OWN_BLOCKS`]). A `user` line holding only those is
/// Grok's own. One that holds neither (only the `<image_files>` list Grok writes for attached
/// images, a block that is not text, a `<user_query>` with nothing in it, content in another
/// shape) cannot be placed, and neither can a line that does not parse.
pub(super) fn is_user_prompt(line: &str) -> Option<bool> {
    let line: Value = serde_json::from_str(line).ok()?;
    if line.get("type").and_then(Value::as_str) != Some("user")
        || line.get("synthetic_reason").is_some()
    {
        return Some(false);
    }
    let blocks = line.get("content")?.as_array()?;
    let mut own_only = !blocks.is_empty();
    for block in blocks {
        let text = block
            .get("text")
            .and_then(Value::as_str)
            .filter(|_| block.get("type").and_then(Value::as_str) == Some("text"))
            .map(str::trim);
        match text {
            Some(text) if OWN_BLOCKS.iter().any(|tag| text.starts_with(tag)) => {}
            Some(text) if text.starts_with("<image_files>") || text.is_empty() => own_only = false,
            Some(text) => {
                let asked = text
                    .strip_prefix("<user_query>")
                    .and_then(|inner| inner.strip_suffix("</user_query>"))
                    .unwrap_or(text);
                if !asked.trim().is_empty() {
                    return Some(true);
                }
                own_only = false;
            }
            None => own_only = false,
        }
    }
    own_only.then_some(false)
}

fn grok_call(call: &Value) -> Option<Entry> {
    let arguments: Value = serde_json::from_str(call.get("arguments")?.as_str()?).ok()?;
    call_entry(call.get("name")?.as_str()?, &arguments)
}

/// What the call of tool `name` with `arguments` says about the conversation: a todo list, or the
/// file it changed.
pub(super) fn call_entry(name: &str, arguments: &Value) -> Option<Entry> {
    match name {
        "todo_write" => Some(Entry::Todos {
            items: todos(arguments.get("todos")?, Some("id"), "content").collect(),
            merge: arguments
                .get("merge")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        "search_replace" | "write" => Some(Entry::Changed(vec![text_of(arguments, "file_path")?])),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_prompt_is_the_users_text_without_what_grok_added() {
        let line = json!({"type": "user", "content": [
            {"type": "text", "text": "<user_info>os</user_info>"},
            {"type": "text", "text": " <user_query>\n fix it \n</user_query> "},
            {"type": "text", "text": "and more"}]});
        assert_eq!(
            entries(&line),
            [Entry::User("fix it\n\nand more".to_owned())]
        );
        let synthetic = json!({"type": "user", "synthetic_reason": "project_rules",
            "content": [{"type": "text", "text": "rules"}]});
        assert!(entries(&synthetic).is_empty());
    }

    /// Replayed over the committed chat histories: only the lines holding the user's own words are
    /// prompts; what Grok marks as its own, and a line holding only the environment it reports or
    /// a reminder, is not; the list of attached images on a line of its own cannot be placed.
    #[test]
    fn a_prompt_is_told_from_the_lines_grok_writes_itself() {
        let placed = |capture: &str, answer: Option<bool>| -> Vec<usize> {
            capture
                .lines()
                .enumerate()
                .filter(|(_, line)| is_user_prompt(line) == answer)
                .map(|(at, _)| at)
                .collect()
        };
        let handoff = include_str!("../../../testdata/handoff_grok_1.0.62_chat_history.jsonl");
        assert_eq!(placed(handoff, Some(true)), [4, 13]);
        assert_eq!(placed(handoff, None), [3], "the `<image_files>` line");
        let feed = include_str!("../../../testdata/feed_grok_1.0.89_chat_history.jsonl");
        let prompts = placed(feed, Some(true));
        assert_eq!(prompts.len(), 4);
        for at in prompts {
            let line = feed.lines().nth(at).unwrap();
            assert!(
                line.contains("<user_query>") && !line.contains("synthetic_reason"),
                "{line}"
            );
        }
        assert!(placed(feed, None).is_empty());
    }

    /// What Grok Build 1.0.92 wrote on launch, before any prompt: its system prompt and a reminder
    /// it marks as its own.
    #[test]
    fn what_is_written_before_any_prompt_is_no_prompt() {
        let unprompted =
            include_str!("../../../testdata/unprompted_grok_1.0.92_chat_history.jsonl");
        assert_eq!(unprompted.lines().count(), 2);
        assert!(unprompted
            .lines()
            .all(|line| is_user_prompt(line) == Some(false)));
    }

    /// A prompt counts whatever its text, beside the image list too; a `user` line with nothing of
    /// the user's that can be told, and a line that does not parse, cannot be placed.
    #[test]
    fn a_prompt_counts_whatever_it_holds() {
        let line = |blocks: &str| format!(r#"{{"type":"user","content":[{blocks}]}}"#);
        let text = |text: &str| format!(r#"{{"type":"text","text":"{text}"}}"#);
        for blocks in [
            text("fix it"),
            text("<error>boom</error>"),
            format!(
                "{},{}",
                text("<image_files>a.png</image_files>"),
                text("look")
            ),
            format!(
                "{},{}",
                text("<user_info>os</user_info>"),
                text("<user_query>go</user_query>")
            ),
        ] {
            assert_eq!(is_user_prompt(&line(&blocks)), Some(true), "{blocks}");
        }
        assert_eq!(
            is_user_prompt(&line(&text("<user_info>os</user_info>"))),
            Some(false)
        );
        for blocks in [
            text("<image_files>a.png</image_files>"),
            text("<user_query></user_query>"),
            r#"{"type":"image","data":"AAAA"}"#.to_owned(),
            String::new(),
        ] {
            assert_eq!(is_user_prompt(&line(&blocks)), None, "{blocks}");
        }
        assert_eq!(
            is_user_prompt(r#"{"type":"user","content":"fix it"}"#),
            None
        );
        assert_eq!(is_user_prompt(r#"{"type":"user","content":[{"ty"#), None);
        assert_eq!(
            is_user_prompt(r#"{"type":"assistant","content":"The \"user\" asked."}"#),
            Some(false)
        );
    }

    /// Only the compaction line that holds the summary is one; the one beside it that restates the
    /// rules is not.
    #[test]
    fn a_compaction_summary_is_told_from_the_rules_it_restates() {
        let line = |text: &str| {
            json!({"type": "user", "synthetic_reason": "compaction_meta",
                   "content": [{"type": "text", "text": text}]})
        };
        let summary = format!("{COMPACTION_SUMMARY}: it did x");
        assert_eq!(entries(&line(&summary)), [Entry::Summary(summary)]);
        assert!(entries(&line("Rules and environment")).is_empty());
    }

    #[test]
    fn a_todo_call_and_a_file_change_are_what_a_call_says_of_the_conversation() {
        let merged = call_entry(
            "todo_write",
            &json!({"merge": true, "todos": [{"id": "1", "content": "a", "status": "pending"}]}),
        );
        let Some(Entry::Todos { items, merge }) = merged else {
            panic!("todos");
        };
        assert!(merge);
        assert_eq!(items[0].id.as_deref(), Some("1"));
        let replaced = call_entry("todo_write", &json!({"todos": []}));
        assert!(matches!(replaced, Some(Entry::Todos { merge: false, .. })));
        assert_eq!(
            call_entry("write", &json!({"file_path": "a.rs"})),
            Some(Entry::Changed(vec!["a.rs".to_owned()]))
        );
        assert_eq!(call_entry("read_file", &json!({"target_file": "a"})), None);
        assert_eq!(call_entry("todo_write", &json!({})), None);
    }

    #[test]
    fn an_assistant_line_is_its_text_and_the_calls_that_say_something() {
        let line = json!({"type": "assistant", "content": "Working.", "tool_calls": [
            {"id": "t", "name": "write", "arguments": "{\"file_path\": \"b.rs\"}"},
            {"id": "u", "name": "write", "arguments": "{"}]});
        assert_eq!(
            entries(&line),
            [
                Entry::Assistant("Working.".to_owned()),
                Entry::Changed(vec!["b.rs".to_owned()])
            ]
        );
    }
}
