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
