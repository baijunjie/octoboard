//! Claude Code's record store (`AgentProvider::records`): a session's transcript at
//! `projects/<working directory slug>/<id>.jsonl`, with the directory `<id>` beside it holding its
//! subagents' transcripts and tool results, and its rewind checkpoints in `file-history/<id>`
//! ([`crate::providers::claude::LAYOUT`]).
//!
//! The transcript gives the context size and the model and effort of every reply, but no window,
//! which the status line reports instead (`crate::adapter::StatusLine`); how its lines read as a
//! conversation is [`super::conversation`], and whether a switch carries its last turn on is
//! [`super::switch_continue`]. What its hooks never report, a declined prompt and an interrupted
//! turn, is watched for in it by [`super::transcript_watch`].

use std::fs::FileType;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;

use crate::hooks::RecordWatch;
use crate::providers::claude::{
    CLAUDE_CHECKPOINTS_DIR, CLAUDE_RECORDS_DIR, CLAUDE_TRANSCRIPT_SUFFIX, LAYOUT,
};
use crate::records::context::{number, Context};
use crate::records::conversation::Entry;
use crate::records::observed::{text, Observed};
use crate::records::removal::Resolved;
use crate::records::store::transcript_path_hint;
use crate::records::{plausible_id, RecordStore};
use crate::state::AppState;

use super::transcript_watch;

/// Claude Code's record store.
pub(crate) struct ClaudeRecords;

impl RecordStore for ClaudeRecords {
    /// The `session_id` and `transcript_path` its hooks carry.
    fn hinted_record(&self, payload: &Value) -> Option<(String, PathBuf)> {
        transcript_path_hint(payload)
    }

    /// The transcript, on the last level below a working-directory folder. A subagent directory may
    /// survive after the main transcript was removed, and is indexed under the transcript's path.
    fn indexed(
        &self,
        path: &Path,
        name: &str,
        kind: FileType,
        leaf: bool,
    ) -> Option<(String, PathBuf)> {
        if !leaf {
            return None;
        }
        if kind.is_file() {
            let id = name.strip_suffix(CLAUDE_TRANSCRIPT_SUFFIX)?;
            return Some((id.to_owned(), path.to_path_buf()));
        }
        (kind.is_dir() && plausible_id(name)).then(|| {
            let transcript = path.with_file_name(format!("{name}{CLAUDE_TRANSCRIPT_SUFFIX}"));
            (name.to_owned(), transcript)
        })
    }

    /// `<dir>/<project>/<id>.jsonl`, which stands for itself and the directory of its own name
    /// beside it.
    fn own_paths(&self, agent_id: &str, record: &Path, levels_below_dir: usize) -> Vec<PathBuf> {
        let name = record.file_name().and_then(|name| name.to_str());
        match record.parent() {
            Some(parent)
                if levels_below_dir == LAYOUT.record_level
                    && name == Some(&format!("{agent_id}{CLAUDE_TRANSCRIPT_SUFFIX}")) =>
            {
                vec![record.to_path_buf(), parent.join(agent_id)]
            }
            _ => Vec::new(),
        }
    }

    /// The main transcript; a subagent directory alone is not a record.
    fn names_record(&self, lowercase_name: &str, kind: FileType, lowercase_id: &str) -> bool {
        kind.is_file() && lowercase_name == format!("{lowercase_id}{CLAUDE_TRANSCRIPT_SUFFIX}")
    }

    fn recognises_records(&self) -> bool {
        true
    }

    /// Claude Code keeps a conversation only in its transcript, so a missing one is gone.
    fn absence_is_conclusive(&self) -> bool {
        true
    }

    /// The rewind checkpoints, `file-history/<id>`.
    fn beside_records(&self, root: &Path, agent_id: &str) -> Vec<PathBuf> {
        let Some(history) = Resolved::of(root.join(CLAUDE_CHECKPOINTS_DIR)) else {
            return Vec::new();
        };
        let checkpoints = history.real().join(agent_id);
        if std::fs::symlink_metadata(&checkpoints).is_err() {
            return Vec::new();
        }
        history.spell(&checkpoints).into_iter().collect()
    }

    /// The directory of the transcript's own name beside it (subagent transcripts, tool results,
    /// the custom title) and everything outside the records directory (the rewind checkpoints).
    fn carried_companions(&self, files: Vec<PathBuf>, record: &Path) -> Vec<PathBuf> {
        let sidecar = record.with_extension("");
        files
            .into_iter()
            .filter(|relative| *relative == sidecar || !relative.starts_with(CLAUDE_RECORDS_DIR))
            .collect()
    }

