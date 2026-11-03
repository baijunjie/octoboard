//! Grok Build's record store (`AgentProvider::records`): a session's directory at
//! `sessions/<url-encoded working directory>/<id>/`, marked by its `usage.json`, holding the
//! conversation in `chat_history.jsonl`, the streamed updates in `updates.jsonl`, the context
//! window in `signals.json` and the title and configured model in `summary.json`
//! ([`crate::providers::grok::LAYOUT`]).
//!
//! The session directory is the record; the index names its marker inside it. Grok restores a
//! session directory missing locally from its remote registry, so a missing one proves nothing.
//! Its hooks report no path that names the record. How its conversation reads is
//! [`super::conversation`], and whether a switch carries its last turn on is
//! [`super::switch_continue`]. A turn cancelled before its first token, which its hooks never
//! report, is watched for in the terminal title alone (`crate::session::title_interrupt`).

use std::fs::FileType;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;

use crate::hooks::RecordWatch;
use crate::providers::grok::{GROK_CONVERSATION_FILE, GROK_SESSION_MARKER, LAYOUT};
use crate::records::context::{number, Context};
use crate::records::conversation::Entry;
use crate::records::observed::{text, Observed};
use crate::records::store::newest_existing;
use crate::records::RecordStore;
use crate::state::AppState;

/// The file inside a session's directory that streams its updates, the context size among them.
pub(super) const UPDATES_LOG: &str = "updates.jsonl";

/// The file inside a session's directory that gives its context window.
const SIGNALS_FILE: &str = "signals.json";

/// Grok Build's record store.
pub(crate) struct GrokRecords;

impl RecordStore for GrokRecords {
    /// A session directory on the last level below a working-directory folder, indexed by its
    /// marker.
    fn indexed(
        &self,
        path: &Path,
        name: &str,
        kind: FileType,
        leaf: bool,
    ) -> Option<(String, PathBuf)> {
        (kind.is_dir() && leaf).then(|| (name.to_owned(), path.join(GROK_SESSION_MARKER)))
    }

    /// `<dir>/<working directory>/<id>/usage.json`, which stands for the session's directory: the
    /// index names the marker inside it, one level below the directory that is the record.
    fn own_paths(&self, agent_id: &str, record: &Path, levels_below_dir: usize) -> Vec<PathBuf> {
        let name = record.file_name().and_then(|name| name.to_str());
        match record.parent() {
            Some(parent)
                if levels_below_dir == LAYOUT.record_level + 1
                    && name == Some(GROK_SESSION_MARKER)
                    && parent.file_name().and_then(|name| name.to_str()) == Some(agent_id) =>
            {
                vec![parent.to_path_buf()]
            }
            _ => Vec::new(),
        }
    }

    /// The chat history inside the session's directory.
    fn conversation_file(&self, record: PathBuf) -> PathBuf {
        record.join(GROK_CONVERSATION_FILE)
    }

    fn names_record(&self, lowercase_name: &str, kind: FileType, lowercase_id: &str) -> bool {
        kind.is_dir() && lowercase_name == lowercase_id
    }

    fn recognises_records(&self) -> bool {
        true
    }

    /// The session's directory, which holds everything Grok keeps of it.
    fn carried_head(&self, root: &Path, records: &[PathBuf]) -> Option<PathBuf> {
        newest_existing(root, records, std::fs::Metadata::is_dir)
    }

    fn should_continue(&self, record: &Path, usage_limit_only: bool) -> std::io::Result<bool> {
        super::switch_continue::should_continue(record, usage_limit_only)
    }

    /// The marker indexes the directory; the figure is in the updates log beside it.
    fn followed_file(&self, record: &Path) -> PathBuf {
        record.with_file_name(UPDATES_LOG)
    }

    /// The updates log never gives the model; the summary does.
    fn observes_record_lines(&self) -> bool {
        false
    }

    /// Every streamed update carries the session's running context size; a compaction reports the
    /// size it left, after which a trailing line may still repeat the size it began from.
    fn fold_line(&self, line: &str, _past_boundary: bool, context: &mut Context, _: &mut Observed) {
        let compacted = line.contains("auto_compact_completed");
        if !compacted && !line.contains("\"totalTokens\"") {
            return;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return;
        };
        let Some(params) = value.get("params") else {
            return;
        };
        let update = params.get("update");
        if compacted
            && update.and_then(|update| update.get("sessionUpdate")?.as_str())
                == Some("auto_compact_completed")
        {
            if let Some(after) = update.and_then(|update| number(update, "tokens_after")) {
                context.compacted_to(
                    after,
                    update.and_then(|update| number(update, "tokens_before")),
                );
            }
        } else if let Some(total) = params
            .get("_meta")
            .and_then(|meta| number(meta, "totalTokens"))
        {
            context.observe_total(total);
        }
    }

    /// The summary names the configured model and effort rather than the id a request was served
    /// by.
    fn observe_summary(&self, summary: &Value, observed: &mut Observed) {
        observed.fold(
            text(summary, "current_model_id"),
            text(summary, "reasoning_effort"),
        );
    }

    fn window_file(&self) -> Option<&'static str> {
        Some(SIGNALS_FILE)
    }

    fn window_in(&self, file: &Value) -> Option<u64> {
        file.get("contextWindowTokens").and_then(Value::as_u64)
    }

    /// Grok's `contextWindowUsage` rounds down.
    fn used_percent(&self, tokens: u64, window: u64) -> Option<u8> {
        let percent = (u128::from(tokens) * 100 / u128::from(window.max(1))).min(100) as u64;
        u8::try_from(percent).ok()
    }

    fn entries(&self, line: &Value) -> Vec<Entry> {
        super::conversation::entries(line)
    }

    /// A `<pasted_content>` wrapper such as Claude Code records is taken off, should a prompt carry
    /// one. Only a guard: none has been observed in Grok Build's records, and its hook reports a
    /// long paste without one ("Writing into a running session" in `docs/agent-cli-reference.md`),
    /// but a wrapper would not be the user's words.
    fn user_text<'a>(&self, text: &'a str) -> Option<std::borrow::Cow<'a, str>> {
        Some(std::borrow::Cow::Borrowed(
            crate::text::without_paste_wrapper(text),
        ))
    }

    /// A turn cancelled before it fired anything is watched for in the terminal title alone.
    fn arm_watch(&self, state: &Arc<AppState>, session_id: &str, watch: RecordWatch<'_>) {
        if let RecordWatch::Interrupt { .. } = watch {
            let generation = state.begin_record_watch(session_id);
            crate::session::title_interrupt::watch(state, session_id, generation);
        }
    }
}
