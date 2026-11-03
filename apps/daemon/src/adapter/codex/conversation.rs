//! How a line of a Codex rollout reads as the common conversation
//! (`crate::records::conversation`, `RecordStore::entries`), and which of its lines a session's
//! history leaves out.

use serde_json::Value;

use crate::records::conversation::{joined_text, text_of, todos, Entry};

/// The entries of one line of Codex's rollout. What was said comes from the `event_msg` lines,
/// which hold what the user typed and what the model answered (`user_message` and `agent_message`
/// in older Codex, `item_completed` of a `UserMessage` or `AgentMessage` item in newer); the
/// `response_item` messages beside them include the instructions and environment context Codex adds
/// itself. Files changed come from the `FileChange` item of newer Codex, or an `apply_patch` call
/// in the response items of older.
pub(super) fn entries(line: &Value) -> Vec<Entry> {
    let Some(payload) = line.get("payload") else {
        return Vec::new();
    };
    if let Some(typed) = super::typed_text(line) {
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

/// Whether a line is a progress note the model wrote on the way (`commentary`), as opposed to its
/// answer (`final_answer`; older Codex wrote no phase at all). The phase sits on the message
/// itself, or on the item an `item_completed` event carries.
pub(super) fn commentary(line: &Value) -> bool {
    let Some(payload) = line.get("payload") else {
        return false;
    };
    let phase = payload
        .get("phase")
        .or_else(|| payload.get("item")?.get("phase"));
    phase.and_then(Value::as_str) == Some("commentary")
}
