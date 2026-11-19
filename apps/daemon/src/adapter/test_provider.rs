//! The behavior of the test-only provider (`crate::providers::test_provider`): the launch plan,
//! which every provider must have, and a record store that finds `<agent session id>.jsonl` files
//! under the records directory and declares a live feed reader and companion, so the generic feed
//! code is shown to follow what a provider declares; every other sub-interface is the trait's
//! default.

use std::fs::FileType;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{json, Value};

use super::{AgentProvider, LaunchPlan, LaunchSpec};
use crate::protocol::{ToolCategory, ToolStatus};
use crate::providers::{AgentId, ProviderDescriptor};
use crate::records::feed::{FeedOut, FeedReader, Pending, ToolCall, ToolOutcome};
use crate::records::RecordStore;

pub struct TestProvider;

impl AgentProvider for TestProvider {
    fn descriptor(&self) -> &'static ProviderDescriptor {
        AgentId::TEST.descriptor()
    }

    fn records(&self) -> &'static dyn RecordStore {
        &TestRecords
    }

    fn plan(&self, _spec: &LaunchSpec<'_>) -> Result<LaunchPlan> {
        Ok(LaunchPlan {
            args: Vec::new(),
            private_server: None,
            env: Vec::new(),
            resolves_approvals_itself: false,
            notice: None,
            carried_trust: None,
            after_exit: None,
            adoption: None,
        })
    }
}

/// The test provider's records: a file `<agent session id>.jsonl` in the records directory, whose
/// lines are `{"user": text}`, `{"assistant": text}`, `{"call": id, "name": name}`,
/// `{"result": id, "output": text}`, `{"truncate": len}` and `{"panic": true}`, which panics;
/// beside it, `<id>.live` lines `{"pending": text}` (or none, which clears what was pending).
pub struct TestRecords;

impl RecordStore for TestRecords {
    fn indexed(
        &self,
        path: &Path,
        name: &str,
        kind: FileType,
        leaf: bool,
    ) -> Option<(String, PathBuf)> {
        let id = name.strip_suffix(".jsonl")?;
        (leaf && kind.is_file()).then(|| (id.to_owned(), path.to_path_buf()))
    }

    fn own_paths(&self, agent_id: &str, record: &Path, levels_below_dir: usize) -> Vec<PathBuf> {
        let named =
            record.file_name().and_then(|name| name.to_str()) == Some(&format!("{agent_id}.jsonl"));
        if levels_below_dir == 1 && named {
            vec![record.to_path_buf()]
        } else {
            Vec::new()
        }
    }

    fn feed_reader(&self) -> Option<Box<dyn FeedReader>> {
        Some(Box::new(TestReader))
    }

    fn feed_companion(&self, record: &Path) -> Option<PathBuf> {
        Some(record.with_extension("live"))
    }
}

struct TestReader;

impl FeedReader for TestReader {
    fn line(&mut self, line: &Value, out: &mut FeedOut) {
        if line["panic"].as_bool() == Some(true) {
            panic!("the test reader was told to panic");
        }
        if let Some(text) = line["user"].as_str() {
            out.user(text, None);
        } else if let Some(text) = line["assistant"].as_str() {
            out.assistant(text, None);
        } else if let Some(id) = line["call"].as_str() {
            out.tool_call(
                ToolCall {
                    call_id: id.to_owned(),
                    name: line["name"].as_str().unwrap_or("tool").to_owned(),
                    category: ToolCategory::Other,
                    title: None,
                    input: json!({}),
                    paths: Vec::new(),
                    status: ToolStatus::Running,
                    output: None,
                    exit_code: None,
                },
                None,
            );
        } else if let Some(id) = line["result"].as_str() {
            out.tool_result(
                id,
                ToolOutcome {
                    status: ToolStatus::Completed,
                    output: line["output"].as_str().map(str::to_owned),
                    exit_code: None,
                },
            );
        } else if let Some(len) = line["truncate"].as_u64() {
            out.truncate(len as usize);
        }
    }

    fn companion_line(&mut self, line: &Value, out: &mut FeedOut) {
        out.set_pending(
            line["pending"]
                .as_str()
                .map(|text| Pending::Assistant(text.to_owned()))
                .into_iter()
                .collect(),
        );
    }
}
