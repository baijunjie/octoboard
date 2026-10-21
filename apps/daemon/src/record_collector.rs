//! Persisting what an agent's own records give each session, for live and archived cards: the
//! context size (`record_context.rs`), the titles (`record_titles.rs`) and the model and effort
//! (`record_observed.rs`). One collector reads append-only records incrementally; archived
//! sources are read once, and get a final pass after their process exits. No conversation content
//! is retained or published here. Missing data stays unknown. Where the records live is
//! `agent_records.rs`'s business; this module reads them and hands each line to the three
//! modules above, and asks `default_effort.rs` for the effort a session's agent runs it at where
//! the records give the model alone.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::adapter::adapter_for;
use crate::agent_records::Locator;
use crate::default_effort::DefaultEfforts;
use crate::line_file::{Change, LineFile, Signature, JSON_FILE_LIMIT};
use crate::protocol::{Agent, Session};
use crate::record_context::{codex_figure, Context};
use crate::record_observed::Observed;
use crate::record_titles::{SharedTitles, Titles};
use crate::state::AppState;
use crate::store::RecordUpdate;

#[derive(Default)]
struct Cursor {
    identity: Option<(Agent, Option<String>, Option<String>)>,
    path: Option<PathBuf>,
    file: LineFile,
    context: Context,
    dormant: bool,
    titles: Titles,
    /// The opening prompt of a Codex thread's first rollout, when the record read is a later
    /// segment, with the path it was read from.
    first_prompt: Option<(PathBuf, Option<String>)>,
    observed: Observed,
    /// The summary file beside the record, which Grok keeps its titles and configured model in.
    summary: JsonFile,
    /// The signals file beside the record, which Grok keeps its context window in.
    signals: JsonFile,
}

/// A small JSON file read again whenever it changes.
#[derive(Default)]
struct JsonFile(Option<Signature>);

impl JsonFile {
    /// What the file holds, when it differs from the last read.
    fn read(&mut self, path: &Path) -> Option<Value> {
        let metadata = std::fs::metadata(path).ok()?;
        let signature = (metadata.len(), metadata.modified().ok());
        if self.0 == Some(signature) || metadata.len() > JSON_FILE_LIMIT {
            return None;
        }
        let value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        self.0 = Some(signature);
        Some(value)
    }
}

#[derive(Default)]
struct Cursors {
    sessions: HashMap<String, Cursor>,
    /// Each account's shared title file, by agent and account directory.
    shared_titles: HashMap<(Agent, PathBuf), SharedTitles>,
    default_efforts: DefaultEfforts,
}

