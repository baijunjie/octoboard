//! Persisting native per-session token totals for live and archived cards. One collector reads
//! append-only records incrementally; archived sources are read once, and get a final pass after
//! their process exits. No conversation content is retained or published here. Missing usage stays
//! unknown. Cache and reasoning counts are subsets, never added to a native total twice. Where the
//! records live is `agent_records.rs`'s business; this module scans and sums them, and hands each
//! line to `record_titles.rs` and `record_observed.rs` on the way, which keep the titles and the
//! model and effort the records give.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::adapter::adapter_for;
use crate::agent_records::Locator;
use crate::line_file::{Change, LineFile, Signature};
use crate::protocol::{Agent, Session, SessionUsage};
use crate::record_observed::Observed;
use crate::record_titles::{SharedTitles, Titles};
use crate::state::AppState;
use crate::store::RecordUpdate;

#[derive(Default)]
struct Cursor {
    identity: Option<(Agent, Option<String>, Option<String>)>,
    path: Option<PathBuf>,
    file: LineFile,
    messages: HashMap<String, SessionUsage>,
    total: Option<SessionUsage>,
    dormant: bool,
    children: HashMap<PathBuf, Cursor>,
    titles: Titles,
    /// The opening prompt of a Codex thread's first rollout, when the record read is a later
    /// segment, with the path it was read from.
    first_prompt: Option<(PathBuf, Option<String>)>,
    observed: Observed,
    /// The summary file beside the record, which Grok keeps its titles and configured model in.
    summary: SummaryFile,
}

/// A small JSON file read again whenever it changes.
#[derive(Default)]
struct SummaryFile(Option<Signature>);

impl SummaryFile {
    /// What the file holds, when it differs from the last read.
    fn read(&mut self, path: &Path) -> Option<Value> {
        let metadata = std::fs::metadata(path).ok()?;
        let signature = (metadata.len(), metadata.modified().ok());
        if self.0 == Some(signature) || metadata.len() > 8 * 1024 * 1024 {
            return None;
        }
        let summary = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        self.0 = Some(signature);
        Some(summary)
    }
}

