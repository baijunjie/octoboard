//! Persisting what an agent's own records give each session, for live and archived cards: the
//! context size (`context.rs`), the titles (`titles.rs`) and the model and effort
//! (`observed.rs`). One collector reads append-only records incrementally; archived
//! sources are read once, and get a final pass after their process exits. No conversation content
//! is retained or published here. Missing data stays unknown. A session that counts as having no
//! conversation is marked as having one once its own record shows a prompt the user sent
//! (`user_prompt.rs`): the agent's hook reports that prompt, and the report does not always
//! arrive. Where the records live is `records/mod.rs`'s business; this module reads them and hands
//! each line to the three modules above, and asks `default_effort.rs` for the effort a session's
//! agent runs it at where the records give the model alone.
//!
//! A conversation kept as rows (`records::rows`) has no file to read as it grows: its rows are
//! read whole through the provider and folded line by line like a file's, while the session has a
//! process, when an event of the session says they may have changed (`Hints::stirred`), and once
//! more after the process ends. One with no process is not read at a daemon's start: what its
//! rows gave was stored while it ran. Reads run independently so a command that is still
//! running does not delay the rest of a collector pass.
//!
//! A change of a session's model restarts it on the same conversation, so its records go on
//! naming the old model up to where the old process stopped. The change stores an observation
//! boundary for the session ([`ObservedBoundary`], taken by [`observed_boundary`] once the old
//! process is down) and clears the observed model and effort in the same write; from then on the
//! collector takes neither from what the records held at the boundary, only from what is written
//! after it, and the store refuses a model or effort read against an older boundary.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::adapter::provider;
use crate::protocol::{AgentId, Session};
use crate::providers::layout::layout;
use crate::records::branch::Leaf;
use crate::records::context::Context;
use crate::records::default_effort::DefaultEfforts;
use crate::records::line_file::{Change, LineFile, Signature, JSON_FILE_LIMIT};
use crate::records::observed::Observed;
use crate::records::titles::{SharedTitles, Titles};
use crate::records::user_prompt;
use crate::records::Locator;
use crate::state::AppState;
use crate::store::RecordUpdate;

mod row_reads;
use row_reads::{RowReads, Version};

#[derive(Default)]
struct Cursor {
    identity: Option<(AgentId, Option<String>, Option<String>)>,
    path: Option<PathBuf>,
    file: LineFile,
    /// Where the lines read stand in a record that is a tree (`RecordStore::branching`).
    leaf: Leaf,
    context: Context,
    without_process: bool,
    titles: Titles,
    /// The opening prompt of the file the conversation opened in, when the record read continues
    /// an earlier file (`RecordStore::continues_earlier_file`), with the path it was read from.
    first_prompt: Option<(PathBuf, Option<String>)>,
    observed: Observed,
    /// The session's observation boundary as the store holds it (`Store::observed_boundaries`),
    /// which tells a boundary a later model change stored from the one already applied, and is
    /// handed back with what is read against it (`RecordUpdate::observed_boundary`).
    stored_boundary: Option<String>,
    /// [`Self::stored_boundary`] read: what of the records the process before the last model
    /// change wrote, which gives no model or effort.
    observed_boundary: Option<ObservedBoundary>,
    /// Where in the record file read the lines written after the boundary start, with that
    /// file's path: worked out by [`ObservedBoundary::start`] once per file read from its start,
    /// since that reads the whole of what the file held at the boundary; not kept while it cannot
    /// be told.
    boundary_start: Option<(PathBuf, u64)>,
    /// The conversation's summary file ([`summary_path`]), which names the
    /// conversation and may name its configured model.
    summary: JsonFile,
    /// The file beside the record that gives the context window (`RecordStore::window_file`).
    window: JsonFile,
    rows_read: Option<Version>,
    /// A process that ends before its first row read completes still needs a final read.
    rows_seen_live: bool,
    /// For a conversation kept as rows whose observation boundary could not count its lines
    /// (`ObservedBoundary::rows`): how many it held at the first reading against that boundary,
    /// which stand for the ones the old process wrote.
    rows_held: Option<usize>,
    /// The conversation file, followed for a prompt the user sent while the session counts as
    /// having no conversation.
    user_prompt: user_prompt::Search,
}

/// What a session's records held when a change of its model ended its process (see the module
/// header): the model and effort shown up to there are the old process's, so none is taken from
/// them again, while what the relaunched process writes after them is. Stored as JSON with the
/// session, and kept across cursor resets and daemon restarts.
///
/// A file is known by its name, without the `.zst` Codex gives a rollout it compresses, and by
/// what it held, never by its directory or inode: an account switch copies a conversation's
/// records into another account's directory, Codex writes a compressed rollout out as a new plain
/// file when it resumes the thread, and a device number can change across a reboot or a remount,
/// and in each case the content held at the boundary is still the old process's. A file whose
/// beginning no longer matches is read whole, as new.
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct ObservedBoundary {
    files: HashMap<String, Vec<Held>>,
    /// For a conversation kept as rows (`records::rows`): how many lines it held, the lines
    /// after them being the relaunched process's. `None` when they could not be read: the
    /// collector then counts them at its first reading of the conversation against this boundary
    /// ([`Cursor::rows_held`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rows: Option<usize>,
    /// Set, to a value of its own, where the rows could not be counted: without it every such
    /// boundary is the same text, so a second model change whose rows could not be read either
    /// would store what is already stored, the collector would not take it up as a new boundary
    /// and would go on counting the rows held at the first, letting the model the process before
    /// wrote after them stand for the new process's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    uncounted: Option<uuid::Uuid>,
}

/// The first `len` bytes of a file (decompressed, for a compressed one), by length and digest,
/// so that the store keeps no conversation content.
#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct Held {
    len: u64,
    digest: u64,
}

impl Held {
    /// What the file at `path` holds in full now.
    fn whole(path: &Path) -> std::io::Result<Self> {
        let mut digest = Digest::default();
        let len = std::io::copy(&mut crate::records::line_file::open(path, 0)?, &mut digest)?;
        Ok(Self {
            len,
            digest: digest.0,
        })
    }

    /// Whether the file at `path` still begins with exactly what this holds. A file shorter than
    /// that, or one that cannot be read through that far (an I/O error, a compressed file with an
    /// incomplete frame), proves nothing either way.
    fn begins(&self, path: &Path) -> Prefix {
        let compressed = path.extension().is_some_and(|ext| ext == "zst");
        if !compressed && std::fs::metadata(path).map_or(true, |now| now.len() < self.len) {
            return Prefix::Unknown;
        }
        let mut digest = Digest::default();
        match crate::records::line_file::open(path, 0)
            .and_then(|file| std::io::copy(&mut file.take(self.len), &mut digest))
        {
            Ok(read) if read < self.len => Prefix::Unknown,
            Ok(_) if digest.0 == self.digest => Prefix::Is,
            Ok(_) => Prefix::IsNot,
            Err(_) => Prefix::Unknown,
        }
    }
}

/// What a file's content says of whether it begins with what an [`ObservedBoundary`] held of it.
#[derive(PartialEq, Eq)]
enum Prefix {
    Is,
    IsNot,
    Unknown,
}

/// FNV-1a, 64 bits: a digest whose value stays the same across daemon versions, which a stored
/// boundary relies on (the standard library's hashers promise no such thing).
struct Digest(u64);

impl Default for Digest {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Digest {
    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
}

impl std::io::Write for Digest {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// What names a file in an [`ObservedBoundary`]: its name, without a `.zst`.
fn boundary_key(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_string_lossy();
    Some(name.strip_suffix(".zst").unwrap_or(&name).to_string())
}

impl ObservedBoundary {
    fn insert(&mut self, path: &Path) {
        let (Some(key), Ok(held)) = (boundary_key(path), Held::whole(path)) else {
            return;
        };
        let held_by_name = self.files.entry(key).or_default();
        if !held_by_name.contains(&held) {
            held_by_name.push(held);
        }
    }

    fn held(&self, path: &Path) -> &[Held] {
        boundary_key(path)
            .and_then(|key| self.files.get(&key))
            .map_or(&[], Vec::as_slice)
    }

    /// Where in the file at `path` the lines written after the boundary start: the length of
    /// what it held then when it still begins with that, 0 when its content shows it is not that
    /// file (or the boundary never held one by its name), the whole file being new. `None` when it
    /// cannot be told yet, the file being shorter than what it held or unreadable: nothing is
    /// taken from it until it can.
    fn start(&self, path: &Path) -> Option<u64> {
        let mut unknown = false;
        let mut start = None;
        for held in self.held(path) {
            match held.begins(path) {
                Prefix::Is => start = start.max(Some(held.len)),
                Prefix::IsNot => {}
                Prefix::Unknown => unknown = true,
            }
        }
        match start {
            Some(start) => Some(start),
            None if unknown => None,
            None => Some(0),
        }
    }

    /// Whether the file at `path` holds exactly what it held at the boundary.
    fn unchanged(&self, path: &Path) -> bool {
        let held = self.held(path);
        !held.is_empty() && Held::whole(path).is_ok_and(|now| held.contains(&now))
    }
}

/// Captures what the records hold once the process has ended and before its replacement can
/// write: the files the collector goes on to ask [`ObservedBoundary::start`] about, which are the
/// record the process was hinted at and the one the agent now names (the head, for Codex), and,
/// where the agent keeps one, the summary file beside it. A record whose lines never give the
/// observed model (`RecordStore::observes_record_lines`) is not kept, only its summary. Reads each
/// in full, so the caller runs this on the blocking pool, with the same shell snapshot as the
/// relaunch.
pub(crate) fn observed_boundary(
    state: &AppState,
    session: &Session,
    shell: Option<HashMap<String, String>>,
) -> ObservedBoundary {
    let mut boundary = ObservedBoundary::default();
    let mut locator = shell
        .map(Locator::with_shell)
        .unwrap_or_default()
        .with_store(&state.store);
    if let Some(rows) = crate::records::rows::read(&locator, session) {
        boundary.rows = rows.ok().map(|rows| rows.len());
        if boundary.rows.is_none() {
            boundary.uncounted = Some(uuid::Uuid::new_v4());
        }
        return boundary;
    }
    let mut records = Vec::new();
    if let Some(path) = state.record_hints.hint(session) {
        records.push(path);
    }
    if let Some(path) = locator.current_session_record(session) {
        records.push(path);
    }
    let observes_lines = provider(session.agent).records().observes_record_lines();
    let mut files = Vec::new();
    for path in records {
        if let Some(summary) = summary_path(session.agent, &path) {
            files.push(summary);
        }
        if observes_lines {
            files.push(path);
        }
    }
    let mut seen = HashSet::new();
    for file in files {
        if seen.insert(file.clone()) {
            boundary.insert(&file);
        }
    }
    boundary
}

/// The summary file of the conversation whose indexed record is `record`: where the provider's
/// store puts it (`RecordStore::summary_file`), else the file of the layout's one name beside the
/// record (`AgentLayout::title_summary`); `None` for an agent that keeps none.
fn summary_path(agent: AgentId, record: &Path) -> Option<PathBuf> {
    provider(agent).records().summary_file(record).or_else(|| {
        layout(agent)
            .title_summary
            .map(|name| record.with_file_name(name))
    })
}

/// Reads the summary file of the conversation whose indexed record is `record`, when it changed,
/// into the cursor's titles and, unless it is as the observation boundary held it, its observed
/// model and effort.
fn fold_summary(agent: AgentId, record: &Path, cursor: &mut Cursor) {
    let Some(summary_path) = summary_path(agent, record) else {
        return;
    };
    let store = provider(agent).records();
    let Some(summary) = cursor
        .summary
        .read_as(&summary_path, |bytes| store.summary_in(bytes))
    else {
        return;
    };
    cursor.titles.fold_summary(provider(agent), &summary);
    // Rewritten in place rather than appended to: as it was at the boundary, it still names the
    // old process's model.
    let old = cursor
        .observed_boundary
        .as_ref()
        .is_some_and(|boundary| boundary.unchanged(&summary_path));
    if !old {
        store.observe_summary(&summary, &mut cursor.observed);
    }
}

/// Reads the file beside the indexed record `record` that gives the context window
/// (`RecordStore::window_file`), when it changed, into the cursor's context.
fn fold_window(agent: AgentId, record: &Path, cursor: &mut Cursor) {
    let store = provider(agent).records();
    let Some(file) = store.window_file() else {
        return;
    };
    if let Some(window) = cursor.window.read(&record.with_file_name(file)) {
        cursor.context.set_window(store.window_in(&window));
    }
}

/// A small file holding one value, JSON unless its reader says otherwise, read again whenever it
/// changes.
#[derive(Default)]
struct JsonFile(Option<Signature>);

impl JsonFile {
    /// What the file holds, when it differs from the last read.
    fn read(&mut self, path: &Path) -> Option<Value> {
        self.read_as(path, |bytes| serde_json::from_slice(bytes).ok())
    }