/// One bounded blocking job owns all cursors, rather than starting a task per historical session.
/// The weak reference lets a test or daemon shutdown release its state without a timer retaining it.
pub fn spawn(state: &Arc<AppState>) {
    let weak = Arc::downgrade(state);
    tokio::spawn(async move {
        let mut cursors = Cursors::default();
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
    let Ok(sessions) = state.store.list_sessions() else {
        return;
    };
    let ids: HashSet<_> = sessions.iter().map(|session| session.id.as_str()).collect();
    cursors.sessions.retain(|id, _| ids.contains(id.as_str()));
    state.retain_default_efforts(&ids);
    cursors.default_efforts.begin_pass();
    let hints = state.record_hints.retain(&ids);
    let windows = state.context_windows.retain(&ids);
    // Default homes resolve through the same login-shell snapshot as agent launches.
    let mut locator = Locator::default();
    // The sessions each account's shared title file is read for.
    let mut wanted: HashMap<(Agent, PathBuf), HashSet<String>> = HashMap::new();
    let mut shared_keys: HashMap<String, (Agent, PathBuf)> = HashMap::new();
    for session in &sessions {
        let sleeping = cursors
            .sessions
            .get(&session.id)
            .is_some_and(|cursor| cursor.dormant)
            && state.live_session(&session.id).is_none();
        if let (Some(_), Some(agent_id), false) = (
            crate::agent_records::title_files(session.agent).shared,
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
        if let Some(file) = crate::agent_records::title_files(agent).shared {
            cursors
                .shared_titles
                .entry((agent, root.clone()))
                .or_default()
                .refresh(adapter_for(agent).as_ref(), &root.join(file), ids);
        }
    }
    let mut updates = Vec::new();
    // Sessions whose default effort changed, to publish unless their record update does.
    let mut republish = Vec::new();
    for session in sessions {
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
        let live = state.live_session(&session.id).is_some();
        if let Some(hint) = crate::agent_records::hinted_path(&hints, &session) {
            if cursor.path.as_ref() != Some(&hint) {
                *cursor = Cursor {
                    identity: cursor.identity.clone(),
                    path: Some(hint),
                    ..Default::default()
                };
            }
        }
        // A session whose records are not read this pass takes its default effort from the stored
        // record, since an account's catalogue can change after the session stops.
        let stored = |defaults: &mut DefaultEfforts, republish: &mut Vec<Session>| {
            settle_default_effort(
                state,
                defaults,
                &locator,
                &session,
                (
                    session.observed_model.as_deref(),
                    session.observed_effort.as_deref(),
                ),
                republish,
            )
        };
        if cursor.dormant && !live {
            stored(&mut cursors.default_efforts, &mut republish);
            continue;
        }
        if cursor.path.is_none() {
            let Some(root) = locator.root(session.agent, session.config_dir.as_deref()) else {
                stored(&mut cursors.default_efforts, &mut republish);
                continue;
            };
            cursor.path = session
                .agent_session_id
                .as_ref()
                .and_then(|id| locator.current_record(session.agent, &root, id));
        }
        refresh_first_prompt(cursor, &mut locator, &session);
        if let Some(path) = cursor.path.clone() {
            if scan(session.agent, &path, cursor).is_ok() {
                refresh_base(cursor, &mut locator, &session);
            }
            if session.agent == Agent::Grok {
                if let Some(signals) = cursor.signals.read(&path.with_file_name("signals.json")) {
                    cursor
                        .context
                        .set_window(signals.get("contextWindowTokens").and_then(Value::as_u64));
                }
            }
            if let Some(summary) = crate::agent_records::title_files(session.agent).summary {
                if let Some(summary) = cursor.summary.read(&path.with_file_name(summary)) {
                    cursor
                        .titles
                        .fold_summary(adapter_for(session.agent).as_ref(), &summary);
                    cursor.observed.fold_grok(&summary);
                }
            }
        }
        // Claude Code's status line reports the window; its transcript never does. The last report
        // wins, and until there is one the stored window stands, so a daemon restart, a cursor
        // started afresh or a record read again from its start (which forgets the window) keeps
        // it. Hence after the scan.
        if session.agent == Agent::Claude {
            let reported = windows.get(&session.id).copied();
            if let Some(window) = reported.or_else(|| session.context.as_ref()?.window) {
                cursor.context.set_window(Some(window));
            }
        }
        let context = cursor
            .context
            .context(session.agent)
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
        cursor.dormant = !live;
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
            });
        }
    }
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

/// For a Codex record that is a later history segment, which holds only the prompts since the last
/// revert, reads the thread's opening prompt from its first rollout (once per file).
fn refresh_first_prompt(cursor: &mut Cursor, locator: &mut Locator, session: &Session) {
    let (Agent::Codex, Some(id)) = (session.agent, session.agent_session_id.as_deref()) else {
        return;
    };
    if !cursor
        .path
        .as_deref()
        .is_some_and(crate::agent_records::is_segment)
    {
        return;
    }
    let Some(first) = locator
        .root(session.agent, session.config_dir.as_deref())
        .and_then(|root| locator.first_rollout(&root, id))
        .filter(|first| cursor.first_prompt.as_ref().map(|(from, _)| from) != Some(first))
    else {
        return;
    };
    let prompt = Titles::first_prompt_of(adapter_for(session.agent).as_ref(), &first);
    cursor.first_prompt = Some((first, prompt));
}

/// For a Codex record that is a later history segment, which holds only the turns since the last
/// revert and so no figure until its next turn, reads the figure the conversation had where the
/// segment picks up: the last one in the file it continues, before the cut, or failing that in the
/// nearest earlier file of the chain that has one (once per head file).
fn refresh_base(cursor: &mut Cursor, locator: &mut Locator, session: &Session) {
    let (Agent::Codex, Some(id), Some(head)) = (
        session.agent,
        session.agent_session_id.as_deref(),
        cursor.path.clone(),
    ) else {
        return;
    };
    if cursor.context.is_shown()
        || !crate::agent_records::is_segment(&head)
        || cursor.context.has_base_for(&head)
    {
        return;
    }
    // A lookup that failed is tried again next pass; only a base without a figure is final.
    let Some(stretches) = locator
        .root(session.agent, session.config_dir.as_deref())
        .and_then(|root| locator.rollout_stretches(&root, id, &head))
    else {
        return;
    };
    let mut figure = None;
    for stretch in stretches.iter().rev().skip(1) {
        let mut last = None;
        let read = |line: &str| last = codex_figure(line).or(last);
        let _ = crate::line_file::read_to(&stretch.path, stretch.cut.unwrap_or(u64::MAX), read);
        if last.is_some() {
            figure = last;
            break;
        }
    }
    cursor.context.set_base(head, figure);
}

fn scan(agent: Agent, path: &Path, cursor: &mut Cursor) -> std::io::Result<()> {
    // Grok's `usage.json` marks its session directory; the figure is in the updates log beside it.
    let record = match agent {
        Agent::Grok => path.with_file_name("updates.jsonl"),
        Agent::Claude | Agent::Codex => path.to_path_buf(),
    };
    let change = cursor.file.check(&record)?;
    if change == Change::Restarted {
        cursor.context.forget();
        cursor.titles.clear();
        cursor.observed = Observed::default();
        cursor.summary = JsonFile::default();
        cursor.signals = JsonFile::default();
    }
    if change != Change::Unchanged {
        let adapter = adapter_for(agent);
        cursor.file.read(&record, |line| {
            match agent {
                Agent::Codex => {
                    cursor.context.fold_codex(line);
                    cursor.observed.fold_codex(line);
                }
                Agent::Claude => {
                    if line.contains("\"usage\"") || line.contains("compact_boundary") {
                        if let Ok(value) = serde_json::from_str::<Value>(line) {
                            cursor.context.fold_claude(&value);
                            cursor.observed.fold_claude(&value);
                        }
                    }
                }
                Agent::Grok => cursor.context.fold_grok(line),
            }
            if agent != Agent::Grok {
                cursor.titles.fold_line(adapter.as_ref(), line);
            }
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn collector_backfills_archives_publishes_changes_and_refreshes_resumed_sources() {
        let (state, directory) = crate::test_support::app_state("record-collector");
        let console = crate::protocol::Console {
            id: "console".to_owned(),
            name: "Console".to_owned(),
            workdir: directory.join("console").to_string_lossy().into_owned(),
            console_session_agent: Agent::Claude,
            default_agent: Agent::Claude,
            project_session_agent: None,
            claude_project_session_model: None,
            claude_project_session_effort: None,
            codex_project_session_model: None,
            codex_project_session_effort: None,
            grok_project_session_model: None,
            grok_project_session_effort: None,
            claude_account_id: None,
            codex_account_id: None,
            grok_account_id: None,
            claude_console_session_model: None,
            claude_console_session_effort: None,
            codex_console_session_model: None,
            codex_console_session_effort: None,
            grok_console_session_model: None,
            grok_console_session_effort: None,
            icon: None,
            console_session_prompt: None,
            created_at: 0,
        };
        state.store.insert_console(&console).unwrap();
        let root = directory.join("account");
        let record = root.join("projects/project/native.jsonl");
        std::fs::create_dir_all(record.parent().unwrap()).unwrap();
        let session: crate::protocol::Session = serde_json::from_value(json!({
            "id":"s","agent":"claude","agent_session_id":"native","console_id":"console",
            "host_id":"local","role":"console","origin":"user","title":"Session",
            "status":"archived","has_conversation":true,"pinned":false,"started_at":0,
            "config_dir":root.to_string_lossy()}))
        .unwrap();
        state.store.insert_session(&session).unwrap();
        let capture = include_str!("../testdata/usage_claude_2.1.296.jsonl");
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
            "host_id":"local","role":"console","origin":"user","title":"Session",
            "status":"idle","has_conversation":true,"pinned":false,"started_at":0,
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
            "host_id":"local","role":"console","origin":"user","title":"Session",
            "status":"archived","has_conversation":true,"pinned":false,"started_at":0,
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
        let capture = include_str!("../testdata/usage_codex_0.162.1.jsonl");
        std::fs::write(&rollout, zstd::encode_all(capture.as_bytes(), 0).unwrap()).unwrap();
        let session: crate::protocol::Session = serde_json::from_value(json!({
            "id":"s","agent":"codex","agent_session_id":ID,"console_id":console_id,
            "host_id":"local","role":"console","origin":"user","title":"Session",
            "status":"archived","has_conversation":true,"pinned":false,"started_at":0,
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
        let named = include_str!("../testdata/observed_codex_0.162.1.jsonl");
        std::fs::write(&rollout, named).unwrap();
        std::fs::write(
            root.join("models_cache.json"),
            include_str!("../testdata/codex_models_cache_0.162.1.json"),
        )
        .unwrap();
        let session: crate::protocol::Session = serde_json::from_value(json!({
            "id":"s","agent":"codex","agent_session_id":ID,"console_id":console_id,
            "host_id":"local","role":"console","origin":"user","title":"Session",
            "status":"idle","has_conversation":true,"pinned":false,"started_at":0,
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
        let unset = include_str!("../testdata/observed_codex_unset_effort_0.162.1.jsonl");
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
                "host_id":"local","role":"console","origin":"user","title":"Session",
                "status":"archived","has_conversation":true,"pinned":false,"started_at":0,
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

    #[test]
    fn claudes_last_main_transcript_turn_names_the_model_and_effort() {
        let capture = include_str!("../testdata/observed_claude.jsonl");
        let file = crate::test_support::ScratchFile::new("record-observed", "session.jsonl");
        let first = capture.lines().next().unwrap().to_owned() + "\n";
        std::fs::write(&file, &first).unwrap();
        let mut cursor = Cursor::default();
        scan(Agent::Claude, &file, &mut cursor).unwrap();
        assert_eq!(cursor.observed.model.as_deref(), Some("claude-opus-5"));
        assert_eq!(cursor.observed.effort.as_deref(), Some("high"));
        // The later turn wins; the sidechain turn and the synthetic notice after it do not.
        std::fs::write(&file, capture).unwrap();
        scan(Agent::Claude, &file, &mut cursor).unwrap();
        assert_eq!(cursor.observed.model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(cursor.observed.effort.as_deref(), Some("medium"));
        // A record that restarts leaves nothing of the old one behind.
        std::fs::write(&file, r#"{"type":"user"}"#.to_owned() + "\n").unwrap();
        scan(Agent::Claude, &file, &mut cursor).unwrap();
        assert!(cursor.observed.model.is_none() && cursor.observed.effort.is_none());
    }

    #[test]
    fn codexs_last_turn_context_names_the_model_and_effort() {
        let file = crate::test_support::ScratchFile::new("record-observed-codex", "rollout.jsonl");
        std::fs::write(
            &file,
            include_str!("../testdata/observed_codex_0.162.1.jsonl"),
        )
        .unwrap();
        let mut cursor = Cursor::default();
        scan(Agent::Codex, &file, &mut cursor).unwrap();
        assert_eq!(cursor.observed.model.as_deref(), Some("codex-auto-review"));
        assert_eq!(cursor.observed.effort.as_deref(), Some("low"));
    }

    #[test]
    fn groks_summary_names_the_configured_model_and_effort_again_when_it_changes() {
        let directory = crate::test_support::ScratchDir::new("record-observed-grok");
        let path = directory.join("summary.json");
        let capture = include_str!("../testdata/titles_grok_1.0.60_summary.json");
        std::fs::write(&path, capture).unwrap();
        let (mut file, mut observed) = (JsonFile::default(), Observed::default());
        observed.fold_grok(&file.read(&path).unwrap());
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
        observed.fold_grok(&file.read(&path).unwrap());
        assert_eq!(observed.model.as_deref(), Some("grok-4.8-mini"));
        assert_eq!(observed.effort.as_deref(), Some("low"));
    }

    /// A subagent's transcript is not read at all, so nothing of what it shows reaches the session.
    #[test]
    fn a_subagents_turns_do_not_change_the_sessions_context_model_and_effort() {
        let capture = include_str!("../testdata/observed_claude.jsonl");
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
        scan(Agent::Claude, &file, &mut cursor).unwrap();
        assert_eq!(cursor.observed.model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(cursor.observed.effort.as_deref(), Some("medium"));
        assert!(matches!(
            cursor.context.context(Agent::Claude),
            Some(Some(_))
        ));
    }

    fn claude_request(input: u64) -> String {
        json!({"type":"assistant","message":{"model":"claude-opus-5","usage":{
            "input_tokens":input,"output_tokens":1}}})
        .to_string()
    }

    /// The compaction rule itself is `record_context.rs`'s; here is what the file read adds.
    #[test]
    fn an_unfinished_claude_line_waits_and_a_restarted_record_is_forgotten() {
        let file = crate::test_support::ScratchFile::new("record-claude-lines", "session.jsonl");
        let (request, next) = (claude_request(3), claude_request(9));
        std::fs::write(&file, format!("{request}\n{next}")).unwrap();
        let mut cursor = Cursor::default();
        scan(Agent::Claude, &file, &mut cursor).unwrap();
        let tokens = |cursor: &Cursor| {
            cursor
                .context
                .context(Agent::Claude)
                .map(|context| context.unwrap().tokens)
        };
        assert_eq!(tokens(&cursor), Some(3), "an unfinished line waits");
        std::fs::write(&file, format!("{request}\n{next}\n")).unwrap();
        scan(Agent::Claude, &file, &mut cursor).unwrap();
        assert_eq!(tokens(&cursor), Some(9));
        // A record that restarts leaves nothing of the old one behind.
        std::fs::write(&file, "{\"type\":\"user\"}\n").unwrap();
        scan(Agent::Claude, &file, &mut cursor).unwrap();
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
            "host_id":"local","role":"console","origin":"user","title":"Session",
            "status":"archived","has_conversation":true,"pinned":false,"started_at":0,
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
            "host_id":"local","role":"console","origin":"user","title":"Session",
            "status":"archived","has_conversation":true,"pinned":false,"started_at":0,
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
            include_str!("../testdata/context_grok_1.0.62_updates.jsonl"),
        )
        .unwrap();
        std::fs::write(
            session_dir.join("signals.json"),
            include_str!("../testdata/context_grok_1.0.62_signals.json"),
        )
        .unwrap();
        let session: crate::protocol::Session = serde_json::from_value(json!({
            "id":"s","agent":"grok","agent_session_id":"native","console_id":console_id,
            "host_id":"local","role":"console","origin":"user","title":"Session",
            "status":"archived","has_conversation":true,"pinned":false,"started_at":0,
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
}
