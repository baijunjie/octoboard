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

/// Whether one line of the rollout is a prompt the user sent (`RecordStore::is_user_prompt`),
/// told from the kind of line it is: the event of a user's message (`user_message` in older Codex,
/// an `item_completed` of a `UserMessage` item in newer), whatever it holds, an image alone
/// included. The user-role response items beside it hold what Codex adds itself as well as what
/// was typed, and are no prompt on their own. A line that does not parse, or an event with no
/// payload, cannot be placed.
pub(super) fn is_user_prompt(line: &str) -> Option<bool> {
    let line: Value = serde_json::from_str(line).ok()?;
    if line.get("type").and_then(Value::as_str) != Some("event_msg") {
        return Some(false);
    }
    let payload = line.get("payload")?;
    Some(match payload.get("type").and_then(Value::as_str) {
        Some("user_message") => true,
        Some("item_completed") => {
            payload
                .get("item")
                .and_then(|item| item.get("type"))
                .and_then(Value::as_str)
                == Some("UserMessage")
        }
        _ => false,
    })
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
            let paths = change_paths(item);
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
            let paths = patch_paths(input);
            if paths.is_empty() {
                Vec::new()
            } else {
                vec![Entry::Changed(paths)]
            }
        }
        _ => Vec::new(),
    }
}

/// The files a `FileChange` item names, each with the path it moved to.
pub(super) fn change_paths(item: &Value) -> Vec<String> {
    item.get("changes")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|changes| {
            changes.iter().flat_map(|(path, change)| {
                [Some(path.clone()), text_of(change, "move_path")]
                    .into_iter()
                    .flatten()
            })
        })
        .collect()
}

/// The files the headers of a patch (`apply_patch`'s input) name.
pub(super) fn patch_paths(patch: &str) -> Vec<String> {
    patch
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
        .collect()
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

#[cfg(test)]
mod tests {
    /// The event of a user's message is a prompt whatever it holds; the user-role response items
    /// beside it, and every other line, are not. Replayed over the committed rollouts, then the
    /// shapes they do not hold.
    #[test]
    fn the_event_of_a_users_message_is_a_prompt_whatever_it_holds() {
        let prompts = |capture: &str| -> Vec<usize> {
            capture
                .lines()
                .enumerate()
                .filter(|(_, line)| super::is_user_prompt(line) == Some(true))
                .map(|(at, _)| at)
                .collect()
        };
        let titles = include_str!("../../../testdata/titles_codex_0.163.0_rollout.jsonl");
        assert_eq!(prompts(titles), [3]);
        let handoff = include_str!("../../../testdata/handoff_codex_0.162.1_rollout.jsonl");
        assert_eq!(prompts(handoff), [2, 11, 14]);
        for capture in [titles, handoff] {
            assert!(capture
                .lines()
                .all(|line| super::is_user_prompt(line).is_some()));
        }

        // Images alone, in either generation's event.
        let image_item = r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"UserMessage","content":[{"type":"image","image_url":"data:image/png;base64,AAAA"}]}}}"#;
        assert_eq!(super::is_user_prompt(image_item), Some(true));
        let image_message = r#"{"type":"event_msg","payload":{"type":"user_message","message":"","images":["data:image/png;base64,AAAA"],"local_images":[]}}"#;
        assert_eq!(super::is_user_prompt(image_message), Some(true));
        // A pasted block of markup.
        let markup = r#"{"type":"event_msg","payload":{"type":"user_message","message":"<error>\nboom\n</error>"}}"#;
        assert_eq!(super::is_user_prompt(markup), Some(true));

        assert_eq!(super::is_user_prompt(r#"{"type":"event_msg"}"#), None);
        assert_eq!(super::is_user_prompt(r#"{"type":"event_msg","payl"#), None);
    }

    use serde_json::json;

    use super::*;

    #[test]
    fn a_patch_names_the_files_of_its_headers() {
        let patch =
            "*** Begin Patch\n*** Update File: /w/a.rs \n@@\n-x\n+y\n*** Add File: /w/b.rs\n\
                     *** Delete File: /w/c.rs\n*** Move to: /w/d.rs\n*** End Patch";
        assert_eq!(
            patch_paths(patch),
            ["/w/a.rs", "/w/b.rs", "/w/c.rs", "/w/d.rs"]
        );
        assert!(patch_paths("nothing to name").is_empty());
    }

    #[test]
    fn a_file_change_names_each_file_and_where_it_moved_to() {
        let item = json!({"changes": {
            "/w/a.rs": {"kind": "update"},
            "/w/old.rs": {"kind": "update", "move_path": "/w/new.rs"}}});
        let mut paths = change_paths(&item);
        paths.sort();
        assert_eq!(paths, ["/w/a.rs", "/w/new.rs", "/w/old.rs"]);
        assert!(change_paths(&json!({})).is_empty());
        assert!(change_paths(&json!({"changes": []})).is_empty());
    }

    /// A change that did not go through is no changed file; a plan is a todo list; a patch call
    /// names the files it changes.
    #[test]
    fn the_common_entries_of_items_and_calls() {
        let item = |status: Option<&str>| {
            let mut item = json!({"type": "FileChange", "changes": {"/w/a.rs": {}}});
            if let Some(status) = status {
                item["status"] = status.into();
            }
            json!({"type": "event_msg", "payload": {"type": "item_completed", "item": item}})
        };
        assert_eq!(
            entries(&item(Some("completed"))),
            [Entry::Changed(vec!["/w/a.rs".to_owned()])]
        );
        assert_eq!(entries(&item(None)).len(), 1);
        assert!(entries(&item(Some("failed"))).is_empty());
        assert!(entries(&item(Some("declined"))).is_empty());

        let plan = json!({"type": "response_item", "payload": {
            "type": "function_call", "name": "update_plan",
            "arguments": "{\"plan\":[{\"step\":\"a\",\"status\":\"pending\"}]}"}});
        let read = entries(&plan);
        let [Entry::Todos { items, merge }] = read.as_slice() else {
            panic!("a todo list");
        };
        assert!(!merge);
        assert_eq!(
            (items[0].text.as_str(), items[0].status.as_str()),
            ("a", "pending")
        );

        let patch = json!({"type": "response_item", "payload": {
            "type": "custom_tool_call", "name": "apply_patch",
            "input": "*** Update File: /w/a.rs"}});
        assert_eq!(
            entries(&patch),
            [Entry::Changed(vec!["/w/a.rs".to_owned()])]
        );
    }

    #[test]
    fn a_commentary_note_is_told_by_the_phase_on_the_message_or_its_item() {
        assert!(commentary(&json!({"payload": {"phase": "commentary"}})));
        assert!(commentary(
            &json!({"payload": {"item": {"phase": "commentary"}}})
        ));
        assert!(!commentary(&json!({"payload": {"phase": "final_answer"}})));
        assert!(!commentary(&json!({"payload": {}})));
        assert!(!commentary(&json!({})));
    }
}
