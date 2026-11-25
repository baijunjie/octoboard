//! GitHub Copilot CLI's session directory, its conversation, and its operation lock.

use std::borrow::Cow;
use std::fs::FileType;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};

use crate::hooks::RecordWatch;
use crate::providers::copilot::{COPILOT_EVENTS_FILE, LAYOUT};
use crate::records::context::{number, Context};
use crate::records::conversation::{text_of, Entry};
use crate::records::feed::FeedReader;
use crate::records::observed::Observed;
use crate::records::{plausible_id, RecordStore};
use crate::state::AppState;

const OPERATION_LOCKS: &str = ".session-operation-locks";

pub(super) struct CopilotRecords;

impl RecordStore for CopilotRecords {
    fn indexed(
        &self,
        path: &Path,
        name: &str,
        kind: FileType,
        leaf: bool,
    ) -> Option<(String, PathBuf)> {
        (leaf && kind.is_dir() && plausible_id(name)).then(|| (name.to_owned(), path.to_owned()))
    }

    fn own_paths(&self, agent_id: &str, record: &Path, levels_below_dir: usize) -> Vec<PathBuf> {
        if levels_below_dir != LAYOUT.record_level
            || !plausible_id(agent_id)
            || record.file_name().and_then(|name| name.to_str()) != Some(agent_id)
        {
            return Vec::new();
        }
        let Some(parent) = record.parent() else {
            return Vec::new();
        };
        let mut paths = vec![record.to_owned()];
        let locks = parent.join(OPERATION_LOCKS);
        if std::fs::symlink_metadata(&locks).is_ok_and(|meta| meta.is_dir()) {
            paths.push(locks.join(format!("{agent_id}.lock")));
        }
        paths
    }

    fn conversation_file(&self, record: PathBuf) -> PathBuf {
        record.join(COPILOT_EVENTS_FILE)
    }

    fn followed_file(&self, record: &Path) -> PathBuf {
        self.conversation_file(record.to_owned())
    }

    fn names_record(&self, lowercase_name: &str, kind: FileType, lowercase_id: &str) -> bool {
        kind.is_dir() && lowercase_name == lowercase_id
    }

    fn recognises_records(&self) -> bool {
        true
    }

    // The database row alone cannot be resumed once the directory is gone.
    fn absence_is_conclusive(&self) -> bool {
        true
    }

    fn carried_head(&self, root: &Path, records: &[PathBuf]) -> Option<PathBuf> {
        crate::records::store::newest_existing(root, records, std::fs::Metadata::is_dir)
    }

    fn summary_file(&self, record: &Path) -> Option<PathBuf> {
        Some(record.join("workspace.yaml"))
    }

    fn summary_in(&self, bytes: &[u8]) -> Option<Value> {
        let name = std::str::from_utf8(bytes)
            .ok()?
            .lines()
            .find_map(|line| line.strip_prefix("name:"))?
            .trim();
        let name = match (name.as_bytes().first(), name.as_bytes().last()) {
            (Some(b'\''), Some(b'\'')) => {
                name.get(1..name.len().checked_sub(1)?)?.replace("''", "'")
            }
            (Some(b'"'), Some(b'"')) => serde_json::from_str(name).ok()?,
            (Some(b'|' | b'>'), _) => return None,
            _ if matches!(name, "null" | "~") => return None,
            _ => name.to_owned(),
        };
        (!name.is_empty()).then(|| json!({ "name": name }))
    }

    fn fold_line(
        &self,
        line: &str,
        past_boundary: bool,
        context: &mut Context,
        observed: &mut Observed,
    ) {
        if ![
            "session.usage_record",
            "session.compaction_complete",
            "session.start",
            "session.model_change",
        ]
        .iter()
        .any(|kind| line.contains(kind))
        {
            return;
        }
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let Some((kind, data)) = main_event(&event) else {
            return;
        };
        match kind {
            "session.usage_record" => {
                let Some(usage) = data.get("usage") else {
                    return;
                };
                // inputTokens includes cache reads even when cacheReadTokens is given separately.
                if usage["interactionType"] == "conversation-agent" {
                    if let Some(tokens) = number(usage, "inputTokens") {
                        context.observe_with_window((tokens, number(usage, "maxPromptTokens")));
                    }
                }
            }
            "session.compaction_complete" if data["success"] == true => context.compacted(),
            "session.start" | "session.model_change" if past_boundary => {
                let (key, effort) = if kind == "session.start" {
                    ("selectedModel", "reasoningEffort")
                } else {
                    ("newModel", "reasoningEffort")
                };
                let model = text_of(data, key);
                let effort = text_of(data, effort);
                if let (Some(model), None, Some(Value::Null)) =
                    (model.as_ref(), &effort, data.get("reasoningEffort"))
                {
                    observed.unset_effort(model.clone());
                } else {
                    observed.fold(model, effort);
                }
            }
            _ => {}
        }
    }

    fn used_percent(&self, tokens: u64, window: u64) -> Option<u8> {
        (window > 0).then(|| (100.0 * tokens as f64 / window as f64).round().min(100.0) as u8)
    }

    fn entries(&self, line: &Value) -> Vec<Entry> {
        let Some((kind, data)) = main_event(line) else {
            return Vec::new();
        };
        match kind {
            "user.message" => text_of(data, "content")
                .filter(|text| self.user_text(text).is_some())
                .map(Entry::User)
                .into_iter()
                .collect(),
            "assistant.message" => text_of(data, "content")
                .map(Entry::Assistant)
                .into_iter()
                .collect(),
            "session.compaction_complete" if data["success"] == true => {
                text_of(data, "summaryContent")
                    .map(Entry::Summary)
                    .into_iter()
                    .collect()
            }
            "tool.execution_start" if is_edit(data) => {
                let paths = edit_paths(&data["arguments"]);
                if paths.is_empty() {
                    Vec::new()
                } else {
                    vec![Entry::Changed(paths)]
                }
            }
            _ => Vec::new(),
        }
    }