    fn should_continue(&self, record: &Path, usage_limit_only: bool) -> std::io::Result<bool> {
        super::switch_continue::should_continue(record, usage_limit_only)
    }

    /// Only a line that carries usage or a compaction says anything here, so the rest are not
    /// parsed.
    fn fold_line(
        &self,
        line: &str,
        past_boundary: bool,
        context: &mut Context,
        observed: &mut Observed,
    ) {
        if !line.contains("\"usage\"") && !line.contains("compact_boundary") {
            return;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return;
        };
        fold_context(context, &value);
        if past_boundary {
            fold_observed(observed, &value);
        }
    }

    /// Claude Code's status line rounds to the nearest percent.
    fn used_percent(&self, tokens: u64, window: u64) -> Option<u8> {
        let percent = ((tokens as f64 / window.max(1) as f64 * 100.0).round() as u64).min(100);
        u8::try_from(percent).ok()
    }

    fn entries(&self, line: &Value) -> Vec<Entry> {
        super::conversation::entries(line)
    }

    /// What the user typed: a paste without its wrapper, a slash command as typed, nothing for the
    /// note an interrupted turn leaves (`super::conversation::user_text`).
    fn user_text<'a>(&self, text: &'a str) -> Option<std::borrow::Cow<'a, str>> {
        super::conversation::user_text(text)
    }

    /// A declined prompt is watched for in the transcript; an interrupted turn in the transcript
    /// and the terminal title together.
    fn arm_watch(&self, state: &Arc<AppState>, session_id: &str, watch: RecordWatch<'_>) {
        match watch {
            RecordWatch::Decline { record } => {
                transcript_watch::watch_for_rejection(state, session_id, record)
            }
            RecordWatch::Interrupt { record, turn_id } => {
                if let Some(generation) =
                    transcript_watch::watch_for_interrupt(state, session_id, record, turn_id)
                {
                    crate::session::title_interrupt::watch(state, session_id, generation);
                }
            }
            RecordWatch::Failure { .. } => {}
        }
    }
}

/// Takes a line of Claude's main transcript into `context`. A subagent's lines are marked as a
/// sidechain, `<synthetic>` is the client's own notice, and replies that were not metered, failed
/// or carried nothing in say nothing. A request's size is what it sent in: ordinary, cache-read and
/// cache-write input, without the reply (Claude's own status line counts it so); a message that
/// carries `iterations` is sized by the last of its own calls. A compaction leaves the size unknown
/// until the next request.
pub(crate) fn fold_context(context: &mut Context, line: &Value) {
    let flag = |key: &str| line.get(key).and_then(Value::as_bool) == Some(true);
    match line.get("type").and_then(Value::as_str) {
        Some("system")
            if line.get("subtype").and_then(Value::as_str) == Some("compact_boundary") =>
        {
            context.compacted();
        }
        Some("assistant") => {
            let Some(message) = line.get("message") else {
                return;
            };
            if flag("isSidechain")
                || flag("isUnmetered")
                || flag("isApiErrorMessage")
                || message.get("model").and_then(Value::as_str) == Some("<synthetic>")
            {
                return;
            }
            let Some(usage) = message.get("usage") else {
                return;
            };
            let usage = usage
                .get("iterations")
                .and_then(Value::as_array)
                .and_then(|calls| {
                    calls.iter().rev().find(|call| {
                        matches!(
                            call.get("type").and_then(Value::as_str),
                            Some("message" | "fallback_message")
                        )
                    })
                })
                .unwrap_or(usage);
            let tokens = [
                "input_tokens",
                "cache_creation_input_tokens",
                "cache_read_input_tokens",
            ]
            .iter()
            .fold(0u64, |sum, key| {
                sum.saturating_add(number(usage, key).unwrap_or(0))
            });
            if tokens > 0 {
                context.observe(tokens);
            }
        }
        _ => {}
    }
}

/// Takes an assistant turn of Claude's main transcript into `observed`. A subagent's turns are
/// marked as a sidechain and run other models, and a `<synthetic>` message is the client's own
/// notice rather than a model's reply.
fn fold_observed(observed: &mut Observed, line: &Value) {
    if line.get("type").and_then(Value::as_str) != Some("assistant")
        || line.get("isSidechain").and_then(Value::as_bool) == Some(true)
    {
        return;
    }
    let Some(model) = line.pointer("/message/model").and_then(Value::as_str) else {
        return;
    };
    if model != "<synthetic>" {
        observed.fold(Some(model.to_owned()), text(line, "effort"));
    }
}
