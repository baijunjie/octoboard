//! Codex's record store (`AgentProvider::records`): a thread's rollouts at
//! `sessions/<year>/<month>/<day>/rollout-<start>-<thread id>.jsonl` (`.jsonl.zst` once compressed;
//! a later history segment adds `_<segment id>`), moved under `archived_sessions/` by
//! `codex archive` ([`crate::providers::codex::LAYOUT`]).
//!
//! A thread that had a turn reverted is a chain of rollouts, whose head holds the conversation as
//! it stands ([`super::rollout_files`]); a head that is a later segment holds only the turns since
//! the revert, so its opening prompt and the context figure it picks up at are read from the files
//! it continues. The rollout gives the context size with the window, and the model and effort each
//! turn started with; the effort a turn leaves out is the catalogue's default for the model. How
//! its lines read as a conversation is [`super::conversation`], and whether a switch carries its
//! last turn on is [`super::switch_continue`]. A turn that ends in an error, which its hooks never
//! report, is watched for in it by [`super::rollout_watch`].

use std::collections::HashMap;
use std::fs::FileType;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;

use crate::hooks::RecordWatch;
use crate::hostfs::lexically_normalise;
use crate::protocol::{AgentId, Session};
use crate::providers::codex::{
    CODEX_ARCHIVED_DIR, CODEX_MODELS_CACHE, CODEX_RECORDS_DIR, CODEX_ROLLOUT_PREFIX, LAYOUT,
};
use crate::records::context::{number, Context, Figure};
use crate::records::conversation::Entry;
use crate::records::locate::{records_dir, walk_records, Step};
use crate::records::observed::{text, Observed};
use crate::records::removal::Resolved;
use crate::records::resumability::{scan_for_record, Located};
use crate::records::store::transcript_path_hint;
use crate::records::{plausible_id, Branch, Locator, RecordStore};
use crate::state::AppState;

use super::rollout_files::{
    is_rollout_of, is_segment, names_thread, own_id, path_stem, rollout_branch, rollout_head,
    rollout_stem, rollout_thread_id,
};

/// What Codex sets aside of every window before it counts anything as used or left.
const CODEX_BASELINE: u64 = 12_000;

/// Codex's record store.
pub(crate) struct CodexRecords;

impl RecordStore for CodexRecords {
    /// The `session_id` and `transcript_path` its hooks carry.
    fn hinted_record(&self, payload: &Value) -> Option<(String, PathBuf)> {
        transcript_path_hint(payload)
    }

    /// A rollout at any depth, under the thread id its name carries.
    fn indexed(
        &self,
        path: &Path,
        name: &str,
        kind: FileType,
        _leaf: bool,
    ) -> Option<(String, PathBuf)> {
        if !kind.is_file() || !name.starts_with(CODEX_ROLLOUT_PREFIX) {
            return None;
        }
        let id = rollout_stem(name).and_then(rollout_thread_id)?;
        Some((id.to_owned(), path.to_path_buf()))
    }

    /// The head of the thread's chain of rollouts.
    fn current_of(&self, paths: &[PathBuf]) -> Option<PathBuf> {
        rollout_head(paths)
    }

    /// `<dir>/<year>/<month>/<day>/rollout-<time>-<id>.jsonl` (or `.jsonl.zst`; a later segment
    /// adds `_<segment id>`).
    fn own_paths(&self, agent_id: &str, record: &Path, levels_below_dir: usize) -> Vec<PathBuf> {
        let name = record.file_name().and_then(|name| name.to_str());
        let named = name.is_some_and(|name| {
            rollout_stem(name).is_some_and(|stem| {
                stem.starts_with(CODEX_ROLLOUT_PREFIX) && names_thread(stem, agent_id)
            })
        });
        if levels_below_dir == LAYOUT.record_level && named {
            vec![record.to_path_buf()]
        } else {
            Vec::new()
        }
    }

    /// The head of the chain, read with the whole current branch of a thread that had a turn
    /// reverted (see [`rollout_branch`]).
    fn branch_among(&self, files: Vec<PathBuf>) -> Option<Branch> {
        let record = rollout_head(&files)?;
        let stretches = rollout_branch(&record, &files);
        Some(Branch { record, stretches })
    }

    /// The rollouts `codex archive` left in each account's `archived_sessions/`.
    fn archived_branch(&self, roots: &[PathBuf], agent_id: &str) -> Option<Branch> {
        let agent_id = agent_id.to_ascii_lowercase();
        let mut files: Vec<PathBuf> = Vec::new();
        for root in roots {
            walk_records(
                &root.join(CODEX_ARCHIVED_DIR),
                LAYOUT.search_depth,
                &mut |path, name, kind, _| {
                    if is_rollout_of(kind, &name.to_ascii_lowercase(), &agent_id) {
                        files.push(path.to_path_buf());
                        Step::Matched
                    } else {
                        Step::Descend
                    }
                },
            );
        }
        self.branch_among(files)
    }

