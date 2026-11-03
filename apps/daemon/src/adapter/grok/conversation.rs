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