#[derive(Default)]
struct Cursors {
    sessions: HashMap<String, Cursor>,
    /// Each account's shared title file, by agent and account directory.
    shared_titles: HashMap<(Agent, PathBuf), SharedTitles>,
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
    let hints = state.record_hints.retain(&ids);
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
        if cursor.dormant && !live {
            continue;
        }
        if cursor.path.is_none() {
            let Some(root) = locator.root(session.agent, session.config_dir.as_deref()) else {
                continue;
            };
            cursor.path = session
                .agent_session_id
                .as_ref()
                .and_then(|id| locator.current_record(session.agent, &root, id));
        }
        refresh_first_prompt(cursor, &mut locator, &session);
        let mut usage = None;
        if let Some(path) = cursor.path.clone() {
            if scan(session.agent, &path, cursor).is_ok() {
                usage = cursor
                    .total
                    .clone()
                    .filter(|total| session.usage.as_ref() != Some(total));
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
        let observed_model = cursor
            .observed
            .model
            .clone()
            .filter(|model| session.observed_model.as_ref() != Some(model));
        let observed_effort = cursor
            .observed
            .effort
            .clone()
            .filter(|effort| session.observed_effort.as_ref() != Some(effort));
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
        if usage.is_some()
            || titles.is_some()
            || observed_model.is_some()
            || observed_effort.is_some()
        {
            updates.push(RecordUpdate {
                session,
                usage,
                titles,
                observed_model,
                observed_effort,
            });
        }
    }
    if !updates.is_empty() {
        if let Ok(changed) = state.store.set_record_data(&updates) {
            for session in changed {
                state.publish_session(&session);
            }
        }
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

fn scan(agent: Agent, path: &Path, cursor: &mut Cursor) -> std::io::Result<()> {
    let change = cursor.file.check(path)?;
    if change == Change::Restarted {
        cursor.messages.clear();
        cursor.total = None;
        cursor.children.clear();
        cursor.titles.clear();
        cursor.observed = Observed::default();
        cursor.summary = SummaryFile::default();
    }
    if change != Change::Unchanged {
        if agent == Agent::Grok {
            // An atomic JSON ledger is small and rewritten as native totals change. Guard its size
            // before reading; malformed/transient writes leave the last valid native total intact.
            if std::fs::metadata(path)?.len() <= 8 * 1024 * 1024 {
                if let Ok(value) = serde_json::from_slice::<Value>(&std::fs::read(path)?) {
                    cursor.total = grok_usage(&value).or_else(|| cursor.total.clone());
                }
            }
            cursor.file.settle();
        } else {
            let adapter = adapter_for(agent);
            cursor.file.read(path, |line| {
                if agent == Agent::Codex {
                    if let Some(usage) = codex_usage(line) {
                        cursor.total = Some(usage);
                    }
                    cursor.observed.fold_codex(line);
                } else if line.contains("\"usage\"") {
                    if let Ok(value) = serde_json::from_str::<Value>(line) {
                        if let Some((id, usage)) = claude_usage(&value) {
                            cursor.messages.insert(id, usage);
                        }
                        cursor.observed.fold_claude(&value);
                    }
                }
                cursor.titles.fold_line(adapter.as_ref(), line);
            })?;
        }
    }
    if agent == Agent::Claude {
        scan_children(path, cursor);
    }
    Ok(())
}

/// Claude stores its internal agents beneath this exact parent session's record. Reading their
/// native usage avoids losing work delegated by the parent. Shared/copied message blocks are
/// deduplicated by native message id across the complete family, not by transcript line uuid.
fn scan_children(path: &Path, cursor: &mut Cursor) {
    let directory = path.with_extension("").join("subagents");
    if let Ok(entries) = std::fs::read_dir(directory) {
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if kind.is_file() && name.starts_with("agent-") && name.ends_with(".jsonl") {
                let path = entry.path();
                let child = cursor.children.entry(path.clone()).or_default();
                // Child files have no nested subagents directory; further descendants are stored
                // alongside them by Claude under the same root session directory.
                let _ = scan(Agent::Claude, &path, child);
            }
        }
    }
    let mut messages: HashMap<&str, &SessionUsage> = HashMap::new();
    for (id, usage) in &cursor.messages {
        messages.insert(id, usage);
    }
    for child in cursor.children.values() {
        for (id, usage) in &child.messages {
            messages.entry(id).or_insert(usage);
        }
    }
    if !messages.is_empty() {
        cursor.total = Some(sum_usage(messages.into_values()));
    }
}

fn number(value: &Value, key: &str) -> Option<u64> {
    value.get(key)?.as_u64()
}

fn codex_usage(line: &str) -> Option<SessionUsage> {
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
    let usage = payload.get("info")?.get("total_token_usage")?;
    Some(SessionUsage {
        input_tokens: number(usage, "input_tokens")?,
        output_tokens: number(usage, "output_tokens")?,
        total_tokens: number(usage, "total_tokens")?,
        cached_input_tokens: number(usage, "cached_input_tokens"),
        cache_write_input_tokens: number(usage, "cache_write_input_tokens"),
    })
}

fn claude_usage(value: &Value) -> Option<(String, SessionUsage)> {
    if value.get("type")?.as_str()? != "assistant" {
        return None;
    }
    let message = value.get("message")?;
    let id = message.get("id")?.as_str()?.to_owned();
    let usage = message.get("usage")?;
    let cached = number(usage, "cache_read_input_tokens");
    let written = number(usage, "cache_creation_input_tokens");
    let input_tokens = number(usage, "input_tokens")?
        .checked_add(cached.unwrap_or(0))?
        .checked_add(written.unwrap_or(0))?;
    let output_tokens = number(usage, "output_tokens")?;
    Some((
        id,
        SessionUsage {
            input_tokens,
            output_tokens,
            total_tokens: input_tokens.checked_add(output_tokens)?,
            cached_input_tokens: cached,
            cache_write_input_tokens: written,
        },
    ))
}

fn grok_usage(value: &Value) -> Option<SessionUsage> {
    let usage = value.get("session")?;
    Some(SessionUsage {
        input_tokens: number(usage, "inputTokens")?,
        output_tokens: number(usage, "outputTokens")?,
        total_tokens: number(usage, "totalTokens")?,
        cached_input_tokens: number(usage, "cachedReadTokens"),
        cache_write_input_tokens: number(usage, "cacheCreationTokens"),
    })
}

fn sum_usage<'a>(values: impl Iterator<Item = &'a SessionUsage>) -> SessionUsage {
    let mut sum = SessionUsage::default();
    for usage in values {
        sum.input_tokens = sum.input_tokens.saturating_add(usage.input_tokens);
        sum.output_tokens = sum.output_tokens.saturating_add(usage.output_tokens);
        sum.total_tokens = sum.total_tokens.saturating_add(usage.total_tokens);
        if let Some(count) = usage.cached_input_tokens {
            sum.cached_input_tokens =
                Some(sum.cached_input_tokens.unwrap_or(0).saturating_add(count));
        }
        if let Some(count) = usage.cache_write_input_tokens {
            sum.cache_write_input_tokens = Some(
                sum.cache_write_input_tokens
                    .unwrap_or(0)
                    .saturating_add(count),
            );
        }
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Captures contain native usage metadata only; their origins and redactions are described
    /// in testdata/usage-captures.md. The old failed-turn capture is the native null near-miss.
    #[test]
    fn committed_native_captures_replay_through_the_usage_readers() {
        let claude = include_str!("../testdata/usage_claude_2.1.296.jsonl");
        let messages: HashMap<_, _> = claude
            .lines()
            .filter_map(|line| claude_usage(&serde_json::from_str(line).ok()?))
            .collect();
        assert_eq!(messages.len(), 2);
        let usage = sum_usage(messages.values());
        assert_eq!(usage.input_tokens, 100_813);
        assert_eq!(usage.output_tokens, 421);
        assert_eq!(usage.total_tokens, 101_234);
        assert_eq!(usage.cached_input_tokens, Some(78_937));
        let codex = include_str!("../testdata/usage_codex_0.162.1.jsonl");
        assert_eq!(codex_usage(codex).unwrap().total_tokens, 18_829);
        assert!(include_str!("../testdata/rollout_running_turn.jsonl")
            .lines()
            .all(|line| codex_usage(line).is_none()));
        let grok: Value =
            serde_json::from_str(include_str!("../testdata/usage_grok_1.0.52.json")).unwrap();
        assert_eq!(grok_usage(&grok).unwrap().total_tokens, 1_191_330);
    }

    #[test]
    fn collector_backfills_archives_publishes_changes_and_refreshes_resumed_sources() {
        let (state, directory) = crate::test_support::app_state("usage-collector");
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
        let before = state.session_record("s").unwrap().usage.unwrap();
        assert!(matches!(
            events.try_recv().unwrap(),
            crate::protocol::Event::SessionUpserted { .. }
        ));
        std::fs::write(&record, capture).unwrap();
        collect(&state, &mut cursors);
        assert_eq!(
            state.session_record("s").unwrap().usage.as_ref(),
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
            state
                .session_record("s")
                .unwrap()
                .usage
                .unwrap()
                .total_tokens,
            101_234
        );
        assert!(matches!(
            events.try_recv().unwrap(),
            crate::protocol::Event::SessionUpserted { .. }
        ));
        collect(&state, &mut cursors);
        assert!(
            events.try_recv().is_err(),
            "unchanged usage emits no extra events"
        );
    }

    #[test]
    fn a_codex_sessions_indexed_name_reaches_the_store_and_the_clients() {
        let (state, directory) = crate::test_support::app_state("usage-collector-title");
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
    /// decompression, so a session idle for a week keeps its usage.
    #[test]
    fn a_compressed_codex_rollout_is_indexed_and_read() {
        const ID: &str = "019a0000-0000-7000-8000-000000000001";
        let (state, directory) = crate::test_support::app_state("usage-collector-compressed");
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
            state
                .session_record("s")
                .unwrap()
                .usage
                .unwrap()
                .total_tokens,
            18_829
        );
    }

    /// A reverted Codex thread's record is its head segment: the usage is the running total the
    /// head ends on, which already counts the segments before it, whichever segment was written
    /// last; the prompt title is the opening one, from the thread's first rollout.
    #[test]
    fn a_reverted_codex_threads_usage_and_titles_come_from_its_head() {
        const ID: &str = "019a0000-0000-7000-8000-000000000001";
        for compressed in [false, true] {
            let (state, directory) = crate::test_support::app_state("usage-collector-chain");
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
                record.usage.unwrap().total_tokens,
                300,
                "compressed: {compressed}"
            );
            assert_eq!(record.record_titles.prompt.as_deref(), Some("first prompt"));
        }
    }

    #[test]
    fn claudes_last_main_transcript_turn_names_the_model_and_effort() {
        let capture = include_str!("../testdata/observed_claude.jsonl");
        let file = crate::test_support::ScratchFile::new("usage-observed", "session.jsonl");
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
        let file = crate::test_support::ScratchFile::new("usage-observed-codex", "rollout.jsonl");
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
        let directory = crate::test_support::ScratchDir::new("usage-observed-grok");
        let path = directory.join("summary.json");
        let capture = include_str!("../testdata/titles_grok_1.0.60_summary.json");
        std::fs::write(&path, capture).unwrap();
        let (mut file, mut observed) = (SummaryFile::default(), Observed::default());
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

    /// A subagent's transcript is scanned for its usage, but nothing of what it shows of the model
    /// and effort reaches the session's own.
    #[test]
    fn a_subagents_turns_do_not_change_the_sessions_model_and_effort() {
        let capture = include_str!("../testdata/observed_claude.jsonl");
        let mut lines = capture.lines();
        let (main, sidechain) = (lines.nth(1).unwrap(), lines.next().unwrap());
        let file = crate::test_support::ScratchFile::new("usage-observed-family", "session.jsonl");
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
        assert_eq!(cursor.children.len(), 1, "the subagent's file was read");
        assert_eq!(cursor.observed.model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(cursor.observed.effort.as_deref(), Some("medium"));
    }

    #[test]
    fn native_totals_do_not_add_cache_or_reasoning_twice() {
        let value = json!({"session":{"inputTokens":100,"outputTokens":20,"totalTokens":120,
            "cachedReadTokens":80,"cacheCreationTokens":0,"reasoningTokens":15}});
        assert_eq!(grok_usage(&value).unwrap().total_tokens, 120);
        let line = json!({"type":"event_msg","payload":{"type":"token_count","info":{
            "total_token_usage":{"input_tokens":100,"output_tokens":20,"total_tokens":120,
                "cached_input_tokens":80,"reasoning_output_tokens":15}}}})
        .to_string();
        assert_eq!(codex_usage(&line).unwrap().total_tokens, 120);
        assert!(codex_usage(
            r#"{"type":"event_msg","payload":{"type":"token_count","info":null}}"#
        )
        .is_none());
    }

    #[test]
    fn owned_claude_subagents_are_included_without_duplicating_shared_messages() {
        let file = crate::test_support::ScratchFile::new("usage-family", "session.jsonl");
        let line = json!({"type":"assistant","message":{"id":"main","usage":{
            "input_tokens":10,"output_tokens":2}}})
        .to_string();
        std::fs::write(&file, format!("{line}\n")).unwrap();
        let directory = file.with_extension("").join("subagents");
        std::fs::create_dir_all(&directory).unwrap();
        let child = directory.join("agent-child.jsonl");
        std::fs::write(
            &child,
            format!("{line}\n{}\n", line.replace("main", "child")),
        )
        .unwrap();
        let mut cursor = Cursor::default();
        scan(Agent::Claude, &file, &mut cursor).unwrap();
        assert_eq!(cursor.total.as_ref().unwrap().total_tokens, 24);
        // The parent's file does not change while its agent continues to work.
        std::fs::write(
            &child,
            format!(
                "{line}\n{}\n{}\n",
                line.replace("main", "child"),
                line.replace("main", "child-2")
            ),
        )
        .unwrap();
        scan(Agent::Claude, &file, &mut cursor).unwrap();
        assert_eq!(cursor.total.as_ref().unwrap().total_tokens, 36);
    }

    #[test]
    fn claude_repeated_message_blocks_count_once_and_incremental_appends_work() {
        let file = crate::test_support::ScratchFile::new("usage-claude", "session.jsonl");
        let line = json!({"type":"assistant","message":{"id":"msg-1","usage":{
            "input_tokens":2,"output_tokens":10,"cache_read_input_tokens":40,
            "cache_creation_input_tokens":8}}})
        .to_string();
        std::fs::write(&file, format!("{line}\n{line}\n")).unwrap();
        let mut cursor = Cursor::default();
        scan(Agent::Claude, &file, &mut cursor).unwrap();
        let usage = cursor.total.as_ref().unwrap();
        assert_eq!(usage.total_tokens, 60);
        assert_eq!(usage.input_tokens, 50);
        let second = line.replace("msg-1", "msg-2");
        std::fs::write(&file, format!("{line}\n{line}\n{second}")).unwrap();
        scan(Agent::Claude, &file, &mut cursor).unwrap();
        assert_eq!(cursor.total.as_ref().unwrap().total_tokens, 60);
        std::fs::write(&file, format!("{line}\n{line}\n{second}\n")).unwrap();
        scan(Agent::Claude, &file, &mut cursor).unwrap();
        assert_eq!(cursor.total.as_ref().unwrap().total_tokens, 120);
        std::fs::write(&file, format!("{second}\n")).unwrap();
        scan(Agent::Claude, &file, &mut cursor).unwrap();
        assert_eq!(cursor.total.as_ref().unwrap().total_tokens, 60);
    }
}