    /// Any file of the thread's chain, its first rollout or a later history segment, so a thread is
    /// never judged gone while a part of it is there.
    fn names_record(&self, lowercase_name: &str, kind: FileType, lowercase_id: &str) -> bool {
        is_rollout_of(kind, lowercase_name, lowercase_id)
    }

    fn recognises_records(&self) -> bool {
        true
    }

    /// A reverted turn starts a new history segment, which becomes the thread's head.
    fn head_moves_during_turn(&self) -> bool {
        true
    }

    /// `codex unarchive`, since a resume refuses an archived thread.
    fn unarchive(
        &self,
        shell_env: &HashMap<String, String>,
        config_dir: Option<&Path>,
        agent_id: &str,
    ) -> anyhow::Result<()> {
        super::unarchive::unarchive(shell_env, config_dir, agent_id)
    }

    /// Codex keeps a thread only in its rollouts, so a thread with none of them is gone.
    fn absence_is_conclusive(&self) -> bool {
        true
    }

    /// Each linked records directory is searched where it leads, by the same rules as any records
    /// directory (exact names, no link followed below it).
    fn archived_behind_links(&self, locator: &Locator, session: &Session) -> bool {
        let Some(agent_id) = session
            .agent_session_id
            .as_deref()
            .filter(|id| session.has_conversation && plausible_id(id))
            .map(str::to_ascii_lowercase)
        else {
            return false;
        };
        let Some(root) = locator.root(AgentId::CODEX, session.config_dir.as_deref()) else {
            return false;
        };
        let root = lexically_normalise(&root);
        let dirs = [root.join(CODEX_RECORDS_DIR), root.join(CODEX_ARCHIVED_DIR)];
        let is_link = |dir: &Path| {
            std::fs::symlink_metadata(dir).is_ok_and(|meta| meta.file_type().is_symlink())
        };
        if !dirs.iter().any(|dir| is_link(dir)) {
            return false;
        }
        let is_record =
            |name: &str, kind: FileType| is_rollout_of(kind, &name.to_ascii_lowercase(), &agent_id);
        let found = |dir: &Path| {
            let dir = if is_link(dir) {
                match crate::hostfs::canonical(dir) {
                    Ok(resolved) => resolved,
                    Err(_) => return Located::Uncertain,
                }
            } else {
                dir.to_path_buf()
            };
            scan_for_record(&dir, LAYOUT.search_depth, &is_record)
        };
        found(&dirs[0]) == Located::Missing && found(&dirs[1]) == Located::Present
    }

    /// The rollouts `codex archive` moved into `archived_sessions/`.
    fn beside_records(&self, root: &Path, agent_id: &str) -> Vec<PathBuf> {
        let Some(archived) = Resolved::of(root.join(CODEX_ARCHIVED_DIR)) else {
            return Vec::new();
        };
        let id = agent_id.to_ascii_lowercase();
        let mut found = Vec::new();
        walk_records(
            archived.real(),
            LAYOUT.search_depth,
            &mut |path, name, kind, _| {
                if is_rollout_of(kind, &name.to_ascii_lowercase(), &id) {
                    found.push(path.to_path_buf());
                    Step::Matched
                } else {
                    Step::Descend
                }
            },
        );
        found
            .iter()
            .filter_map(|path| archived.spell(path))
            .collect()
    }

    /// The head of the thread's chain of rollouts.
    fn carried_head(&self, root: &Path, records: &[PathBuf]) -> Option<PathBuf> {
        let absolute: Vec<PathBuf> = records.iter().map(|relative| root.join(relative)).collect();
        rollout_head(&absolute).and_then(|head| head.strip_prefix(root).ok().map(Path::to_path_buf))
    }

    /// The other rollouts of the thread's chain, which with the head are the whole conversation: a
    /// target given the head alone is refused, and one given the first rollout alone resumes a
    /// stale branch. A subagent's rollout is named by its own thread id and stays behind.
    fn carried_companions(&self, files: Vec<PathBuf>, record: &Path) -> Vec<PathBuf> {
        files
            .into_iter()
            .filter(|relative| relative.starts_with(CODEX_RECORDS_DIR) && relative != record)
            .collect()
    }

    fn should_continue(&self, record: &Path, usage_limit_only: bool) -> std::io::Result<bool> {
        super::switch_continue::should_continue(record, usage_limit_only)
    }

    /// A `token_count` record gives the last request's total and the window limit; a turn context
    /// names the model and effort the turn started with.
    fn fold_line(
        &self,
        line: &str,
        past_boundary: bool,
        context: &mut Context,
        observed: &mut Observed,
    ) {
        if let Some(figure) = figure(line) {
            context.observe_with_window(figure);
        }
        if past_boundary {
            fold_observed(observed, line);
        }
    }