    fn user_text<'a>(&self, text: &'a str) -> Option<Cow<'a, str>> {
        (!text.trim_start().starts_with("<system_notification>")).then_some(Cow::Borrowed(text))
    }

    fn is_user_prompt(&self, line: &str) -> Option<bool> {
        let line: Value = serde_json::from_str(line).ok()?;
        let kind = line.get("type")?.as_str()?;
        if kind != "user.message" {
            return Some(false);
        }
        line.get("data")?.as_object()?;
        Some(main_event(&line).is_some_and(|(_, data)| {
            data.get("content")
                .and_then(Value::as_str)
                .is_none_or(|text| self.user_text(text).is_some())
        }))
    }

    fn feed_reader(&self) -> Option<Box<dyn FeedReader>> {
        Some(Box::new(super::feed::CopilotFeed))
    }

    fn arm_watch(&self, state: &Arc<AppState>, session_id: &str, watch: RecordWatch<'_>) {
        super::record_watch::arm(state, session_id, watch);
    }
}

pub(super) fn main_event(line: &Value) -> Option<(&str, &Value)> {
    if line.get("agentId").and_then(Value::as_str).is_some() {
        return None;
    }
    Some((line.get("type")?.as_str()?, line.get("data")?))
}

pub(super) fn is_edit(data: &Value) -> bool {
    matches!(
        data["toolName"].as_str(),
        Some("create" | "edit" | "apply_patch")
    )
}

pub(super) fn edit_paths(args: &Value) -> Vec<String> {
    match args {
        Value::String(patch) => crate::adapter::patch::patch_targets(patch),
        _ => text_of(args, "path").into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::provider;
    use crate::protocol::{AgentId, ConversationBody, ToolStatus};
    use crate::records::conversation::{read_stretches, Speaker};
    use crate::records::feed::{reader_for, FeedOut};
    use crate::records::{carried, Locator, Stretch};
    use crate::test_support::ScratchDir;

    const REWOUND: &str = include_str!("../../../testdata/copilot_rewound_events_1.0.114.jsonl");
    const TITLE: &str = include_str!("../../../testdata/copilot_workspace_title_1.0.114.yaml");
    const ID: &str = "63b517d8-4911-4e0c-95bb-35eccd3a4a30";

    #[test]
    fn a_rewound_record_keeps_its_older_turn_context_and_workspace_title() {
        assert_eq!(
            REWOUND
                .lines()
                .filter(|line| CopilotRecords.is_user_prompt(line) == Some(true))
                .count(),
            1
        );
        let home = ScratchDir::new("copilot-records");
        let record = home.join(LAYOUT.records_dir).join(ID);
        std::fs::create_dir_all(&record).unwrap();
        let events = record.join(COPILOT_EVENTS_FILE);
        std::fs::write(&events, REWOUND).unwrap();
        let agent = AgentId::COPILOT;
        let captured = read_stretches(
            agent,
            &[Stretch {
                path: events,
                cut: None,
            }],
        )
        .unwrap();
        assert_eq!(
            captured.messages,
            [
                (Speaker::User, "First milestone probe".into()),
                (Speaker::Assistant, "Probe response.".into()),
            ]
        );
        let store = provider(agent).records();
        let mut context = Context::default();
        let mut observed = Observed::default();
        for line in REWOUND.lines() {
            store.fold_line(line, true, &mut context, &mut observed);
        }
        let context = context.context(store).flatten().unwrap();
        assert_eq!((context.tokens, context.window), (320, Some(128000)));
        let summary = store.summary_in(TITLE.as_bytes()).unwrap();
        assert_eq!(
            provider(agent).summary_title(&summary).as_deref(),
            Some("Milestone: reader's title")
        );
    }

    #[test]
    fn a_switch_carries_the_directory_and_removal_also_owns_the_operation_lock() {
        let home = ScratchDir::new("copilot-records-carry");
        let records = home.join(LAYOUT.records_dir);
        let record = records.join(ID);
        let lock = records.join(OPERATION_LOCKS).join(format!("{ID}.lock"));
        std::fs::create_dir_all(lock.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&record).unwrap();
        std::fs::write(record.join(COPILOT_EVENTS_FILE), REWOUND).unwrap();
        std::fs::write(&lock, "").unwrap();
        std::fs::write(home.join("session-store.db"), "").unwrap();
        let head =
            carried::locate_record(&mut Locator::default(), AgentId::COPILOT, &home, ID).unwrap();
        assert_eq!(home.join(&head), record);
        assert!(carried::companions(AgentId::COPILOT, &home, &head, ID).is_empty());
        assert_eq!(CopilotRecords.own_paths(ID, &record, 1), [record, lock]);
    }

    #[test]
    fn the_feed_reads_a_tool_start_and_updates_its_entry_on_completion() {
        let recorded = include_str!("../../../testdata/copilot_background_events.jsonl");
        let mut reader = reader_for(AgentId::COPILOT);
        let mut out = FeedOut::default();
        for line in recorded.lines() {
            let event: Value = serde_json::from_str(line).unwrap();
            reader.line(&event, &mut out);
        }
        assert!(out.entries().iter().any(|entry| matches!(&entry.body,
            ConversationBody::ToolCall(call)
                if call.name == "bash" && call.status == ToolStatus::Completed)));
    }
}
