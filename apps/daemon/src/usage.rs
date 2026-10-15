//! Persisting native per-session token totals for live and archived cards. One collector reads
//! append-only records incrementally; archived sources are read once, and get a final pass after
//! their process exits. No conversation content is retained or published. Missing usage stays
//! unknown. Cache and reasoning counts are subsets, never added to a native total twice.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use serde_json::Value;

use crate::protocol::{Agent, SessionUsage};
use crate::state::AppState;

#[derive(Default)]
pub struct Watch {
    hints: Mutex<HashMap<String, (String, PathBuf)>>,
}

/// The main session's hook provides the exact record path, including a newly created record.
/// Subagent hooks are filtered by the caller before this point.
pub fn on_hook(state: &AppState, id: &str, payload: &Value) {
    let Some(path) = payload.get("transcript_path").and_then(Value::as_str) else {
        return;
    };
    let Some(agent_id) = payload.get("session_id").and_then(Value::as_str) else {
        return;
    };
    state
        .usage_watch
        .hints
        .lock()
        .expect("usage hint lock poisoned")
        .insert(id.to_owned(), (agent_id.to_owned(), PathBuf::from(path)));
}

#[derive(Default)]
struct Cursor {
    identity: Option<(Agent, Option<String>, Option<String>)>,
    path: Option<PathBuf>,
    offset: u64,
    file_identity: Option<(u64, u64)>,
    messages: HashMap<String, SessionUsage>,
    total: Option<SessionUsage>,
    signature: Option<(u64, Option<SystemTime>)>,
    dormant: bool,
    children: HashMap<PathBuf, Cursor>,
}