    /// Codex's `/status` shows what is left of the window past the baseline, rounded; the share
    /// used is what that leaves of the whole. A window no larger than the baseline has nothing
    /// left.
    fn used_percent(&self, tokens: u64, window: u64) -> Option<u8> {
        let percent = match window.saturating_sub(CODEX_BASELINE) {
            0 => 100,
            usable => {
                let used = tokens.saturating_sub(CODEX_BASELINE).min(usable);
                let left = ((usable - used) as f64 / usable as f64 * 100.0).round() as u64;
                100 - left
            }
        };
        u8::try_from(percent).ok()
    }

    /// A later history segment of its thread.
    fn continues_earlier_file(&self, record: &Path) -> bool {
        is_segment(record)
    }

    /// The thread's first rollout. The account's records root may be shared through a link; links
    /// below it are still skipped. This read-only lookup does not populate the account index used
    /// for destructive record discovery.
    fn opening_file(&self, locator: &mut Locator, root: &Path, agent_id: &str) -> Option<PathBuf> {
        let records = crate::hostfs::canonical(records_dir(AgentId::CODEX, root)).ok()?;
        locator
            .index_at(AgentId::CODEX, &records)
            .records
            .get(agent_id)?
            .iter()
            .find(|path| path_stem(path).and_then(own_id) == Some(agent_id))
            .cloned()
    }

    /// The last figure in the file the segment continues, before the cut, or failing that in the
    /// nearest earlier file of the chain that has one.
    fn earlier_figure(
        &self,
        locator: &mut Locator,
        root: &Path,
        agent_id: &str,
        head: &Path,
    ) -> Option<Option<Figure>> {
        let stretches = rollout_branch(
            head,
            locator.index(AgentId::CODEX, root).records.get(agent_id)?,
        )?;
        for stretch in stretches.iter().rev().skip(1) {
            let mut last = None;
            let read = |line: &str| last = figure(line).or(last);
            let _ = crate::records::line_file::read_to(
                &stretch.path,
                stretch.cut.unwrap_or(u64::MAX),
                read,
            );
            if last.is_some() {
                return Some(last);
            }
        }
        Some(None)
    }

    /// The model catalogue Codex caches per account.
    fn effort_defaults_file(&self) -> Option<&'static str> {
        Some(CODEX_MODELS_CACHE)
    }

    /// Each entry's `default_reasoning_level` by its `slug`, hidden entries included, since a
    /// session can run a model the pickers do not list.
    fn effort_defaults(&self, catalogue: &Value) -> HashMap<String, String> {
        let Some(entries) = catalogue.get("models").and_then(Value::as_array) else {
            return HashMap::new();
        };
        entries
            .iter()
            .filter_map(|entry| {
                let slug = entry.get("slug").and_then(Value::as_str)?;
                let level = entry
                    .get("default_reasoning_level")
                    .and_then(Value::as_str)?;
                (!slug.is_empty() && !level.is_empty()).then(|| (slug.to_owned(), level.to_owned()))
            })
            .collect()
    }

    fn entries(&self, line: &Value) -> Vec<Entry> {
        super::conversation::entries(line)
    }

    /// A progress note the model wrote on the way, which is not its answer.
    fn hidden_from_history(&self, line: &Value) -> bool {
        super::conversation::commentary(line)
    }

    /// A turn that ends in an error is watched for in the rollout.
    fn arm_watch(&self, state: &Arc<AppState>, session_id: &str, watch: RecordWatch<'_>) {
        if let RecordWatch::Failure { record, turn_id } = watch {
            super::rollout_watch::watch_for_failure(state, session_id, record, turn_id);
        }
    }
}

/// The size of the context the conversation's last request carried and the window limit, from a
/// `token_count` record. One whose `info` is null reports nothing yet.
pub(crate) fn figure(line: &str) -> Option<Figure> {
    if !line.contains("\"token_count\"") {
        return None;
    }
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("type")?.as_str()? != "event_msg" {
        return None;
    }
    let payload = value.get("payload")?;
    if payload.get("type")?.as_str()? != "token_count" {
        return None;
    }
    let info = payload.get("info").filter(|info| !info.is_null())?;
    Some((
        number(info.get("last_token_usage")?, "total_tokens")?,
        number(info, "model_context_window"),
    ))
}

/// Takes a line of a Codex rollout into `observed`, which names the model and effort a turn
/// started with. A turn context carries the effort its whole configuration resolved to and leaves
/// it out when no layer sets one, so the latest one is the whole truth about the effort: one that
/// names a model and no effort unsets an effort an earlier turn named.
fn fold_observed(observed: &mut Observed, line: &str) {
    if !line.contains("\"turn_context\"") {
        return;
    }
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return;
    };
    if value.get("type").and_then(Value::as_str) != Some("turn_context") {
        return;
    }
    if let Some(payload) = value.get("payload") {
        match (text(payload, "model"), text(payload, "effort")) {
            (Some(model), None) => observed.unset_effort(model),
            (model, effort) => observed.fold(model, effort),
        }
    }
}