    /// [`Self::read`], with `parse` reading the file's content. Content `parse` cannot read is
    /// tried again at the next look, as content that is not JSON is.
    fn read_as(
        &mut self,
        path: &Path,
        parse: impl FnOnce(&[u8]) -> Option<Value>,
    ) -> Option<Value> {
        let metadata = std::fs::metadata(path).ok()?;
        let signature = (metadata.len(), metadata.modified().ok());
        if self.0 == Some(signature) || metadata.len() > JSON_FILE_LIMIT {
            return None;
        }
        let value = parse(&crate::hostfs::read_regular(path, JSON_FILE_LIMIT as usize).ok()?)?;
        self.0 = Some(signature);
        Some(value)
    }
}

impl Cursor {
    /// Forgets everything folded from the record and the files beside it, for a record that is
    /// read again from its start: the context with its window, the titles with the name the
    /// summary gave, the observed model and effort, and what was read of the summary and window
    /// files, which are otherwise not read again while they stay as they are. Where the record
    /// itself is read from is the caller's.
    fn forget_folded(&mut self) {
        self.context.forget();
        self.titles.clear();
        self.observed = Observed::default();
        self.summary = JsonFile::default();
        self.window = JsonFile::default();
    }
}

#[derive(Default)]
struct Cursors {
    sessions: HashMap<String, Cursor>,
    /// Each account's shared title file, by agent and account directory.
    shared_titles: HashMap<(AgentId, PathBuf), SharedTitles>,
    default_efforts: DefaultEfforts,
    row_reads: RowReads,
}

/// One blocking pass owns the cursors. Slow row queries occupy a bounded number of separate
/// blocking tasks across passes.
/// The weak reference lets a test or daemon shutdown release its state without a timer retaining it.
pub fn spawn(state: &Arc<AppState>) {
    let weak = Arc::downgrade(state);
    tokio::spawn(async move {
        let mut cursors = Cursors {
            row_reads: RowReads::with_background(),
            ..Cursors::default()
        };
        loop {
            let Some(state) = weak.upgrade() else { return };
            let result = tokio::task::spawn_blocking(move || {
                collect(&state, &mut cursors);
                cursors
            })
            .await;
            let Ok(next) = result else { return };
            cursors = next;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}

fn collect(state: &AppState, cursors: &mut Cursors) {
    // Default homes resolve through the same login-shell snapshot as agent launches.
    collect_with(state, cursors, Locator::default());
}

/// [`collect`], resolving the accounts through `locator`.
fn collect_with(state: &AppState, cursors: &mut Cursors, locator: Locator) {
    let mut locator = locator.with_store(&state.store);
    let Ok(sessions) = state.store.list_sessions() else {
        return;
    };
    // One read for the whole pass; the store refuses a model read against a replaced boundary.
    let Ok(boundaries) = state.store.observed_boundaries() else {
        return;
    };
    let ids: HashSet<_> = sessions.iter().map(|session| session.id.as_str()).collect();
    cursors.sessions.retain(|id, _| ids.contains(id.as_str()));
    cursors.row_reads.begin_pass(&ids);
    state.retain_default_efforts(&ids);
    cursors.default_efforts.begin_pass();
    let hints = state.record_hints.retain(&ids);
    let windows = state.context_windows.retain(&ids);
    state.launch_context.retain(&ids);
    // The sessions each account's shared title file is read for.
    let mut wanted: HashMap<(AgentId, PathBuf), HashSet<String>> = HashMap::new();
    let mut shared_keys: HashMap<String, (AgentId, PathBuf)> = HashMap::new();
    for session in &sessions {
        let sleeping = cursors
            .sessions
            .get(&session.id)
            .is_some_and(|cursor| cursor.without_process)
            && state.live_session(&session.id).is_none();
        if let (Some(_), Some(agent_id), false) = (
            layout(session.agent).title_index,
            session.agent_session_id.as_ref(),
            sleeping,
        ) {
            if let Some(root) = locator.root(session.agent, session.config_dir.as_deref()) {
                let key = (session.agent, root);
                wanted
                    .entry(key.clone())
                    .or_default()
                    .insert(agent_id.clone());
                shared_keys.insert(session.id.clone(), key);
            }
        }
    }
    cursors
        .shared_titles
        .retain(|key, _| wanted.contains_key(key));
    for ((agent, root), ids) in wanted {
        if let Some(file) = layout(agent).title_index {
            cursors
                .shared_titles
                .entry((agent, root.clone()))
                .or_default()
                .refresh(provider(agent), &root.join(file), ids);
        }
    }
    let mut updates = Vec::new();
    // Sessions whose default effort changed, to publish unless their record update does.
    let mut republish = Vec::new();
    for mut session in sessions {
        let identity = (
            session.agent,
            session.agent_session_id.clone(),
            session.config_dir.clone(),
        );
        let cursor = cursors.sessions.entry(session.id.clone()).or_default();
        if cursor.identity.as_ref() != Some(&identity) {
            *cursor = Cursor {
                identity: Some(identity),
                ..Default::default()
            };
        }
        let process = state.live_session(&session.id);
        let launch = process.as_ref().map(|process| process.launch_id.as_str());
        let live = launch.is_some();
        if let Some(hint) = crate::records::hinted_path(&hints, &session) {
            if cursor.path.as_ref() != Some(&hint) {
                *cursor = Cursor {
                    identity: cursor.identity.clone(),
                    path: Some(hint),
                    stored_boundary: cursor.stored_boundary.take(),
                    observed_boundary: cursor.observed_boundary.take(),
                    ..Default::default()
                };
            }
        }
        // A session whose records are not read this pass takes its default effort from the stored
        // record, since an account's catalogue can change after the session stops. A boundary
        // stored meanwhile waits for its process: a model change stores one only for a session
        // whose process it relaunches, and the change's write cleared the observed model already.
        if cursor.without_process && !live {
            settle_default_effort(
                state,
                &mut cursors.default_efforts,
                &locator,
                &session,
                (
                    session.observed_model.as_deref(),
                    session.observed_effort.as_deref(),
                ),
                &mut republish,
            );
            continue;
        }
        // The boundary also gates writes under the store lock: a pass already scanning when a
        // restart stored it may still update the context and titles, never the old model.
        let stored_boundary = boundaries.get(&session.id).cloned();
        if cursor.stored_boundary != stored_boundary {
            cursor.observed_boundary = stored_boundary
                .as_deref()
                .and_then(|text| serde_json::from_str(text).ok());
            cursor.stored_boundary = stored_boundary;
            cursor.boundary_start = None;
            // A stale pass may have consumed the replacement process's first records before its
            // write was rejected by the new boundary. Replay them through the new boundary.
            cursor.file = LineFile::default();
            cursor.rows_read = None;
            cursor.rows_held = None;
            cursor.forget_folded();
            if let Ok(current) = state.session_record(&session.id) {
                session.observed_model = current.observed_model;
                session.observed_effort = current.observed_effort;
            }
        }
        let rows_pending = if crate::records::rows::shaped(session.agent).is_some() {
            cursors
                .row_reads
                .scan(&state.record_hints, &locator, &session, launch, cursor)
        } else {
            if cursor.path.is_none() {
                cursor.path = locator.current_session_record(&session);
            }
            false
        };
        let store = provider(session.agent).records();
        let recorded = recorded_conversation(state, cursor, &locator, &session);
        if recorded == Recorded::Marked {
            // Kept in the copy this pass goes on to publish, which would otherwise take it back.
            session.has_conversation = true;
        }
        refresh_first_prompt(cursor, &mut locator, &session);
        if let Some(path) = cursor.path.clone() {
            if scan(session.agent, &path, cursor).is_ok() {
                refresh_base(cursor, &mut locator, &session);
            }
            fold_window(session.agent, &path, cursor);
            fold_summary(session.agent, &path, cursor);
        }
        // A provider's status line reports the window (Claude Code's transcript never does). The
        // last report wins, and until there is one the stored window stands, so a daemon restart,
        // a cursor started afresh or a record read again from its start (which forgets the window)
        // keeps it. Hence after the scan.
        if session.agent.descriptor().capabilities.status_line {
            let reported = windows.get(&session.id).copied();
            if let Some(window) = reported.or_else(|| session.context.as_ref()?.window) {
                cursor.context.set_window(Some(window));
            }
        }
        let report = state.live_session(&session.id).and_then(|process| {
            state
                .launch_context
                .for_launch(&session.id, &process.launch_id)
        });
        let context = cursor
            .context
            .with_report(store, report)
            .filter(|context| context != &session.context);
        let observed_model = cursor
            .observed
            .model
            .clone()
            .filter(|model| session.observed_model.as_ref() != Some(model));
        let observed_effort = if cursor.observed.effort_unset {
            Some(None)
        } else {
            cursor.observed.effort.clone().map(Some)
        }
        .filter(|effort| &session.observed_effort != effort);
        // From the values about to be stored, so the default reaches clients with the record that
        // makes it apply rather than a pass later.
        settle_default_effort(
            state,
            &mut cursors.default_efforts,
            &locator,
            &session,
            (
                observed_model
                    .as_deref()
                    .or(session.observed_model.as_deref()),
                match &observed_effort {
                    Some(effort) => effort.as_deref(),
                    None => session.observed_effort.as_deref(),
                },
            ),
            &mut republish,
        );
        let shared = shared_keys
            .get(&session.id)
            .zip(session.agent_session_id.as_deref())
            .and_then(|(key, id)| cursors.shared_titles.get(key)?.name(id));
        let mut titles = cursor.titles.merged(shared, &session.record_titles);
        if let Some((_, Some(prompt))) = &cursor.first_prompt {
            titles.prompt = Some(prompt.clone());
        }
        let titles = (titles != session.record_titles).then_some(titles);
        // A session whose mark was not written is read again, process or not, until it is.
        cursor.without_process = !live && !rows_pending && recorded != Recorded::Pending;
        if context.is_some()
            || titles.is_some()
            || observed_model.is_some()
            || observed_effort.is_some()
        {
            updates.push(RecordUpdate {
                session,
                context,
                titles,
                observed_model,
                observed_effort,
                observed_boundary: cursor.stored_boundary.clone(),
            });
        }
    }
    cursors.row_reads.start_pending(&locator);
    let mut published = HashSet::new();
    if !updates.is_empty() {
        if let Ok(changed) = state.store.set_record_data(&updates) {
            for session in changed {
                state.publish_session(&session);
                published.insert(session.id);
            }
        }
    }
    for session in republish {
        if !published.contains(&session.id) {
            state.publish_session(&session);
        }
    }
}

/// Records the effort `session`'s agent runs it at by default, given the model and effort its records
/// show, and queues the session for publication when that changed what clients are shown. A default
/// that could not be looked up leaves the one recorded before.
fn settle_default_effort(
    state: &AppState,
    defaults: &mut DefaultEfforts,
    locator: &Locator,
    session: &Session,
    (model, effort): (Option<&str>, Option<&str>),
    republish: &mut Vec<Session>,
) {
    let Some(default) = defaults.of(
        session.agent,
        session.config_dir.as_deref(),
        model,
        effort,
        locator,
    ) else {
        return;
    };
    if state.set_default_effort(&session.id, default) {
        republish.push(session.clone());
    }
}

/// What a pass found of a conversation no hook reported ([`recorded_conversation`]).
#[derive(PartialEq, Eq)]
enum Recorded {
    /// The session was marked as having a conversation by this pass.
    Marked,
    /// Its record shows a prompt the user sent, and the mark was not written: a launch holds the
    /// session, or the write failed. The next pass asks again.
    Pending,
    /// The session has a conversation already, or its record shows no prompt.
    No,
}

/// Marks `session` as having a conversation when it does not count as having one and its own
/// record shows a prompt the user sent: the mark otherwise comes from the agent's hook alone, whose
/// report of the prompt may never arrive. The conversation file is followed only while the session
/// is unmarked, and the write holds only while the session still names the conversation read and
/// no launch holds it (`AppState::mark_conversation_recorded`).
fn recorded_conversation(
    state: &AppState,
    cursor: &mut Cursor,
    locator: &Locator,
    session: &Session,
) -> Recorded {
    if session.has_conversation {
        return Recorded::No;
    }
    let (Some(record), Some(agent_id)) =
        (cursor.path.as_deref(), session.agent_session_id.as_deref())
    else {
        return Recorded::No;
    };
    let Some(file) = locator
        .alternate_root(session)
        .ok()
        .flatten()
        .filter(|(dir, depth)| {
            record
                .strip_prefix(dir)
                .is_ok_and(|relative| relative.components().count() == *depth)
        })
        .and_then(|(dir, _)| {
            user_prompt::conversation_file_under(session.agent, &dir, agent_id, record)
        })
        .or_else(|| {
            let root = locator.root(session.agent, session.config_dir.as_deref())?;
            user_prompt::conversation_file_of(session.agent, &root, agent_id, record)
        })
    else {
        return Recorded::No;
    };
    if !cursor
        .user_prompt
        .read(provider(session.agent).records(), &file)
    {
        return Recorded::No;
    }
    match state.mark_conversation_recorded(&session.id, agent_id) {
        Ok(true) => Recorded::Marked,
        Ok(false) => Recorded::Pending,
        Err(err) => {
            tracing::debug!(
                session = %session.id, %err,
                "recording the conversation the session's record shows failed"
            );
            Recorded::Pending
        }
    }
}

/// For a record that continues an earlier file (`RecordStore::continues_earlier_file`), which
/// holds only the prompts since it picked up, reads the conversation's opening prompt from the file
/// it opened in (`RecordStore::opening_file`), once per file.
fn refresh_first_prompt(cursor: &mut Cursor, locator: &mut Locator, session: &Session) {
    let Some(id) = session.agent_session_id.as_deref() else {
        return;
    };
    let store = provider(session.agent).records();
    if !cursor
        .path
        .as_deref()
        .is_some_and(|path| store.continues_earlier_file(path))
    {
        return;
    }
    let Some(first) = locator
        .root(session.agent, session.config_dir.as_deref())
        .and_then(|root| store.opening_file(locator, &root, id))
        .filter(|first| cursor.first_prompt.as_ref().map(|(from, _)| from) != Some(first))
    else {
        return;
    };
    let prompt = Titles::first_prompt_of(provider(session.agent), &first);
    cursor.first_prompt = Some((first, prompt));
}

/// For a record that continues an earlier file, which holds only the turns since it picked up and
/// so no figure until its next turn, reads the figure the conversation had where it picks up
/// (`RecordStore::earlier_figure`), once per head file.
fn refresh_base(cursor: &mut Cursor, locator: &mut Locator, session: &Session) {
    let (Some(id), Some(head)) = (session.agent_session_id.as_deref(), cursor.path.clone()) else {
        return;
    };
    let store = provider(session.agent).records();
    if cursor.context.is_shown()
        || !store.continues_earlier_file(&head)
        || cursor.context.has_base_for(&head)
    {
        return;
    }
    // A lookup that failed is tried again next pass; only a base without a figure is final.
    let Some(figure) = locator
        .root(session.agent, session.config_dir.as_deref())
        .and_then(|root| store.earlier_figure(locator, &root, id, &head))
    else {
        return;
    };
    cursor.context.set_base(head, figure);
}

#[cfg(test)]
fn scan_rows(
    hints: &crate::records::Hints,
    locator: &Locator,
    session: &Session,
    live: bool,
    cursor: &mut Cursor,
) {
    RowReads::default().scan(hints, locator, session, live.then_some("launch"), cursor);
}

/// A boundary that could not count the rows holds what the first successful read finds.
fn fold_rows(session: &Session, cursor: &mut Cursor, rows: Vec<Value>) {
    let adapter = provider(session.agent);
    let store = adapter.records();
    let held = match cursor
        .observed_boundary
        .as_ref()
        .map(|boundary| boundary.rows)
    {
        None => 0,
        Some(Some(held)) => held,
        Some(None) => *cursor.rows_held.get_or_insert(rows.len()),
    };
    cursor.forget_folded();
    crate::records::branch::for_each_row_on_branch(store.branching(), &rows, |index, row| {
        let past_boundary = index >= held;
        let line = row.to_string();
        store.fold_line(
            &line,
            past_boundary,
            &mut cursor.context,
            &mut cursor.observed,
        );
        cursor.titles.fold_line(adapter, &line);
    });
}

fn scan(agent: AgentId, path: &Path, cursor: &mut Cursor) -> std::io::Result<()> {
    let adapter = provider(agent);
    let store = adapter.records();
    let record = store.followed_file(path);
    let change = cursor.file.check(&record)?;
    if change == Change::Restarted {
        cursor.forget_folded();
    }
    if change != Change::Unchanged {
        let start = match &cursor.boundary_start {
            Some((read, start)) if *read == record && change != Change::Restarted => Some(*start),
            _ => {
                let start = cursor
                    .observed_boundary
                    .as_ref()
                    .map_or(Some(0), |boundary| boundary.start(&record));
                cursor.boundary_start = start.map(|start| (record.clone(), start));
                start
            }
        };
        let Some(branching) = store.branching() else {
            return cursor.file.read_with_position(&record, |at, line| {
                let past_boundary = start.is_some_and(|start| at >= start);
                store.fold_line(
                    line,
                    past_boundary,
                    &mut cursor.context,
                    &mut cursor.observed,
                );
                cursor.titles.fold_line(adapter, line);
            });
        };
        // A record that is a tree: the new lines are read once for their links alone, which says
        // whether they all carry on from where the record stood. When they do, they are folded as
        // they are; when one forks off, what was folded holds lines the agent abandoned, so the
        // record is folded again from its start, along the branch it now ends on.
        let from = cursor.file.offset();
        if from == 0 {
            cursor.leaf = Leaf::default();
        }
        let mut carries_on = true;
        let leaf = &mut cursor.leaf;
        cursor.file.read_with_position(&record, |_, line| {
            let link = serde_json::from_str::<Value>(line)
                .ok()
                .and_then(|line| branching.link(&line));
            carries_on &= leaf.follow(link);
        })?;
        let to = cursor.file.offset();
        if !carries_on {
            cursor.forget_folded();
        }
        let mut fold = |at: u64, line: &str| {
            let past_boundary = start.is_some_and(|start| at >= start);
            store.fold_line(
                line,
                past_boundary,
                &mut cursor.context,
                &mut cursor.observed,
            );
            cursor.titles.fold_line(adapter, line);
        };
        let folded = if carries_on {
            crate::records::line_file::read_range(&record, from, to, fold)
        } else {
            crate::records::branch::read_branch_to(branching, &record, to, &mut fold)
        };
        if folded.is_err() {
            // The links were read and the lines were not all folded: the record is read again
            // from its start, onto nothing.
            cursor.file = LineFile::default();
            cursor.forget_folded();
        }
        folded?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn post_context_hook(state: &Arc<AppState>, launch: &str, payload: serde_json::Value) {
        use crate::hooks::queue::{Push, QueuedHook};

        assert_eq!(
            state.hook_queues.push(
                "s",
                QueuedHook {
                    body: payload.to_string().into(),
                    launch: Some(launch.to_owned()),
                    event: None,
                },
            ),
            Push::StartWorker
        );
        crate::hooks::apply::drain_hook_queue(state, "s");
    }

    #[tokio::test]
    async fn a_live_projection_reports_a_rewind_without_a_record_write() {
        use crate::protocol::SessionStatus;
        use crate::test_support::{app_state, console, console_session, fake_live_session};

        let (state, directory) = app_state("record-collector-live-projection");
        state.store.insert_console(&console(&directory)).unwrap();
        let mut session = console_session("s", SessionStatus::Idle);
        session.agent = AgentId::PI;
        state.store.insert_session(&session).unwrap();
        let first = fake_live_session("s", session.agent, 80, 24, "sleep 30");
        state.register_live(first.clone());
        let mut cursors = Cursors::default();
        let mut pass = || {
            collect_with(&state, &mut cursors, Locator::with_shell(HashMap::new()));
            state.session_record("s").unwrap().context
        };
        let usage =
            |tokens| json!({"hook_event_name":"context_usage","tokens":tokens,"window":60_000});
        post_context_hook(&state, &first.launch_id, usage(Some(3485)));
        let context = pass().expect("the live projection");
        assert_eq!((context.tokens, context.window), (3485, Some(60_000)));

        post_context_hook(&state, &first.launch_id, usage(Some(140)));
        assert_eq!(pass().unwrap().tokens, 140);

        assert!(state.remove_live(&first));
        let second = fake_live_session("s", session.agent, 80, 24, "sleep 30");
        state.register_live(second.clone());
        post_context_hook(&state, &second.launch_id, usage(None));
        assert_eq!(pass(), None, "compaction left the live size unknown");

        post_context_hook(&state, &first.launch_id, usage(Some(3485)));
        assert_eq!(
            pass(),
            None,
            "the ended launch cannot replace the current one"
        );
    }

    #[tokio::test]
    async fn a_stop_hook_supplies_the_window_for_the_recorded_branch() {
        use crate::protocol::SessionStatus;
        use crate::test_support::{app_state, console, console_session, fake_live_session};

        let (state, directory) = app_state("record-collector-hook-window");
        state.store.insert_console(&console(&directory)).unwrap();
        let mut session = console_session("s", SessionStatus::Idle);
        session.agent = AgentId::QWEN;
        let mut stop = include_str!("../../testdata/qwen_hooks.jsonl")
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .find(|payload| payload["hook_event_name"] == "Stop")
            .expect("the captured stop hook");
        let id = stop["session_id"].as_str().unwrap();
        session.agent_session_id = Some(id.to_owned());
        let root = directory.join("account");
        let chats = root.join("projects/project/chats");
        std::fs::create_dir_all(&chats).unwrap();
        let chat = chats.join(format!("{id}.jsonl"));
        std::fs::write(
            &chat,
            include_str!("../../testdata/qwen_record_rewind.jsonl"),
        )
        .unwrap();
        stop["transcript_path"] = json!(chat);
        session.config_dir = Some(root.to_str().unwrap().to_owned());
        state.store.insert_session(&session).unwrap();
        let live = fake_live_session("s", session.agent, 80, 24, "sleep 30");
        state.register_live(live.clone());
        post_context_hook(&state, &live.launch_id, stop);
        collect_with(
            &state,
            &mut Cursors::default(),
            Locator::with_shell(HashMap::new()),
        );
        let context = state.session_record("s").unwrap().context.unwrap();
        assert_eq!((context.tokens, context.window), (23, Some(200_000)));
    }

    #[cfg(unix)]
    #[test]
    fn a_json_record_must_be_a_regular_file() {
        let dir = crate::test_support::ScratchDir::new("json-record-kind");
        let path = dir.join("record.json");
        crate::test_support::mkfifo(&path);
        let mut reader = JsonFile::default();
        assert!(reader.read(&path).is_none());

        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, br#"{"title":"A record"}"#).unwrap();
        assert_eq!(
            reader.read(&path),
            Some(serde_json::json!({"title": "A record"}))
        );
    }

    #[test]
    fn collector_backfills_archives_publishes_changes_and_refreshes_resumed_sources() {
        let (state, directory) = crate::test_support::app_state("record-collector");
        let console = crate::protocol::Console {
            id: "console".to_owned(),
            name: "Console".to_owned(),
            workdir: directory.join("console").to_string_lossy().into_owned(),
            default_agent: AgentId::CLAUDE,
            project_session_agent: None,
            accounts: Default::default(),
            launch: Default::default(),
            project_session_launch: Default::default(),
            icon: None,
            console_session_prompt: None,
            created_at: 0,
            versions: Default::default(),
        };
        state.store.insert_console(&console).unwrap();
        let root = directory.join("account");
        let record = root.join("projects/project/native.jsonl");
        std::fs::create_dir_all(record.parent().unwrap()).unwrap();
        let session: crate::protocol::Session = serde_json::from_value(json!({
            "id":"s","agent":"claude","agent_session_id":"native","console_id":"console",
            "host_id":"local","kind":"console","origin":"user","title":"Session",
            "status":"archived","role":"coordinator","has_conversation":true,"pinned":false,"started_at":0,
            "config_dir":root.to_string_lossy()}))
        .unwrap();
        state.store.insert_session(&session).unwrap();
        let capture = include_str!("../../testdata/usage_claude_2.1.296.jsonl");
        let initial = capture.lines().take(3).collect::<Vec<_>>().join("\n") + "\n";
        std::fs::write(&record, initial).unwrap();
        let mut events = state.subscribe();
        let mut cursors = Cursors::default();
        collect(&state, &mut cursors);
        let before = state.session_record("s").unwrap().context.unwrap();
        assert_eq!(before.tokens, 48_762);
        assert!(matches!(
            events.try_recv().unwrap(),
            crate::protocol::Event::SessionUpserted { .. }
        ));
        std::fs::write(&record, capture).unwrap();
        collect(&state, &mut cursors);
        assert_eq!(
            state.session_record("s").unwrap().context.as_ref(),
            Some(&before)
        );
        assert!(
            events.try_recv().is_err(),
            "an archived source is not repeatedly scanned"
        );
        let live = crate::test_support::idle_stand_in("s");
        state.register_live(live.clone());
        collect(&state, &mut cursors);
        assert_eq!(
            state.session_record("s").unwrap().context.unwrap().tokens,
            52_051
        );
        assert!(matches!(
            events.try_recv().unwrap(),
            crate::protocol::Event::SessionUpserted { .. }
        ));
        collect(&state, &mut cursors);
        assert!(
            events.try_recv().is_err(),
            "an unchanged context emits no extra events"
        );
    }

    /// The window Claude Code's status line reported is stored with the transcript's size, and a
    /// daemon that starts again with nothing reported keeps the stored one rather than clearing it.
    #[test]
    fn a_claude_sessions_reported_window_is_stored_and_outlives_a_restart() {
        let (state, directory) = crate::test_support::app_state("record-collector-claude-window");
        state
            .store
            .insert_console(&crate::test_support::console(&directory))
            .unwrap();
        let console_id = state.store.list_consoles().unwrap()[0].id.clone();
        let root = directory.join("account");
        let record = root.join("projects/project/native.jsonl");
        std::fs::create_dir_all(record.parent().unwrap()).unwrap();
        std::fs::write(&record, claude_request(48_762) + "\n").unwrap();
        let session: crate::protocol::Session = serde_json::from_value(json!({
            "id":"s","agent":"claude","agent_session_id":"native","console_id":console_id,
            "host_id":"local","kind":"console","origin":"user","title":"Session",
            "status":"idle","role":"coordinator","has_conversation":true,"pinned":false,"started_at":0,
            "config_dir":root.to_string_lossy()}))
        .unwrap();
        state.store.insert_session(&session).unwrap();
        let live = crate::test_support::idle_stand_in("s");
        state.register_live(live.clone());
        let mut cursors = Cursors::default();
        collect(&state, &mut cursors);
        assert_eq!(
            state.session_record("s").unwrap().context.unwrap().window,
            None
        );

        state.context_windows.report("s", 200_000);
        collect(&state, &mut cursors);
        let expected = crate::protocol::SessionContext {
            tokens: 48_762,
            window: Some(200_000),
            used_percent: Some(24),
        };
        assert_eq!(
            state.session_record("s").unwrap().context,
            Some(expected.clone())
        );

        let (restarted, restarted_dir) =
            crate::test_support::app_state("record-collector-claude-window-restart");
        restarted
            .store
            .insert_console(&crate::test_support::console(&restarted_dir))
            .unwrap();
        restarted
            .store
            .insert_session(&state.session_record("s").unwrap())
            .unwrap();
        collect(&restarted, &mut Cursors::default());
        assert_eq!(
            restarted.session_record("s").unwrap().context,
            Some(expected)
        );
    }

    #[test]
    fn a_codex_sessions_indexed_name_reaches_the_store_and_the_clients() {
        let (state, directory) = crate::test_support::app_state("record-collector-title");
        state
            .store
            .insert_console(&crate::test_support::console(&directory))
            .unwrap();
        let root = directory.join("account");
        std::fs::create_dir_all(&root).unwrap();
        let console_id = state.store.list_consoles().unwrap()[0].id.clone();
        let session: crate::protocol::Session = serde_json::from_value(json!({
            "id":"s","agent":"codex","agent_session_id":"thread-1","console_id":console_id,
            "host_id":"local","kind":"console","origin":"user","title":"Session",
            "status":"archived","role":"coordinator","has_conversation":true,"pinned":false,"started_at":0,
            "config_dir":root.to_string_lossy()}))
        .unwrap();
        state.store.insert_session(&session).unwrap();
        std::fs::write(
            root.join("session_index.jsonl"),
            concat!(
                r#"{"id":"thread-1","thread_name":"Explain mutex briefly","updated_at":"t"}"#,
                "\n",
                r#"{"id":"thread-2","thread_name":"Another","updated_at":"t"}"#,
                "\n"
            ),
        )
        .unwrap();
        let mut events = state.subscribe();
        collect(&state, &mut Cursors::default());
        let crate::protocol::Event::SessionUpserted { session } = events.try_recv().unwrap() else {
            panic!("expected the session to be published")
        };
        assert_eq!(
            session.terminal_title.as_deref(),
            Some("Explain mutex briefly")
        );
        assert_eq!(
            state
                .session_record("s")
                .unwrap()
                .record_titles
                .named
                .as_deref(),
            Some("Explain mutex briefly")
        );
    }

    /// A rollout Codex has compressed is found by the record index and read through its
    /// decompression, so a session idle for a week keeps its context size.
    #[test]
    fn a_compressed_codex_rollout_is_indexed_and_read() {
        const ID: &str = "019a0000-0000-7000-8000-000000000001";
        let (state, directory) = crate::test_support::app_state("record-collector-compressed");
        state
            .store
            .insert_console(&crate::test_support::console(&directory))
            .unwrap();
        let console_id = state.store.list_consoles().unwrap()[0].id.clone();
        let root = directory.join("account");
        let rollout = root
            .join("sessions/2026/10/01")
            .join(format!("rollout-2026-10-01T00-00-00-{ID}.jsonl.zst"));
        std::fs::create_dir_all(rollout.parent().unwrap()).unwrap();
        let capture = include_str!("../../testdata/usage_codex_0.162.1.jsonl");
        std::fs::write(&rollout, zstd::encode_all(capture.as_bytes(), 0).unwrap()).unwrap();
        let session: crate::protocol::Session = serde_json::from_value(json!({
            "id":"s","agent":"codex","agent_session_id":ID,"console_id":console_id,
            "host_id":"local","kind":"console","origin":"user","title":"Session",
            "status":"archived","role":"coordinator","has_conversation":true,"pinned":false,"started_at":0,
            "config_dir":root.to_string_lossy()}))
        .unwrap();
        state.store.insert_session(&session).unwrap();

        collect(&state, &mut Cursors::default());

        assert_eq!(
            state.session_record("s").unwrap().context,
            Some(crate::protocol::SessionContext {
                tokens: 18_829,
                window: Some(475_000),
                used_percent: Some(1),
            })
        );
    }

    /// What the records showed is forgotten once a change of the session's model asks for it, and
    /// not written back from the lines already read; what the agent records afterwards is taken.
    #[test]
    fn a_forgotten_observed_model_is_read_afresh_from_what_the_agent_records_next() {
        const ID: &str = "019a0000-0000-7000-8000-000000000001";
        let (state, directory) = crate::test_support::app_state("record-collector-forget-observed");
        state
            .store
            .insert_console(&crate::test_support::console(&directory))
            .unwrap();
        let console_id = state.store.list_consoles().unwrap()[0].id.clone();
        let root = directory.join("account");
        let rollout = root
            .join("sessions/2026/10/01")
            .join(format!("rollout-2026-10-01T00-00-00-{ID}.jsonl"));
        std::fs::create_dir_all(rollout.parent().unwrap()).unwrap();
        let named = include_str!("../../testdata/observed_codex_0.162.1.jsonl");
        std::fs::write(&rollout, named).unwrap();
        let session: crate::protocol::Session = serde_json::from_value(json!({
            "id":"s","agent":"codex","agent_session_id":ID,"console_id":console_id,
            "host_id":"local","kind":"console","origin":"user","title":"Session",
            "status":"idle","role":"coordinator","has_conversation":true,"pinned":false,"started_at":0,
            "config_dir":root.to_string_lossy()}))
        .unwrap();
        state.store.insert_session(&session).unwrap();
        let live = crate::test_support::idle_stand_in("s");
        state.register_live(live.clone());
        let mut cursors = Cursors::default();
        let observed = || {
            let record = state.session_record("s").unwrap();
            (record.observed_model, record.observed_effort)
        };

        collect(&state, &mut cursors);
        let shown = observed();
        assert!(shown.0.is_some(), "the records name a model");
        // The old process wrote a final turn after the last one-second collector pass.
        let final_context = codex_count(json!({"last_token_usage":{"total_tokens":123}}));
        let old = format!("{named}{named}{final_context}\n");
        std::fs::write(&rollout, &old).unwrap();
        state
            .store
            .reset_observed_model(
                "s",
                &serde_json::to_string(&observed_boundary(&state, &session, None)).unwrap(),
            )
            .unwrap();
        collect(&state, &mut cursors);
        assert_eq!(observed(), (None, None));
        assert_eq!(
            state.session_record("s").unwrap().context.unwrap().tokens,
            123,
            "the unread tail still updates unrelated context"
        );
        collect(&state, &mut cursors);
        assert_eq!(observed(), (None, None), "nothing already read comes back");
        std::fs::write(&rollout, format!("{old}{named}")).unwrap();
        collect(&state, &mut cursors);
        assert_eq!(observed(), shown, "the next turn is read");
    }

    #[test]
    fn a_model_boundary_survives_a_fresh_daemon_and_accepts_new_observations() {
        const ID: &str = "019a0000-0000-7000-8000-000000000001";
        let (state, directory) = crate::test_support::app_state("model-boundary-restart");
        state
            .store
            .insert_console(&crate::test_support::console(&directory))
            .unwrap();
        let root = directory.join("account");
        let rollout = root
            .join("sessions/2026/10/01")
            .join(format!("rollout-2026-10-01T00-00-00-{ID}.jsonl"));
        std::fs::create_dir_all(rollout.parent().unwrap()).unwrap();
        let old = include_str!("../../testdata/observed_codex_0.162.1.jsonl");
        std::fs::write(&rollout, old).unwrap();
        codex_session(&state, &root, ID);
        collect(&state, &mut Cursors::default());
        assert!(state.session_record("s").unwrap().observed_model.is_some());
        let boundary = serde_json::to_string(&observed_boundary(
            &state,
            &state.session_record("s").unwrap(),
            None,
        ))
        .unwrap();
        // The production write stores the boundary and the new override at once.
        let mut session = state.session_record("s").unwrap();
        session.status = crate::protocol::SessionStatus::Interrupted;
        state.store.update_session(&session).unwrap();
        assert!(state
            .store
            .set_session_launch_overrides_unless_archived(
                "s",
                &Some(Some("gpt-6.1-sol".to_string())),
                &None,
                Some(&boundary)
            )
            .unwrap());
        drop(state);
        let state = Arc::new(AppState::new(
            crate::store::Store::open(&directory.join("octoboard.db")).unwrap(),
            1234,
            crate::test_support::TEST_KEY.to_string(),
            "/opt/octoboardd".to_string(),
            "/opt/octoboardd".to_string(),
            directory.join("output"),
            directory.join("consoles"),
            directory.join("handoff"),
            directory.join("agent-query-homes"),
            crate::processes::ledger::ProcessLedger::new(&directory),
        ));
        state.process_ledger.without_scanner();
        let mut cursors = Cursors::default();
        collect(&state, &mut cursors);
        let after = state.session_record("s").unwrap();
        assert_eq!((after.observed_model, after.observed_effort), (None, None));
        assert_eq!(after.model, Some(Some("gpt-6.1-sol".to_string())));
        let live = crate::test_support::idle_stand_in("s");
        state.register_live(live.clone());
        // A real turn context captured from the same fixture, after the restart boundary.
        let new = old.lines().next().unwrap();
        std::fs::write(&rollout, format!("{old}{new}\n")).unwrap();
        // A scan in flight from before the boundary can consume the new line, yet have its write
        // rejected.
        let cursor = cursors.sessions.get_mut("s").unwrap();
        cursor.stored_boundary = None;
        cursor.observed_boundary = None;
        cursor.boundary_start = None;
        scan(AgentId::CODEX, &rollout, cursor).unwrap();
        state
            .store
            .set_record_data(&[RecordUpdate {
                session: state.session_record("s").unwrap(),
                context: None,
                titles: None,
                observed_model: cursor.observed.model.clone(),
                observed_effort: cursor.observed.effort.clone().map(Some),
                observed_boundary: None,
            }])
            .unwrap();
        assert!(state.session_record("s").unwrap().observed_model.is_none());
        collect(&state, &mut cursors);
        assert_eq!(
            state.session_record("s").unwrap().observed_model.as_deref(),
            Some("gpt-6.1-sol")
        );
        // A second fresh collector also observes the post-boundary turn, not the old last model.
        collect(&state, &mut Cursors::default());
        assert_eq!(
            state.session_record("s").unwrap().observed_model.as_deref(),
            Some("gpt-6.1-sol")
        );
    }

    /// A boundary knows a file by its name and what it held, so moving it to another directory
    /// leaves it old, while a rewrite in place of the same length is new output.
    #[test]
    fn an_observation_boundary_follows_a_moved_file_but_not_a_rewrite() {
        let dir = crate::test_support::ScratchDir::new("model-boundary-path");
        let (original, moved) = (dir.join("a/rollout.jsonl"), dir.join("b/rollout.jsonl"));
        std::fs::create_dir_all(original.parent().unwrap()).unwrap();
        std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
        let capture = include_str!("../../testdata/observed_codex_0.162.1.jsonl");
        std::fs::write(&original, capture).unwrap();
        let mut boundary = ObservedBoundary::default();
        boundary.insert(&original);
        std::fs::rename(&original, &moved).unwrap();
        let mut cursor = Cursor {
            observed_boundary: Some(boundary),
            ..Default::default()
        };
        scan(AgentId::CODEX, &moved, &mut cursor).unwrap();
        assert!(
            cursor.observed.model.is_none(),
            "a moved old file stays old"
        );
        let rewritten = capture.replace("codex-auto-review", "codex-next-review");
        assert_eq!(rewritten.len(), capture.len());
        std::fs::write(&moved, &rewritten).unwrap();
        // A later modification time, so the cursor sees the rewrite.
        std::fs::File::options()
            .write(true)
            .open(&moved)
            .unwrap()
            .set_modified(std::time::SystemTime::now() + Duration::from_secs(60))
            .unwrap();
        scan(AgentId::CODEX, &moved, &mut cursor).unwrap();
        assert_eq!(
            cursor.observed.model.as_deref(),
            Some("codex-next-review"),
            "a same-length in-place rewrite is new output"
        );
    }

    /// Codex compresses a rollout it is done with, and writes a compressed head out again as a
    /// new plain file when it resumes the thread: either way the old history stays old, and only
    /// the turns after it are read.
    #[test]
    fn an_observation_boundary_survives_codex_compressing_and_decompressing_a_rollout() {
        let dir = crate::test_support::ScratchDir::new("model-boundary-zst");
        let capture = include_str!("../../testdata/observed_codex_0.162.1.jsonl");
        let next = capture.replace("codex-auto-review", "codex-next-review");
        let (plain, compressed) = (dir.join("rollout.jsonl"), dir.join("rollout.jsonl.zst"));
        let compress = |text: &str| zstd::stream::encode_all(text.as_bytes(), 1).unwrap();

        // Taken from the plain file, read once Codex has compressed it.
        std::fs::write(&plain, capture).unwrap();
        let mut boundary = ObservedBoundary::default();
        boundary.insert(&plain);
        std::fs::remove_file(&plain).unwrap();
        std::fs::write(&compressed, compress(capture)).unwrap();
        let mut cursor = Cursor {
            observed_boundary: Some(boundary),
            ..Default::default()
        };
        scan(AgentId::CODEX, &compressed, &mut cursor).unwrap();
        assert!(
            cursor.observed.model.is_none(),
            "compressing old history does not make it new"
        );

        // Taken from the compressed head, which the resumed thread writes out as a new plain
        // file and appends its next turn to.
        let mut boundary = ObservedBoundary::default();
        boundary.insert(&compressed);
        std::fs::remove_file(&compressed).unwrap();
        std::fs::write(&plain, capture).unwrap();
        let mut cursor = Cursor {
            observed_boundary: Some(boundary),
            ..Default::default()
        };
        scan(AgentId::CODEX, &plain, &mut cursor).unwrap();
        assert!(cursor.observed.model.is_none(), "the old history stays old");
        std::fs::write(&plain, format!("{capture}{next}")).unwrap();
        scan(AgentId::CODEX, &plain, &mut cursor).unwrap();
        assert_eq!(cursor.observed.model.as_deref(), Some("codex-next-review"));
        // Read by a fresh cursor, as after a daemon restart, the old part is still skipped.
        let mut fresh = Cursor {
            observed_boundary: cursor.observed_boundary.take(),
            ..Default::default()
        };
        std::fs::write(&plain, format!("{capture}{capture}")).unwrap();
        scan(AgentId::CODEX, &plain, &mut fresh).unwrap();
        assert_eq!(fresh.observed.model.as_deref(), Some("codex-auto-review"));
        std::fs::write(&plain, capture).unwrap();
        let mut fresh = Cursor {
            observed_boundary: fresh.observed_boundary.take(),
            ..Default::default()
        };
        scan(AgentId::CODEX, &plain, &mut fresh).unwrap();
        assert!(fresh.observed.model.is_none());
    }

    /// A session whose model was changed and which was then switched to another account reads
    /// its conversation from the copy in that account's directory: what the copy holds of the
    /// old process's records gives no model, and what the relaunch writes after it does.
    #[test]
    fn an_observation_boundary_survives_an_account_switch_copying_the_claude_transcript() {
        const ID: &str = "00000000-0000-4000-8000-0000000000aa";
        let (state, directory) = crate::test_support::app_state("model-boundary-switch");
        state
            .store
            .insert_console(&crate::test_support::console(&directory))
            .unwrap();
        let console_id = state.store.list_consoles().unwrap()[0].id.clone();
        let transcript = |account: &str| {
            directory
                .join(account)
                .join("projects/-work")
                .join(format!("{ID}.jsonl"))
        };
        let capture = include_str!("../../testdata/observed_claude.jsonl");
        std::fs::create_dir_all(transcript("a").parent().unwrap()).unwrap();
        std::fs::write(transcript("a"), capture).unwrap();
        let session: crate::protocol::Session = serde_json::from_value(json!({
            "id":"s","agent":"claude","agent_session_id":ID,"console_id":console_id,
            "host_id":"local","kind":"console","origin":"user","title":"Session",
            "status":"idle","role":"coordinator","has_conversation":true,"pinned":false,"started_at":0,
            "config_dir":directory.join("a").to_string_lossy()}))
        .unwrap();
        state.store.insert_session(&session).unwrap();
        let live = crate::test_support::idle_stand_in("s");
        state.register_live(live.clone());
        let mut cursors = Cursors::default();
        let observed = || state.session_record("s").unwrap().observed_model;
        collect(&state, &mut cursors);
        assert_eq!(observed().as_deref(), Some("claude-opus-5-5"));

        let boundary = observed_boundary(&state, &session, None);
        state
            .store
            .reset_observed_model("s", &serde_json::to_string(&boundary).unwrap())
            .unwrap();
        // The switch copies the transcript into the other account and moves the session there.
        std::fs::create_dir_all(transcript("b").parent().unwrap()).unwrap();
        std::fs::copy(transcript("a"), transcript("b")).unwrap();
        let target = directory.join("b").to_string_lossy().into_owned();
        state
            .set_session_account("s", Some("b"), Some(&target))
            .unwrap();
        collect(&state, &mut cursors);
        assert_eq!(observed(), None, "the copied old records give no model");
        collect(&state, &mut Cursors::default());
        assert_eq!(observed(), None, "nor after a daemon restart");

        let next = json!({"type":"assistant","isSidechain":false,"message":{
            "model":"claude-sonnet-5","usage":{"input_tokens":1,"output_tokens":1}}});
        std::fs::write(transcript("b"), format!("{capture}{next}\n")).unwrap();
        collect(&state, &mut cursors);
        assert_eq!(observed().as_deref(), Some("claude-sonnet-5"));
    }

    /// Grok keeps its configured model in a summary file it rewrites in place: copied unchanged
    /// into another account it still names the old model, and once rewritten it names the new one.
    #[test]
    fn a_grok_summary_copied_unchanged_by_an_account_switch_stays_old() {
        let dir = crate::test_support::ScratchDir::new("model-boundary-grok");
        let capture = include_str!("../../testdata/titles_grok_1.0.60_summary.json");
        let (old, copied) = (
            dir.join("a/sessions/proj/grok-1/summary.json"),
            dir.join("b/sessions/proj/grok-1/summary.json"),
        );
        for path in [&old, &copied] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        }
        std::fs::write(&old, capture).unwrap();
        let mut boundary = ObservedBoundary::default();
        boundary.insert(&old);
        std::fs::copy(&old, &copied).unwrap();
        assert!(boundary.unchanged(&copied));
        std::fs::write(&copied, capture.replace("\"grok-4.7\"", "\"grok-4.8\"")).unwrap();
        assert!(!boundary.unchanged(&copied));
    }

    /// A Codex turn that names its model and no effort unsets the effort an earlier turn named,
    /// and the session reaches clients in the same upsert with the model's default from the
    /// account's catalogue, published only when it changes.
    #[test]
    fn a_codex_turn_that_names_no_effort_is_published_with_the_catalogue_default() {
        const ID: &str = "019a0000-0000-7000-8000-000000000001";
        let (state, directory) = crate::test_support::app_state("record-collector-default-effort");
        state
            .store
            .insert_console(&crate::test_support::console(&directory))
            .unwrap();
        let console_id = state.store.list_consoles().unwrap()[0].id.clone();
        let root = directory.join("account");
        let rollout = root
            .join("sessions/2026/10/01")
            .join(format!("rollout-2026-10-01T00-00-00-{ID}.jsonl"));
        std::fs::create_dir_all(rollout.parent().unwrap()).unwrap();
        let named = include_str!("../../testdata/observed_codex_0.162.1.jsonl");
        std::fs::write(&rollout, named).unwrap();
        std::fs::write(
            root.join("models_cache.json"),
            include_str!("../../testdata/codex_models_cache_0.162.1.json"),
        )
        .unwrap();
        let session: crate::protocol::Session = serde_json::from_value(json!({
            "id":"s","agent":"codex","agent_session_id":ID,"console_id":console_id,
            "host_id":"local","kind":"console","origin":"user","title":"Session",
            "status":"idle","role":"coordinator","has_conversation":true,"pinned":false,"started_at":0,
            "config_dir":root.to_string_lossy()}))
        .unwrap();
        state.store.insert_session(&session).unwrap();
        let live = crate::test_support::idle_stand_in("s");
        state.register_live(live.clone());
        let mut events = state.subscribe();
        let mut cursors = Cursors::default();
        let mut published = || {
            let mut sessions = Vec::new();
            while let Ok(crate::protocol::Event::SessionUpserted { session }) = events.try_recv() {
                sessions.push((session.observed_effort, session.default_effort));
            }
            sessions
        };

        collect(&state, &mut cursors);
        assert_eq!(published(), [(Some("low".to_owned()), None)]);
        let unset = include_str!("../../testdata/observed_codex_unset_effort_0.162.1.jsonl");
        std::fs::write(&rollout, format!("{named}{unset}")).unwrap();
        collect(&state, &mut cursors);
        assert_eq!(published(), [(None, Some("medium".to_owned()))]);
        assert!(state.session_record("s").unwrap().observed_effort.is_none());
        collect(&state, &mut cursors);
        assert!(
            published().is_empty(),
            "an unchanged default is not published again"
        );
    }

    /// A reverted Codex thread's record is its head segment: the context size is the one the head
    /// ends on, whichever segment was written last; the prompt title is the opening one, from the
    /// thread's first rollout.
    #[test]
    fn a_reverted_codex_threads_context_and_titles_come_from_its_head() {
        const ID: &str = "019a0000-0000-7000-8000-000000000001";
        for compressed in [false, true] {
            let (state, directory) = crate::test_support::app_state("record-collector-chain");
            state
                .store
                .insert_console(&crate::test_support::console(&directory))
                .unwrap();
            let console_id = state.store.list_consoles().unwrap()[0].id.clone();
            let root = directory.join("account");
            crate::test_support::codex_chain(&root, ID, compressed);
            let session: crate::protocol::Session = serde_json::from_value(json!({
                "id":"s","agent":"codex","agent_session_id":ID,"console_id":console_id,
                "host_id":"local","kind":"console","origin":"user","title":"Session",
                "status":"archived","role":"coordinator","has_conversation":true,"pinned":false,"started_at":0,
                "config_dir":root.to_string_lossy()}))
            .unwrap();
            state.store.insert_session(&session).unwrap();

            collect(&state, &mut Cursors::default());

            let record = state.session_record("s").unwrap();
            assert_eq!(
                record.context.unwrap().tokens,
                300,
                "compressed: {compressed}"
            );
            assert_eq!(record.record_titles.prompt.as_deref(), Some("first prompt"));
        }
    }

    /// A shared records root can be linked even though destructive record discovery never
    /// follows links. The hook still names the head, and its first prompt must not rename the
    /// conversation after a revert, including one whose incorrect title was already saved.
    #[test]
    fn a_reverted_codex_thread_keeps_its_opening_title_in_linked_records() {
        const ID: &str = "019a0000-0000-7000-8000-000000000001";
        let (state, directory) = crate::test_support::app_state("record-collector-linked-title");
        state
            .store
            .insert_console(&crate::test_support::console(&directory))
            .unwrap();
        let console_id = state.store.list_consoles().unwrap()[0].id.clone();
        let shared = directory.join("shared");
        let chain = crate::test_support::codex_chain(&shared, ID, false);
        let root = directory.join("account");
        std::fs::create_dir_all(&root).unwrap();
        crate::test_support::symlink(shared.join("sessions"), root.join("sessions")).unwrap();
        let session: crate::protocol::Session = serde_json::from_value(json!({
            "id":"s","agent":"codex","agent_session_id":ID,"console_id":console_id,
            "host_id":"local","kind":"console","origin":"user","title":"Session",
            "status":"archived","role":"coordinator","has_conversation":true,"pinned":false,"started_at":0,
            "config_dir":root.to_string_lossy(),
            "record_titles":{"named":null,"prompt":"head prompt"}}))
        .unwrap();
        state.store.insert_session(&session).unwrap();
        crate::records::on_hook(
            &state,
            "s",
            crate::adapter::provider(crate::protocol::AgentId::CODEX).records(),
            &json!({
                "session_id":ID,
                "transcript_path":root.join(chain.head.strip_prefix(&shared).unwrap())
            }),
        );

        collect(&state, &mut Cursors::default());

        assert_eq!(
            state
                .session_record("s")
                .unwrap()
                .record_titles
                .prompt
                .as_deref(),
            Some("first prompt")
        );
        let codex = provider(AgentId::CODEX).records();
        let mut locator = Locator::default();
        assert!(codex.opening_file(&mut locator, &root, ID).is_some());
        assert!(
            crate::records::locate::record_paths(&state.record_hints, &mut locator, &[], &session)
                .is_empty(),
            "reading a title must not make linked records removable"
        );

        let year = shared.join("sessions/2026");
        let outside = directory.join("outside");
        std::fs::rename(&year, &outside).unwrap();
        crate::test_support::symlink(&outside, &year).unwrap();
        assert!(
            codex
                .opening_file(&mut Locator::default(), &root, ID)
                .is_none(),
            "links below the records root are not traversed"
        );
    }

    #[test]
    fn claudes_last_main_transcript_turn_names_the_model_and_effort() {
        let capture = include_str!("../../testdata/observed_claude.jsonl");
        let file = crate::test_support::ScratchFile::new("record-observed", "session.jsonl");
        let first = capture.lines().next().unwrap().to_owned() + "\n";
        std::fs::write(&file, &first).unwrap();
        let mut cursor = Cursor::default();
        scan(AgentId::CLAUDE, &file, &mut cursor).unwrap();
        assert_eq!(cursor.observed.model.as_deref(), Some("claude-opus-5"));
        assert_eq!(cursor.observed.effort.as_deref(), Some("high"));
        // The later turn wins; the sidechain turn and the synthetic notice after it do not.
        std::fs::write(&file, capture).unwrap();
        scan(AgentId::CLAUDE, &file, &mut cursor).unwrap();
        assert_eq!(cursor.observed.model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(cursor.observed.effort.as_deref(), Some("medium"));
        // A record that restarts leaves nothing of the old one behind.
        std::fs::write(&file, r#"{"type":"user"}"#.to_owned() + "\n").unwrap();
        scan(AgentId::CLAUDE, &file, &mut cursor).unwrap();
        assert!(cursor.observed.model.is_none() && cursor.observed.effort.is_none());
    }

    #[test]
    fn codexs_last_turn_context_names_the_model_and_effort() {
        let file = crate::test_support::ScratchFile::new("record-observed-codex", "rollout.jsonl");
        std::fs::write(
            &file,
            include_str!("../../testdata/observed_codex_0.162.1.jsonl"),
        )
        .unwrap();
        let mut cursor = Cursor::default();
        scan(AgentId::CODEX, &file, &mut cursor).unwrap();
        assert_eq!(cursor.observed.model.as_deref(), Some("codex-auto-review"));
        assert_eq!(cursor.observed.effort.as_deref(), Some("low"));
    }

    #[test]
    fn groks_summary_names_the_configured_model_and_effort_again_when_it_changes() {
        let directory = crate::test_support::ScratchDir::new("record-observed-grok");
        let path = directory.join("summary.json");
        let capture = include_str!("../../testdata/titles_grok_1.0.60_summary.json");
        std::fs::write(&path, capture).unwrap();
        let (mut file, mut observed) = (JsonFile::default(), Observed::default());
        let grok = provider(AgentId::GROK).records();
        grok.observe_summary(&file.read(&path).unwrap(), &mut observed);
        assert_eq!(observed.model.as_deref(), Some("grok-4.7"));
        assert_eq!(observed.effort.as_deref(), Some("high"));
        assert!(
            file.read(&path).is_none(),
            "an unchanged file is not read again"
        );
        std::fs::write(
            &path,
            capture
                .replace("\"grok-4.7\"", "\"grok-4.8-mini\"")
                .replace("\"high\"", "\"low\""),
        )
        .unwrap();
        grok.observe_summary(&file.read(&path).unwrap(), &mut observed);
        assert_eq!(observed.model.as_deref(), Some("grok-4.8-mini"));
        assert_eq!(observed.effort.as_deref(), Some("low"));
    }

    /// A subagent's transcript is not read at all, so nothing of what it shows reaches the session.
    #[test]
    fn a_subagents_turns_do_not_change_the_sessions_context_model_and_effort() {
        let capture = include_str!("../../testdata/observed_claude.jsonl");
        let mut lines = capture.lines();
        let (main, sidechain) = (lines.nth(1).unwrap(), lines.next().unwrap());
        let file = crate::test_support::ScratchFile::new("record-observed-family", "session.jsonl");
        std::fs::write(&file, main.to_owned() + "\n").unwrap();
        let directory = file.with_extension("").join("subagents");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("agent-child.jsonl"),
            sidechain.to_owned() + "\n",
        )
        .unwrap();
        let mut cursor = Cursor::default();
        scan(AgentId::CLAUDE, &file, &mut cursor).unwrap();
        assert_eq!(cursor.observed.model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(cursor.observed.effort.as_deref(), Some("medium"));
        assert!(matches!(
            cursor.context.context(provider(AgentId::CLAUDE).records()),
            Some(Some(_))
        ));
    }

    fn claude_request(input: u64) -> String {
        json!({"type":"assistant","message":{"model":"claude-opus-5","usage":{
            "input_tokens":input,"output_tokens":1}}})
        .to_string()
    }

    /// The compaction rule itself is `context.rs`'s; here is what the file read adds.
    #[test]
    fn an_unfinished_claude_line_waits_and_a_restarted_record_is_forgotten() {
        let file = crate::test_support::ScratchFile::new("record-claude-lines", "session.jsonl");
        let (request, next) = (claude_request(3), claude_request(9));
        std::fs::write(&file, format!("{request}\n{next}")).unwrap();
        let mut cursor = Cursor::default();
        scan(AgentId::CLAUDE, &file, &mut cursor).unwrap();
        let tokens = |cursor: &Cursor| {
            cursor
                .context
                .context(provider(AgentId::CLAUDE).records())
                .map(|context| context.unwrap().tokens)
        };
        assert_eq!(tokens(&cursor), Some(3), "an unfinished line waits");
        std::fs::write(&file, format!("{request}\n{next}\n")).unwrap();
        scan(AgentId::CLAUDE, &file, &mut cursor).unwrap();
        assert_eq!(tokens(&cursor), Some(9));
        // A record that restarts leaves nothing of the old one behind.
        std::fs::write(&file, "{\"type\":\"user\"}\n").unwrap();
        scan(AgentId::CLAUDE, &file, &mut cursor).unwrap();
        assert_eq!(tokens(&cursor), None);
    }

    #[test]
    fn a_codex_head_segment_shows_where_it_picks_up_until_its_own_first_turn() {
        const ID: &str = "019a0000-0000-7000-8000-000000000001";
        let (state, directory) = crate::test_support::app_state("record-collector-segment");
        state
            .store
            .insert_console(&crate::test_support::console(&directory))
            .unwrap();
        let console_id = state.store.list_consoles().unwrap()[0].id.clone();
        let root = directory.join("account");
        let count = |last: u64| {
            codex_count(json!({"last_token_usage":{"total_tokens":last},
                "model_context_window":475_000}))
                + "\n"
        };
        let meta = |base: Value| {
            json!({"type":"session_meta","payload":{"id":ID,"history_base":base}}).to_string()
                + "\n"
        };
        let prompt = json!({"type":"event_msg","payload":{"type":"user_message","message":"x",
            "images":[],"local_images":[],"text_elements":[]}})
        .to_string()
            + "\n";
        // The base: a turn, then the turn the revert cut off, which sizes nothing.
        let first_meta = meta(Value::Null);
        let kept = format!("{first_meta}{prompt}{}", count(1_000));
        let base = format!("{kept}{prompt}{}", count(9_000));
        let dir = root.join("sessions/2026/10");
        std::fs::create_dir_all(dir.join("01")).unwrap();
        std::fs::create_dir_all(dir.join("02")).unwrap();
        std::fs::write(
            dir.join(format!("01/rollout-2026-10-01T00-00-00-{ID}.jsonl")),
            base,
        )
        .unwrap();
        let head = dir.join(format!(
            "02/rollout-2026-10-02T00-00-00-{ID}_{}.jsonl",
            crate::test_support::CodexChain::HEAD_ID
        ));
        let base_ref =
            json!({"thread_id":ID,"end_ordinal_exclusive":3,"end_byte_offset":kept.len()});
        std::fs::write(&head, meta(base_ref.clone()) + &prompt).unwrap();
        let session: crate::protocol::Session = serde_json::from_value(json!({
            "id":"s","agent":"codex","agent_session_id":ID,"console_id":console_id,
            "host_id":"local","kind":"console","origin":"user","title":"Session",
            "status":"archived","role":"coordinator","has_conversation":true,"pinned":false,"started_at":0,
            "config_dir":root.to_string_lossy()}))
        .unwrap();
        state.store.insert_session(&session).unwrap();
        let mut cursors = Cursors::default();

        collect(&state, &mut cursors);

        let context = state.session_record("s").unwrap().context.unwrap();
        assert_eq!((context.tokens, context.window), (1_000, Some(475_000)));
        assert_eq!(context.used_percent, Some(0));
        // Its own first turn takes over.
        std::fs::write(&head, meta(base_ref) + &prompt + &count(2_000)).unwrap();
        let live = crate::test_support::idle_stand_in("s");
        state.register_live(live.clone());
        collect(&state, &mut cursors);
        assert_eq!(
            state.session_record("s").unwrap().context.unwrap().tokens,
            2_000
        );
    }

    fn codex_count(info: Value) -> String {
        json!({"type":"event_msg","payload":{"type":"token_count","info":info}}).to_string()
    }

    /// A Codex thread of a first rollout and two segments, written by hand: `figures` are the
    /// sizes each of the first two files has before the point the next one continues from (none
    /// for no `token_count`), and the head, with only its opening lines, is returned.
    fn segment_chain(root: &Path, id: &str, figures: [Option<u64>; 2]) -> PathBuf {
        let count = |last: u64| {
            codex_count(json!({"last_token_usage":{"total_tokens":last},
                "model_context_window":475_000}))
                + "\n"
        };
        let meta = |thread: &str, base: Value| {
            json!({"type":"session_meta","payload":{"id":thread,"history_base":base}}).to_string()
                + "\n"
        };
        let prompt = json!({"type":"event_msg","payload":{"type":"user_message","message":"x",
            "images":[],"local_images":[],"text_elements":[]}})
        .to_string()
            + "\n";
        let kept = |meta: String, figure: Option<u64>| {
            format!("{meta}{prompt}{}", figure.map(count).unwrap_or_default())
        };
        let dir = root.join("sessions/2026/10");
        let path = |day: &str, name: String| {
            let file = dir
                .join(day)
                .join(format!("rollout-2026-10-{day}T00-00-00-{name}.jsonl"));
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            file
        };
        // Each file goes on past the cut with a turn the revert took away.
        let first_kept = kept(meta(id, Value::Null), figures[0]);
        std::fs::write(
            path("01", id.to_owned()),
            format!("{first_kept}{prompt}{}", count(9_000)),
        )
        .unwrap();
        let middle_id = crate::test_support::CodexChain::MIDDLE_ID;
        let first_ref = json!({"thread_id":id,"end_ordinal_exclusive":3,
            "end_byte_offset":first_kept.len()});
        let middle_kept = kept(meta(id, first_ref), figures[1]);
        std::fs::write(
            path("02", format!("{id}_{middle_id}")),
            format!("{middle_kept}{prompt}{}", count(9_000)),
        )
        .unwrap();
        let middle_ref = json!({"thread_id":middle_id,"end_ordinal_exclusive":3,
            "end_byte_offset":middle_kept.len()});
        let head = path(
            "03",
            format!("{id}_{}", crate::test_support::CodexChain::HEAD_ID),
        );
        std::fs::write(&head, meta(id, middle_ref) + &prompt).unwrap();
        head
    }

    fn codex_session(state: &AppState, root: &Path, id: &str) {
        let console_id = state.store.list_consoles().unwrap()[0].id.clone();
        let session: crate::protocol::Session = serde_json::from_value(json!({
            "id":"s","agent":"codex","agent_session_id":id,"console_id":console_id,
            "host_id":"local","kind":"console","origin":"user","title":"Session",
            "status":"archived","role":"coordinator","has_conversation":true,"pinned":false,"started_at":0,
            "config_dir":root.to_string_lossy()}))
        .unwrap();
        state.store.insert_session(&session).unwrap();
    }

    #[test]
    fn a_head_segment_reads_the_nearest_earlier_file_that_has_a_figure() {
        const ID: &str = "019a0000-0000-7000-8000-000000000001";
        for (figures, expected) in [
            ([Some(1_000), Some(2_000)], 2_000),
            ([Some(1_000), None], 1_000),
        ] {
            let (state, directory) = crate::test_support::app_state("record-collector-chain-base");
            state
                .store
                .insert_console(&crate::test_support::console(&directory))
                .unwrap();
            let root = directory.join("account");
            segment_chain(&root, ID, figures);
            codex_session(&state, &root, ID);

            collect(&state, &mut Cursors::default());

            let context = state.session_record("s").unwrap().context.unwrap();
            assert_eq!(context.tokens, expected, "{figures:?}");
        }
    }

    /// A lookup that found no chain yet is not remembered as a base without a figure.
    #[test]
    fn a_base_that_was_not_there_yet_is_looked_for_again() {
        const ID: &str = "019a0000-0000-7000-8000-000000000001";
        let (state, directory) = crate::test_support::app_state("record-collector-late-base");
        state
            .store
            .insert_console(&crate::test_support::console(&directory))
            .unwrap();
        let root = directory.join("account");
        // Only the head is on disk at first: its chain names files that are not there.
        let scratch = crate::test_support::ScratchDir::new("record-collector-late-chain");
        let head = segment_chain(&scratch, ID, [Some(1_000), Some(2_000)]);
        let dated = |day: &str| root.join("sessions/2026/10").join(day);
        std::fs::create_dir_all(dated("03")).unwrap();
        let head_here = dated("03").join(head.file_name().unwrap());
        std::fs::copy(&head, &head_here).unwrap();
        codex_session(&state, &root, ID);
        let live = crate::test_support::idle_stand_in("s");
        state.register_live(live.clone());
        let mut cursors = Cursors::default();

        collect(&state, &mut cursors);

        assert!(state.session_record("s").unwrap().context.is_none());
        for day in ["01", "02"] {
            std::fs::create_dir_all(dated(day)).unwrap();
            let from = scratch.join("sessions/2026/10").join(day);
            for file in std::fs::read_dir(from).unwrap() {
                let file = file.unwrap().path();
                std::fs::copy(&file, dated(day).join(file.file_name().unwrap())).unwrap();
            }
        }
        collect(&state, &mut cursors);
        let context = state.session_record("s").unwrap().context.unwrap();
        assert_eq!(context.tokens, 2_000);
    }

    #[test]
    fn groks_window_comes_from_the_signals_file_beside_the_log() {
        let (state, directory) = crate::test_support::app_state("record-collector-grok");
        state
            .store
            .insert_console(&crate::test_support::console(&directory))
            .unwrap();
        let console_id = state.store.list_consoles().unwrap()[0].id.clone();
        let root = directory.join("account");
        let session_dir = root.join("sessions/project/native");
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::write(session_dir.join("usage.json"), "{}").unwrap();
        std::fs::write(
            session_dir.join("updates.jsonl"),
            include_str!("../../testdata/context_grok_1.0.62_updates.jsonl"),
        )
        .unwrap();
        std::fs::write(
            session_dir.join("signals.json"),
            include_str!("../../testdata/context_grok_1.0.62_signals.json"),
        )
        .unwrap();
        let session: crate::protocol::Session = serde_json::from_value(json!({
            "id":"s","agent":"grok","agent_session_id":"native","console_id":console_id,
            "host_id":"local","kind":"console","origin":"user","title":"Session",
            "status":"archived","role":"coordinator","has_conversation":true,"pinned":false,"started_at":0,
            "config_dir":root.to_string_lossy()}))
        .unwrap();
        state.store.insert_session(&session).unwrap();

        collect(&state, &mut Cursors::default());

        assert_eq!(
            state.session_record("s").unwrap().context,
            Some(crate::protocol::SessionContext {
                tokens: 25_500,
                window: Some(256_000),
                used_percent: Some(9),
            })
        );
    }

    /// What a boundary holds is what the collector asks it about: the head of a Codex thread alone,
    /// not the earlier history segments it was cut from.
    #[test]
    fn a_boundary_of_a_codex_thread_holds_only_its_head() {
        const ID: &str = "019a0000-0000-7000-8000-000000000001";
        let (state, directory) = crate::test_support::app_state("model-boundary-head-only");
        state
            .store
            .insert_console(&crate::test_support::console(&directory))
            .unwrap();
        let root = directory.join("account");
        let head = segment_chain(&root, ID, [Some(1_000), Some(2_000)]);
        codex_session(&state, &root, ID);

        let boundary = observed_boundary(&state, &state.session_record("s").unwrap(), None);

        let name = head.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(boundary.files.keys().collect::<Vec<_>>(), [&name]);
        assert_eq!(boundary.files[&name].len(), 1);
    }

    /// Grok's lines never give the observed model, so only its summary is held, and a summary
    /// that the relaunch has not rewritten is still the old process's inside `collect`.
    #[test]
    fn a_grok_summary_at_the_boundary_stays_old_in_the_collector_until_it_is_rewritten() {
        let (state, directory) = crate::test_support::app_state("model-boundary-grok-collect");
        state
            .store
            .insert_console(&crate::test_support::console(&directory))
            .unwrap();
        let console_id = state.store.list_consoles().unwrap()[0].id.clone();
        let root = directory.join("account");
        let session_dir = root.join("sessions/project/native");
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::write(session_dir.join("usage.json"), "{}").unwrap();
        std::fs::write(
            session_dir.join("updates.jsonl"),
            include_str!("../../testdata/context_grok_1.0.62_updates.jsonl"),
        )
        .unwrap();
        let capture = include_str!("../../testdata/titles_grok_1.0.60_summary.json");
        let summary = session_dir.join("summary.json");
        std::fs::write(&summary, capture).unwrap();
        let session: crate::protocol::Session = serde_json::from_value(json!({
            "id":"s","agent":"grok","agent_session_id":"native","console_id":console_id,
            "host_id":"local","kind":"console","origin":"user","title":"Session",
            "status":"idle","role":"coordinator","has_conversation":true,"pinned":false,"started_at":0,
            "config_dir":root.to_string_lossy()}))
        .unwrap();
        state.store.insert_session(&session).unwrap();
        let live = crate::test_support::idle_stand_in("s");
        state.register_live(live.clone());
        let mut cursors = Cursors::default();
        let observed = || state.session_record("s").unwrap().observed_model;

        collect(&state, &mut cursors);
        assert_eq!(observed().as_deref(), Some("grok-4.7"));
        let boundary = observed_boundary(&state, &session, None);
        assert_eq!(
            boundary.files.keys().collect::<Vec<_>>(),
            [&"summary.json".to_string()],
            "neither the usage file nor the log is held"
        );
        state
            .store
            .reset_observed_model("s", &serde_json::to_string(&boundary).unwrap())
            .unwrap();
        collect(&state, &mut cursors);
        assert_eq!(observed(), None, "the summary as it was is the old model");
        collect(&state, &mut Cursors::default());
        assert_eq!(observed(), None, "nor after a daemon restart");

        std::fs::write(&summary, capture.replace("\"grok-4.7\"", "\"grok-4.8\"")).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&summary)
            .unwrap()
            .set_modified(std::time::SystemTime::now() + Duration::from_secs(60))
            .unwrap();
        collect(&state, &mut cursors);
        assert_eq!(observed().as_deref(), Some("grok-4.8"));
    }

    /// A fork forgets what the files beside the record gave along with what its lines gave, and
    /// they are read again though they did not change: the name and the model the summary gives
    /// and the window the window file gives are still known after the record is folded along its
    /// new branch.
    #[test]
    fn a_fork_reads_the_summary_and_the_window_file_again() {
        use crate::protocol::RecordTitles;
        use crate::records::fixtures::{test_record, CLAUDE_ID, TREE_BRANCH, TREE_TRUNK};
        let _declared = crate::providers::test_provider::with_tree_records();
        let dir = crate::test_support::ScratchDir::new("agent-records-collector-fork-files");
        let record = test_record(&dir.join("account"), CLAUDE_ID, TREE_TRUNK);
        std::fs::write(
            record.with_extension("title"),
            "name: Named by the agent\nmodel: test-model\n",
        )
        .unwrap();
        std::fs::write(record.with_file_name("window.json"), r#"{"window":4000}"#).unwrap();
        // One pass of the collector over the session's files.
        let pass = |cursor: &mut Cursor| {
            scan(AgentId::TEST, &record, cursor).unwrap();
            fold_window(AgentId::TEST, &record, cursor);
            fold_summary(AgentId::TEST, &record, cursor);
        };
        let shown = |cursor: &Cursor| {
            let context = cursor
                .context
                .context(provider(AgentId::TEST).records())
                .flatten();
            (
                cursor.titles.merged(None, &RecordTitles::default()).named,
                cursor.observed.model.clone(),
                context.as_ref().map(|context| context.tokens),
                context.and_then(|context| context.window),
            )
        };
        let known = |tokens| {
            (
                Some("Named by the agent".to_owned()),
                Some("test-model".to_owned()),
                Some(tokens),
                Some(4000),
            )
        };

        let mut cursor = Cursor::default();
        pass(&mut cursor);
        assert_eq!(shown(&cursor), known(900));
        // The record forks; neither file beside it changes.
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&record)
            .unwrap();
        std::io::Write::write_all(&mut file, TREE_BRANCH.as_bytes()).unwrap();
        pass(&mut cursor);
        assert_eq!(shown(&cursor), known(200));
    }

    /// A summary file named for its record and written in a form of the provider's own (the
    /// test-only provider's tree-shaped variant: `<id>.title`, a line `name: <title>`) is found
    /// and read through the provider's store; content its reader cannot read is no summary, and is
    /// read again once it changes. A provider with neither answer and no name in its layout has no
    /// summary file.
    #[test]
    fn a_summary_file_is_found_and_read_as_the_providers_store_says() {
        use crate::protocol::RecordTitles;
        use crate::records::fixtures::{test_record, CLAUDE_ID, TREE_TRUNK};
        let dir = crate::test_support::ScratchDir::new("agent-records-collector-summary");
        let record = test_record(&dir.join("account"), CLAUDE_ID, TREE_TRUNK);
        let summary = record.with_extension("title");
        std::fs::write(&summary, "name: Named by the agent\n").unwrap();
        let named = |cursor: &Cursor| cursor.titles.merged(None, &RecordTitles::default()).named;

        // The plain provider: no name in its layout, no answer of its store.
        assert_eq!(summary_path(AgentId::TEST, &record), None);
        let mut plain = Cursor::default();
        fold_summary(AgentId::TEST, &record, &mut plain);
        assert_eq!(named(&plain), None);

        let _declared = crate::providers::test_provider::with_tree_records();
        assert_eq!(summary_path(AgentId::TEST, &record), Some(summary.clone()));
        let mut cursor = Cursor::default();
        fold_summary(AgentId::TEST, &record, &mut cursor);
        assert_eq!(named(&cursor).as_deref(), Some("Named by the agent"));

        // Not in the provider's form: nothing is taken, and the name read before stands.
        std::fs::write(&summary, "{\"name\":\"JSON is not this provider's form\"}").unwrap();
        fold_summary(AgentId::TEST, &record, &mut cursor);
        assert_eq!(named(&cursor).as_deref(), Some("Named by the agent"));
        std::fs::write(&summary, "name: Renamed by the user, later\n").unwrap();
        fold_summary(AgentId::TEST, &record, &mut cursor);
        assert_eq!(
            named(&cursor).as_deref(),
            Some("Renamed by the user, later")
        );

        // A layout's one name still stands where the store names no file: Grok Build's.
        let grok = dir.join("sessions/work").join(CLAUDE_ID).join("usage.json");
        assert_eq!(
            summary_path(AgentId::GROK, &grok),
            Some(grok.with_file_name("summary.json"))
        );
        assert_eq!(summary_path(AgentId::CLAUDE, &record), None);
    }

    /// A record whose lines name what they follow is folded along its current branch: a line that
    /// forks off an earlier one makes the collector fold the record again without what the agent
    /// abandoned.
    #[test]
    fn the_context_of_a_tree_shaped_record_is_its_current_branchs() {
        use crate::records::fixtures::{test_record, CLAUDE_ID, TREE_BRANCH, TREE_TRUNK};
        let _declared = crate::providers::test_provider::with_tree_records();
        let dir = crate::test_support::ScratchDir::new("agent-records-collector-tree");
        let account = dir.join("account");
        let tokens = |cursor: &Cursor| {
            cursor
                .context
                .context(provider(AgentId::TEST).records())
                .flatten()
                .map(|context| context.tokens)
        };
        let append = |record: &Path, lines: &str| {
            use std::io::Write;
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(record)
                .unwrap();
            file.write_all(lines.as_bytes()).unwrap();
        };

        // Read as it grows: the figure is the last request's while each line carries on.
        let record = test_record(&account, CLAUDE_ID, TREE_TRUNK);
        let mut cursor = Cursor::default();
        scan(AgentId::TEST, &record, &mut cursor).unwrap();
        assert_eq!(tokens(&cursor), Some(900));
        // The agent goes back to the first reply: the prompt it carries on with is all the file
        // gains at first, and the figure is already the one the conversation had there, not the
        // abandoned turn's.
        let (prompt, reply) = TREE_BRANCH.split_once('\n').unwrap();
        append(&record, &format!("{prompt}\n"));
        scan(AgentId::TEST, &record, &mut cursor).unwrap();
        assert_eq!(tokens(&cursor), Some(100));
        append(&record, reply);
        scan(AgentId::TEST, &record, &mut cursor).unwrap();
        assert_eq!(tokens(&cursor), Some(200));
        // A line that carries on from there is folded on its own, onto what was read.
        append(
            &record,
            "{\"id\":\"g\",\"parent\":\"f\",\"assistant\":\"more\",\"tokens\":300}\n",
        );
        scan(AgentId::TEST, &record, &mut cursor).unwrap();
        assert_eq!(tokens(&cursor), Some(300));

        // Read whole for the first time, a record that already holds an abandoned turn gives its
        // current branch's figure, whatever order the lines came in.
        let other = "22222222-2222-4222-8222-222222222222";
        let reordered = format!(
            "{}{}",
            TREE_BRANCH, "{\"id\":\"h\",\"parent\":\"e\",\"assistant\":\"again\",\"tokens\":50}\n"
        );
        let record = test_record(&account, other, &format!("{TREE_TRUNK}{reordered}"));
        let mut fresh = Cursor::default();
        scan(AgentId::TEST, &record, &mut fresh).unwrap();
        assert_eq!(tokens(&fresh), Some(50));
    }

    /// The context of a conversation kept as rows is read through the provider while the session
    /// has a process: at first, again only when an event of the session came in, and once more
    /// after the process ends; it is its current branch's. A session with no process is not read
    /// at a daemon's start.
    #[test]
    fn the_context_of_a_conversation_kept_as_rows_is_read_when_the_session_stirs() {
        use crate::adapter::test_provider::row_reads;
        use crate::records::fixtures::{row_database, session, ROW_FORK, ROW_ID, ROW_TURNS};
        let _declared = crate::providers::test_provider::with_row_records();
        let dir = crate::test_support::ScratchDir::new("agent-records-collector-rows");
        let account = dir.join("account");
        let own = session(AgentId::TEST, ROW_ID, Some(&account));
        let hints = crate::records::Hints::default();
        let locator = Locator::with_shell(HashMap::new());
        let tokens = |cursor: &Cursor| {
            cursor
                .context
                .context(provider(AgentId::TEST).records())
                .flatten()
                .map(|context| context.tokens)
        };

        // No process, never read: left alone.
        row_database(&account, &[(ROW_ID, ROW_TURNS)]);
        let mut cursor = Cursor::default();
        let before = row_reads();
        scan_rows(&hints, &locator, &own, false, &mut cursor);
        assert_eq!((row_reads(), tokens(&cursor)), (before, None));

        // With a process: read once, and not again while no event comes in.
        scan_rows(&hints, &locator, &own, true, &mut cursor);
        assert_eq!((row_reads(), tokens(&cursor)), (before + 1, Some(900)));
        let forked = format!("{ROW_TURNS},{ROW_FORK}");
        row_database(&account, &[(ROW_ID, forked.as_str())]);
        scan_rows(&hints, &locator, &own, true, &mut cursor);
        assert_eq!((row_reads(), tokens(&cursor)), (before + 1, Some(900)));

        // An event of the session: read again, along the branch the conversation now ends on.
        hints.stir(&own.id);
        scan_rows(&hints, &locator, &own, true, &mut cursor);
        assert_eq!((row_reads(), tokens(&cursor)), (before + 2, Some(200)));

        // The process has ended: one last reading.
        scan_rows(&hints, &locator, &own, false, &mut cursor);
        assert_eq!(row_reads(), before + 3);
    }

    /// After a change of model, the rows the conversation held when the old process ended give
    /// no model; those written after them do.
    #[test]
    fn rows_held_at_an_observation_boundary_give_no_model() {
        use crate::records::fixtures::{row_database, session, ROW_ID};
        let _declared = crate::providers::test_provider::with_row_records();
        let dir = crate::test_support::ScratchDir::new("agent-records-collector-rows-boundary");
        let account = dir.join("account");
        let own = session(AgentId::TEST, ROW_ID, Some(&account));
        let (state, _directory) = crate::test_support::app_state("record-collector-rows-boundary");
        let hints = crate::records::Hints::default();
        let locator = Locator::with_shell(HashMap::new());
        let old = r#"{"user":"first"},{"assistant":"reply","model":"old-model"}"#;
        row_database(&account, &[(ROW_ID, old)]);

        let boundary = observed_boundary(&state, &own, Some(HashMap::new()));
        assert_eq!(boundary.rows, Some(2));
        let stored = serde_json::to_string(&boundary).unwrap();
        let mut cursor = Cursor {
            observed_boundary: serde_json::from_str(&stored).ok(),
            ..Default::default()
        };
        scan_rows(&hints, &locator, &own, true, &mut cursor);
        assert_eq!(cursor.observed.model, None);

        let grown = format!(r#"{old},{{"assistant":"again","model":"new-model"}}"#);
        row_database(&account, &[(ROW_ID, grown.as_str())]);
        hints.stir(&own.id);
        scan_rows(&hints, &locator, &own, true, &mut cursor);
        assert_eq!(cursor.observed.model.as_deref(), Some("new-model"));
    }

    /// A conversation kept as rows that could not be read when the model changed has a boundary
    /// with no count: the rows its first reading afterwards finds stand for the old process's,
    /// so they give no model, and the ones written after them do.
    #[test]
    fn a_boundary_that_could_not_count_the_rows_is_counted_at_the_next_reading() {
        use crate::records::fixtures::{row_database, session, ROW_ID};
        let _declared = crate::providers::test_provider::with_row_records();
        let dir = crate::test_support::ScratchDir::new("agent-records-collector-rows-unread");
        let account = dir.join("account");
        let own = session(AgentId::TEST, ROW_ID, Some(&account));
        let (state, _directory) = crate::test_support::app_state("record-collector-rows-unread");
        let hints = crate::records::Hints::default();
        let locator = Locator::with_shell(HashMap::new());

        // No database to read at the change.
        let boundary = observed_boundary(&state, &own, Some(HashMap::new()));
        assert_eq!(boundary.rows, None);
        let stored = serde_json::to_string(&boundary).unwrap();
        let mut cursor = Cursor {
            observed_boundary: serde_json::from_str(&stored).ok(),
            ..Default::default()
        };
        // Nor at the first reading: nothing is counted yet.
        scan_rows(&hints, &locator, &own, true, &mut cursor);
        assert_eq!((cursor.rows_held, &cursor.observed.model), (None, &None));

        let old = r#"{"user":"first"},{"assistant":"reply","model":"old-model"}"#;
        row_database(&account, &[(ROW_ID, old)]);
        hints.stir(&own.id);
        scan_rows(&hints, &locator, &own, true, &mut cursor);
        assert_eq!((cursor.rows_held, &cursor.observed.model), (Some(2), &None));

        let grown = format!(r#"{old},{{"assistant":"again","model":"new-model"}}"#);
        row_database(&account, &[(ROW_ID, grown.as_str())]);
        hints.stir(&own.id);
        scan_rows(&hints, &locator, &own, true, &mut cursor);
        assert_eq!(cursor.observed.model.as_deref(), Some("new-model"));
    }

    /// Two model changes in a row whose rows could not be read store boundaries the collector tells
    /// apart: the second is taken up as a new boundary, so the rows the conversation holds when it
    /// is first read again stand for the processes before, and the model the process before last
    /// wrote among them is not shown for the new one.
    #[test]
    fn a_second_boundary_that_could_not_count_the_rows_is_taken_up_too() {
        use crate::records::fixtures::{row_database, ROW_ID};
        let _declared = crate::providers::test_provider::with_row_records();
        let (state, directory) =
            crate::test_support::app_state("record-collector-rows-unread-twice");
        state
            .store
            .insert_console(&crate::test_support::console(&directory))
            .unwrap();
        let account = directory.join("account");
        let mut session = crate::records::fixtures::session(AgentId::TEST, ROW_ID, Some(&account));
        session.console_id = "console-1".to_owned();
        session.kind = crate::protocol::SessionKind::Console;
        session.status = crate::protocol::SessionStatus::Idle;
        state.store.insert_session(&session).unwrap();
        let live = crate::test_support::idle_stand_in_of("s", AgentId::TEST);
        state.register_live(live.clone());
        let mut cursors = Cursors::default();
        let mut pass = || {
            state.record_hints.stir("s");
            collect_with(&state, &mut cursors, Locator::with_shell(HashMap::new()));
        };
        let observed = || state.session_record("s").unwrap().observed_model;
        let change_model = |boundary: &ObservedBoundary| {
            let text = serde_json::to_string(boundary).unwrap();
            assert!(state
                .store
                .set_session_launch_overrides_unless_archived("s", &None, &None, Some(&text))
                .unwrap());
        };
        let database = account.join(crate::adapter::test_provider::ROW_DATABASE);

        // A to B, with no database to read.
        let first = observed_boundary(&state, &session, Some(HashMap::new()));
        assert_eq!(first.rows, None);
        change_model(&first);
        let old = r#"{"user":"first"},{"assistant":"reply","model":"model-a"}"#;
        row_database(&account, &[(ROW_ID, old)]);
        pass();
        assert_eq!(
            observed(),
            None,
            "the rows held at the boundary give no model"
        );
        let by_b = format!(r#"{old},{{"assistant":"again","model":"model-b"}}"#);
        row_database(&account, &[(ROW_ID, by_b.as_str())]);
        pass();
        assert_eq!(observed().as_deref(), Some("model-b"));

        // B to C, the rows unreadable again; then readable, nothing written by C yet.
        std::fs::remove_file(&database).unwrap();
        let second = observed_boundary(&state, &session, Some(HashMap::new()));
        assert_eq!(second.rows, None);
        assert_ne!(
            serde_json::to_string(&first).unwrap(),
            serde_json::to_string(&second).unwrap(),
            "each uncounted boundary is its own text"
        );
        change_model(&second);
        row_database(&account, &[(ROW_ID, by_b.as_str())]);
        pass();
        assert_eq!(observed(), None, "B's model is not shown for C");
    }

    /// A pass over the sessions reads a conversation kept as rows through its provider, never
    /// by looking for a file: once when its session has a process, again only when an event of
    /// the session came in, once more when the process has ended, and not again after that
    /// whatever comes in. What it read reaches the session's record.
    #[test]
    fn a_pass_reads_a_row_sessions_conversation_only_when_it_is_due() {
        use crate::adapter::test_provider::row_reads;
        use crate::records::fixtures::{row_database, ROW_FORK, ROW_ID, ROW_TURNS};
        let _declared = crate::providers::test_provider::with_row_records();
        let (state, directory) = crate::test_support::app_state("record-collector-rows-pass");
        state
            .store
            .insert_console(&crate::test_support::console(&directory))
            .unwrap();
        let account = directory.join("account");
        row_database(&account, &[(ROW_ID, ROW_TURNS)]);
        let mut session = crate::records::fixtures::session(AgentId::TEST, ROW_ID, Some(&account));
        session.console_id = "console-1".to_owned();
        session.kind = crate::protocol::SessionKind::Console;
        session.status = crate::protocol::SessionStatus::Idle;
        state.store.insert_session(&session).unwrap();
        let mut cursors = Cursors::default();
        let mut pass = || collect_with(&state, &mut cursors, Locator::with_shell(HashMap::new()));
        let tokens = || {
            let context = state.session_record("s").unwrap().context;
            context.map(|context| context.tokens)
        };
        let before = row_reads();

        // With a process: read once, and not again while no event comes in.
        let live = crate::test_support::idle_stand_in_of("s", AgentId::TEST);
        state.register_live(live.clone());
        pass();
        assert_eq!((row_reads(), tokens()), (before + 1, Some(900)));
        let forked = format!("{ROW_TURNS},{ROW_FORK}");
        row_database(&account, &[(ROW_ID, forked.as_str())]);
        pass();
        assert_eq!((row_reads(), tokens()), (before + 1, Some(900)));

        // An event of the session: read again.
        state.record_hints.stir("s");
        pass();
        assert_eq!((row_reads(), tokens()), (before + 2, Some(200)));

        // The process has ended: one last reading, and none after it, stirred or not.
        assert!(state.remove_live(&live.clone()));
        pass();
        assert_eq!(row_reads(), before + 3);
        pass();
        state.record_hints.stir("s");
        pass();
        assert_eq!(row_reads(), before + 3);
    }

    #[tokio::test]
    async fn a_slow_row_read_does_not_hold_back_a_later_session() {
        use crate::protocol::{SessionKind, SessionStatus};
        use crate::records::fixtures::{row_database, session, ROW_ID, ROW_TURNS};
        use crate::test_support::{app_state, console, idle_stand_in_of, PATIENCE};

        let _declared = crate::providers::test_provider::with_row_records();
        let (state, dir) = app_state("record-collector-row-concurrency");
        state.store.insert_console(&console(&dir)).unwrap();
        let mut stand_ins = Vec::new();
        for (id, started_at) in [("a-slow", 1), ("b-fast", 2)] {
            let account = dir.join(id);
            row_database(&account, &[(ROW_ID, ROW_TURNS)]);
            let mut each = session(AgentId::TEST, ROW_ID, Some(&account));
            each.id = id.to_owned();
            each.console_id = "console-1".to_owned();
            each.kind = SessionKind::Console;
            each.status = SessionStatus::Idle;
            each.started_at = started_at;
            state.store.insert_session(&each).unwrap();
            let live = idle_stand_in_of(id, AgentId::TEST);
            state.register_live(live.clone());
            stand_ins.push(live);
        }
        let (started, slow_started) = std::sync::mpsc::channel();
        let (release, slow_release) = std::sync::mpsc::channel();
        let slow_release = std::sync::Mutex::new(slow_release);
        let mut cursors = Cursors {
            row_reads: RowReads::with_background(),
            ..Cursors::default()
        };
        cursors.row_reads.before_read = Some(Arc::new(move |id| {
            if id == "a-slow" {
                started.send(()).unwrap();
                let _ = slow_release.lock().unwrap().recv_timeout(PATIENCE);
            }
        }));
        let mut pass = || collect_with(&state, &mut cursors, Locator::with_shell(HashMap::new()));
        let tokens = |id| {
            state
                .session_record(id)
                .unwrap()
                .context
                .map(|context| context.tokens)
        };

        pass();
        slow_started
            .recv_timeout(PATIENCE)
            .expect("slow query started");
        assert!(crate::session::wait::wait_for(PATIENCE, || {
            pass();
            tokens("b-fast") == Some(900)
        }));
        assert_eq!(tokens("a-slow"), None);

        release.send(()).expect("release the slow query");
        assert!(crate::session::wait::wait_for(PATIENCE, || {
            pass();
            tokens("a-slow") == Some(900)
        }));
    }

    /// A file that is still shorter than what the boundary held of it, or cannot be read through
    /// that far, proves nothing: the model is not taken from it for that pass, and what is later
    /// written after the old prefix is.
    #[test]
    fn a_file_that_cannot_yet_be_told_from_the_old_one_gives_no_model() {
        let dir = crate::test_support::ScratchDir::new("model-boundary-unknown");
        let capture = include_str!("../../testdata/observed_codex_0.162.1.jsonl");
        let next = capture.replace("codex-auto-review", "codex-next-review");
        let path = dir.join("rollout.jsonl");
        std::fs::write(&path, capture).unwrap();
        let mut boundary = ObservedBoundary::default();
        boundary.insert(&path);

        // Shorter than the prefix: cut at a line end, so its lines read whole.
        let cut = capture[..capture.len() / 2].rfind('\n').unwrap() + 1;
        std::fs::write(&path, &capture[..cut]).unwrap();
        assert_eq!(boundary.start(&path), None);
        let mut cursor = Cursor {
            observed_boundary: Some(boundary),
            ..Default::default()
        };
        scan(AgentId::CODEX, &path, &mut cursor).unwrap();
        assert!(cursor.observed.model.is_none(), "nothing from a short file");
        assert!(cursor.boundary_start.is_none(), "and nothing is remembered");

        // Grown past the prefix with the old content intact: only what follows it is new.
        std::fs::write(&path, format!("{capture}{next}")).unwrap();
        scan(AgentId::CODEX, &path, &mut cursor).unwrap();
        assert_eq!(cursor.observed.model.as_deref(), Some("codex-next-review"));
        assert_eq!(
            cursor.boundary_start,
            Some((path.clone(), capture.len() as u64))
        );

        // A compressed file whose last frame is incomplete cannot be read through.
        let compressed = dir.join("other.jsonl.zst");
        let bytes = zstd::stream::encode_all(capture.as_bytes(), 1).unwrap();
        std::fs::write(&compressed, &bytes).unwrap();
        let mut boundary = ObservedBoundary::default();
        boundary.insert(&compressed);
        std::fs::write(&compressed, &bytes[..bytes.len() - 4]).unwrap();
        assert_eq!(boundary.start(&compressed), None);
        // Whole again, it is the old file; a different one is new.
        std::fs::write(&compressed, &bytes).unwrap();
        assert_eq!(boundary.start(&compressed), Some(capture.len() as u64));
        std::fs::write(
            &compressed,
            zstd::stream::encode_all(next.as_bytes(), 1).unwrap(),
        )
        .unwrap();
        assert_eq!(boundary.start(&compressed), Some(0));
    }

    /// Stores a session of `each`'s agent that counts as having no conversation, named for the
    /// agent, on `each`'s account and conversation.
    fn insert_unreported(
        state: &AppState,
        each: &crate::records::fixtures::Unreported,
        status: &str,
    ) {
        let console_id = state.store.list_consoles().unwrap()[0].id.clone();
        let id = each.agent.descriptor().id;
        let session: Session = serde_json::from_value(json!({
            "id":id,"agent":id,"agent_session_id":each.agent_id,"console_id":console_id,
            "host_id":"local","kind":"console","origin":"user","title":"Session",
            "status":status,"role":"coordinator","has_conversation":false,"pinned":false,
            "started_at":0,"config_dir":each.account.to_string_lossy()}))
        .unwrap();
        state.store.insert_session(&session).unwrap();
    }

    fn unreported_state(label: &str) -> (Arc<AppState>, crate::test_support::ScratchDir) {
        let (state, directory) = crate::test_support::app_state(label);
        state
            .store
            .insert_console(&crate::test_support::console(&directory))
            .unwrap();
        (state, directory)
    }

    fn used(state: &AppState, each: &crate::records::fixtures::Unreported) -> bool {
        state
            .session_record(each.agent.descriptor().id)
            .unwrap()
            .has_conversation
    }

    /// For each agent, a running session whose hook never reported a prompt counts as having a
    /// conversation one pass after its own record holds a prompt the user sent, and not while the
    /// record holds only what the agent writes with none sent; clients are told.
    #[test]
    fn a_prompt_in_the_record_marks_a_session_no_hook_reported_as_having_a_conversation() {
        let (state, directory) = unreported_state("record-collector-unreported");
        let cases = crate::records::fixtures::unreported(&directory);
        let mut live = Vec::new();
        for each in &cases {
            insert_unreported(&state, each, "idle");
            each.write(false);
            let stand_in =
                crate::test_support::idle_stand_in_of(each.agent.descriptor().id, each.agent);
            state.register_live(stand_in.clone());
            live.push(stand_in);
        }
        let mut cursors = Cursors::default();
        collect(&state, &mut cursors);
        for each in &cases {
            assert!(!used(&state, each), "{:?}: no prompt sent", each.agent);
        }
        let mut events = state.subscribe();
        for each in &cases {
            each.write(true);
        }
        collect(&state, &mut cursors);
        for each in &cases {
            assert!(used(&state, each), "{:?}: the record holds it", each.agent);
        }
        let mut told = Vec::new();
        while let Ok(event) = events.try_recv() {
            if let crate::protocol::Event::SessionUpserted { session } = event {
                if session.has_conversation {
                    told.push(session.id.clone());
                }
            }
        }
        for each in &cases {
            assert!(
                told.iter().any(|id| id == each.agent.descriptor().id),
                "{:?}: clients are told",
                each.agent
            );
        }
    }

    /// A session with no process is read once, and that one pass marks it too.
    #[test]
    fn a_stopped_sessions_one_pass_marks_the_conversation_its_record_holds() {
        let (state, directory) = unreported_state("record-collector-unreported-once");
        let cases = crate::records::fixtures::unreported(&directory);
        for each in &cases {
            insert_unreported(&state, each, "archived");
            each.write(true);
        }
        collect(&state, &mut Cursors::default());
        for each in &cases {
            assert!(used(&state, each), "{:?}", each.agent);
        }
    }

    /// The mark holds only for the conversation that was read: a session that names another one by
    /// now, or none, is left as it is.
    #[test]
    fn a_recorded_conversation_is_not_marked_on_a_session_that_names_another() {
        let (state, directory) = unreported_state("record-collector-unreported-stale");
        let each = crate::records::fixtures::unreported(&directory).remove(0);
        insert_unreported(&state, &each, "idle");
        let id = each.agent.descriptor().id;
        assert!(!state.mark_conversation_recorded(id, "earlier").unwrap());
        assert!(!used(&state, &each));
        assert!(state.mark_conversation_recorded(id, each.agent_id).unwrap());
        assert!(used(&state, &each));
        assert!(
            !state.mark_conversation_recorded(id, each.agent_id).unwrap(),
            "already marked"
        );
    }

    /// While a launch holds a session, the conversation its record shows is not marked: the launch
    /// has read that there is none to resume, and the id it will store is the fresh conversation's.
    /// The session, though it has no process, is read again once the claim is released, and is
    /// marked then if it still names that conversation.
    #[test]
    fn a_recorded_conversation_is_not_marked_while_a_launch_holds_the_session() {
        let (state, directory) = unreported_state("record-collector-unreported-launching");
        let each = crate::records::fixtures::unreported(&directory).remove(0);
        insert_unreported(&state, &each, "interrupted");
        each.write(true);
        let id = each.agent.descriptor().id;
        let mut cursors = Cursors::default();

        let claim = state.begin_launch(id).unwrap();
        collect(&state, &mut cursors);
        assert!(!used(&state, &each), "the launch holds the session");
        assert!(!state.mark_conversation_recorded(id, each.agent_id).unwrap());
        drop(claim);
        collect(&state, &mut cursors);
        assert!(
            used(&state, &each),
            "asked again once the claim is released"
        );

        // A launch that opened a fresh conversation: the mark against the old id never lands.
        state.store.forget_conversation(id).unwrap();
        state.set_agent_session_id(id, each.agent_id).unwrap();
        let mut cursors = Cursors::default();
        let claim = state.begin_launch(id).unwrap();
        collect(&state, &mut cursors);
        state
            .set_agent_session_id(id, "22222222-3333-4444-8555-666666666666")
            .unwrap();
        drop(claim);
        collect(&state, &mut cursors);
        assert!(!used(&state, &each), "the new conversation has no prompt");
    }
}