/// One bounded blocking job owns all cursors, rather than starting a task per historical session.
/// The weak reference lets a test or daemon shutdown release its state without a timer retaining it.
pub fn spawn(state: &Arc<AppState>) {
    let weak = Arc::downgrade(state);
    tokio::spawn(async move {
        let mut cursors = HashMap::new();
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

fn collect(state: &AppState, cursors: &mut HashMap<String, Cursor>) {
    let Ok(sessions) = state.store.list_sessions() else {
        return;
    };
    let ids: HashSet<_> = sessions.iter().map(|session| session.id.as_str()).collect();
    cursors.retain(|id, _| ids.contains(id.as_str()));
    let hints = {
        let mut hints = state
            .usage_watch
            .hints
            .lock()
            .expect("usage hint lock poisoned");
        hints.retain(|id, _| ids.contains(id.as_str()));
        hints.clone()
    };
    // Resolve the default homes using the same login-shell snapshot as agent launches. Failure is
    // unknown, never a fallback to a different account's history.
    let shell = sessions
        .iter()
        .any(|session| session.config_dir.is_none())
        .then(crate::env_shell::cached_snapshot)
        .and_then(Result::ok);
    let mut indexes: HashMap<(Agent, PathBuf), HashMap<String, PathBuf>> = HashMap::new();
    let mut updates = Vec::new();
    for session in sessions {
        let identity = (
            session.agent,
            session.agent_session_id.clone(),
            session.config_dir.clone(),
        );
        let cursor = cursors.entry(session.id.clone()).or_default();
        if cursor.identity.as_ref() != Some(&identity) {
            *cursor = Cursor {
                identity: Some(identity),
                ..Default::default()
            };
        }
        let live = state.live_session(&session.id).is_some();
        if let Some((_, hint)) = hints
            .get(&session.id)
            .filter(|(id, _)| Some(id) == session.agent_session_id.as_ref())
        {
            if session.agent != Agent::Grok && cursor.path.as_ref() != Some(hint) {
                *cursor = Cursor {
                    identity: cursor.identity.clone(),
                    path: Some(hint.clone()),
                    ..Default::default()
                };
            }
        }
        if cursor.dormant && !live {
            continue;
        }
        if cursor.path.is_none() {
            let root = match &session.config_dir {
                Some(dir) => PathBuf::from(dir),
                None => match &shell {
                    Some(shell) => crate::adapter::default_account_dir(session.agent, shell),
                    None => continue,
                },
            };
            let index = indexes
                .entry((session.agent, root.clone()))
                .or_insert_with(|| index_records(session.agent, &root));
            cursor.path = session
                .agent_session_id
                .as_ref()
                .and_then(|id| index.get(id))
                .cloned();
        }
        if let Some(path) = cursor.path.clone() {
            if scan(session.agent, &path, cursor).is_ok() {
                if let Some(total) = &cursor.total {
                    if session.usage.as_ref() != Some(total) {
                        updates.push((session.clone(), total.clone()));
                    }
                }
            }
        }
        cursor.dormant = !live;
    }
    if !updates.is_empty() {
        if let Ok(changed) = state.store.set_session_usages(&updates) {
            for session in changed {
                state.publish_session(&session);
            }
        }
    }
}

/// Index names once per account per pass; never read other sessions' contents or follow links.
fn index_records(agent: Agent, root: &Path) -> HashMap<String, PathBuf> {
    fn walk(agent: Agent, dir: &Path, depth: usize, out: &mut HashMap<String, PathBuf>) {
        if depth == 0 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let id = match agent {
                Agent::Claude if kind.is_file() => name.strip_suffix(".jsonl").map(str::to_owned),
                Agent::Codex if kind.is_file() && name.starts_with("rollout-") => name
                    .strip_suffix(".jsonl")
                    .filter(|name| name.len() >= 36)
                    .and_then(|name| name.get(name.len() - 36..))
                    .map(str::to_owned),
                Agent::Grok if kind.is_dir() && depth == 1 => Some(name),
                _ => None,
            };
            if let Some(id) = id {
                let path = if agent == Agent::Grok {
                    path.join("usage.json")
                } else {
                    path
                };
                out.entry(id)
                    .and_modify(|old| {
                        if modified(&path) > modified(old) {
                            *old = path.clone();
                        }
                    })
                    .or_insert(path);
            } else if kind.is_dir() {
                walk(agent, &path, depth - 1, out);
            }
        }
    }
    let mut out = HashMap::new();
    let (directory, depth) = match agent {
        Agent::Claude => ("projects", 2),
        Agent::Codex => ("sessions", 6),
        Agent::Grok => ("sessions", 2),
    };
    walk(agent, &root.join(directory), depth, &mut out);
    out
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

fn scan(agent: Agent, path: &Path, cursor: &mut Cursor) -> std::io::Result<()> {
    let metadata = std::fs::metadata(path)?;
    let file_identity = (metadata.dev(), metadata.ino());
    if cursor
        .file_identity
        .is_some_and(|previous| previous != file_identity)
    {
        cursor.offset = 0;
        cursor.messages.clear();
        cursor.total = None;
        cursor.signature = None;
        cursor.children.clear();
    }
    cursor.file_identity = Some(file_identity);
    let signature = (metadata.len(), metadata.modified().ok());
    if cursor.signature == Some(signature) {
        if agent == Agent::Claude {
            scan_children(path, cursor);
        }
        return Ok(());
    }
    if agent == Agent::Grok {
        // An atomic JSON ledger is small and rewritten as native totals change. Guard its size
        // before reading; malformed/transient writes leave the last valid native total intact.
        if metadata.len() <= 8 * 1024 * 1024 {
            if let Ok(value) = serde_json::from_slice::<Value>(&std::fs::read(path)?) {
                cursor.total = grok_usage(&value).or_else(|| cursor.total.clone());
            }
        }
    } else {
        if metadata.len() < cursor.offset
            || (metadata.len() == cursor.offset && cursor.signature != Some(signature))
        {
            cursor.offset = 0;
            cursor.messages.clear();
            cursor.total = None;
        }
        let offset = scan_lines(path, cursor.offset, |line| {
            if agent == Agent::Codex {
                if let Some(usage) = codex_usage(line) {
                    cursor.total = Some(usage);
                }
            } else if let Some((id, usage)) = claude_usage(line) {
                cursor.messages.insert(id, usage);
            }
        })?;
        cursor.offset = offset;
        if agent == Agent::Claude {
            scan_children(path, cursor);
        }
    }
    cursor.signature = Some(signature);
    Ok(())
}

/// Read complete lines with bounded memory even when a tool writes a very large body. A record
/// larger than 8 MiB is not a supported usage record and is skipped, never deserialized. The
/// offset still stops before an unfinished line, exactly as `record_watch::scan_lines` does.
fn scan_lines(path: &Path, offset: u64, mut visit: impl FnMut(&str)) -> std::io::Result<u64> {
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut reader = std::io::BufReader::new(file);
    let mut complete = offset;
    let mut position = offset;
    let mut line = Vec::new();
    let mut oversized = false;
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(complete);
        }
        let end = buffer.iter().position(|byte| *byte == b'\n');
        let length = end.map_or(buffer.len(), |end| end + 1);
        if !oversized {
            if line.len() + length <= 8 * 1024 * 1024 {
                line.extend_from_slice(&buffer[..length]);
            } else {
                line.clear();
                oversized = true;
            }
        }
        position += length as u64;
        reader.consume(length);
        if end.is_some() {
            if !oversized {
                if let Ok(line) = std::str::from_utf8(&line) {
                    visit(line);
                }
            }
            complete = position;
            line.clear();
            oversized = false;
        }
    }
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

fn claude_usage(line: &str) -> Option<(String, SessionUsage)> {
    if !line.contains("\"usage\"") {
        return None;
    }
    let value: Value = serde_json::from_str(line).ok()?;
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
        let messages: HashMap<_, _> = claude.lines().filter_map(claude_usage).collect();
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
        let mut cursors = HashMap::new();
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
