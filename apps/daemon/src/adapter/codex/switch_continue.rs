//! Whether a Codex rollout shows a turn an account switch is to carry on
//! (`crate::records::switch_continue`, `RecordStore::should_continue`).

use std::path::Path;

use serde_json::Value;

use crate::records::line_file;

/// The Codex decision on the rollout at `record` (compressed or not): the last turn's ending.
/// Codex records an Esc or a declined approval as `turn_aborted`, every turn that ends otherwise as
/// `task_complete` (with an `error` when it failed), and writes nothing when its process is ended
/// mid-turn. A turn with no ending is continued only when it holds the user's message: a manual
/// `/compact` ended mid-way leaves a turn with none.
pub(super) fn should_continue(record: &Path, usage_limit_only: bool) -> std::io::Result<bool> {
    // What the last turn has shown so far: `None` before any turn.
    let mut last: Option<CodexTurn> = None;
    line_file::read_from(record, 0, |line| {
        // Most of a rollout is model traffic; only its events bear on this.
        if !line.contains(r#""type":"event_msg""#) {
            return;
        }
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            return;
        };
        if entry["type"] != "event_msg" {
            return;
        }
        let payload = &entry["payload"];
        match payload["type"].as_str() {
            Some("task_started") => last = Some(CodexTurn::Running { prompted: false }),
            Some("item_completed") if payload["item"]["type"] == "UserMessage" => {
                if let Some(CodexTurn::Running { prompted }) = &mut last {
                    *prompted = true;
                }
            }
            Some("turn_aborted") => last = Some(CodexTurn::Ended { carry_on: false }),
            Some("task_complete") => {
                // The `error` is a string for most failures but an object for some, so only the
                // one string is matched.
                let carry_on = payload["error"]["codex_error_info"] == "usage_limit_exceeded";
                last = Some(CodexTurn::Ended { carry_on });
            }
            _ => {}
        }
    })?;
    Ok(match last {
        Some(CodexTurn::Running { prompted }) => prompted && !usage_limit_only,
        Some(CodexTurn::Ended { carry_on }) => carry_on,
        None => false,
    })
}

enum CodexTurn {
    Running { prompted: bool },
    Ended { carry_on: bool },
}
