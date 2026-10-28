//! The tool with which a session reads what another session's conversation held,
//! `read_session_history`: the user's messages, those Octoboard wrote into the session and the
//! agent's final reply to each, never its tool calls. It reads the same history the application
//! shows, from the agent's own record, and pages it from the most recent entry backwards. Any
//! session may read any session's, as `get_session` does.

use std::sync::Arc;

use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use crate::protocol::{HistoryMessage, Session};
use crate::session_history;
use crate::state::AppState;

use super::args::optional_count;
use super::exec::session_or_caller;

/// How many entries a call returns when `limit` is left out.
const DEFAULT_LIMIT: usize = 20;

/// The most entries a call returns, so that one call never hands back a whole long conversation.
const MAX_LIMIT: usize = 100;

/// A page of `session`'s history (the caller's own when it is left out): the `limit` entries
/// before entry `before` (the end of the history when it is left out), oldest first. Each entry
/// carries `index`, its position in the whole history from 0, so a page's `index` is the next
/// call's `before`; indices hold while entries are only appended, and a `total` smaller than
/// before means the history was rewritten (a Claude Code rewind, a Grok compaction). The last
/// agent entry of a turn still open in a running session can be a progress note, not yet a final
/// reply. `older` counts the entries before the page and `total` those there are. A record that
/// cannot be read answers `unavailable: true` with nothing in it.
pub(super) async fn read_session_history(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let limit = optional_count(arguments, "limit", 1, MAX_LIMIT)?.unwrap_or(DEFAULT_LIMIT);
    let before = optional_count(arguments, "before", 0, usize::MAX)?;
    // Left out, the caller itself, as for `get_session`.
    let session = session_or_caller(state, caller, arguments)?;
    let messages = session_history::of_session_blocking(state, session.clone()).await?;
    let Some(messages) = messages else {
        return Ok(json!({
            "session": session.id,
            "unavailable": true,
            "total": 0,
            "older": 0,
            "entries": [],
        }));
    };
    let (start, entries) = page(&messages, limit, before)?;
    Ok(json!({
        "session": session.id,
        "unavailable": false,
        "total": messages.len(),
        "older": start,
        "entries": entries,
    }))
}

/// The entries `limit` back from `before` (the end when `None`), oldest first, with the index of
/// the first. A `before` past the end is refused: it names an entry that is not there.
fn page(
    messages: &[HistoryMessage],
    limit: usize,
    before: Option<usize>,
) -> Result<(usize, Vec<Value>)> {
    let end = before.unwrap_or(messages.len());
    if end > messages.len() {
        bail!(
            "`before` is {end}, but the history has {} entries; it is an entry's `index`, or left \
             out for the latest",
            messages.len()
        );
    }
    let start = end.saturating_sub(limit);
    let entries = messages[start..end]
        .iter()
        .enumerate()
        .map(|(offset, message)| {
            json!({
                "index": start + offset,
                "speaker": message.speaker,
                "text": message.text,
                "turn": message.turn,
                "at": message.at,
            })
        })
        .collect();
    Ok((start, entries))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::exec::call;
    use crate::mcp::exec::tests::{arguments, shared_console_state};
    use crate::protocol::{Account, Agent, HistorySpeaker};

    fn messages(count: usize) -> Vec<HistoryMessage> {
        (0..count)
            .map(|at| HistoryMessage {
                speaker: if at % 2 == 0 {
                    HistorySpeaker::User
                } else {
                    HistorySpeaker::Agent
                },
                text: format!("message {at}"),
                turn: (at / 2) as u32 + 1,
                at: (at != 3).then_some(1_000 + at as i64),
            })
            .collect()
    }

    fn indices(entries: &[Value]) -> Vec<u64> {
        entries
            .iter()
            .map(|entry| entry["index"].as_u64().unwrap())
            .collect()
    }

    /// A page is the entries just before `before`, oldest first, each with its place in the whole
    /// history; the latest page is the one with no `before`.
    #[test]
    fn a_page_counts_back_from_the_end_or_from_before() {
        let history = messages(7);
        let (start, latest) = page(&history, 3, None).unwrap();
        assert_eq!((start, indices(&latest)), (4, vec![4, 5, 6]));
        assert_eq!(latest[0]["speaker"], "user");
        assert_eq!(latest[1]["text"], "message 5");
        assert_eq!(latest[1]["turn"], 3);
        assert_eq!(latest[1]["at"], 1_005);

        let (start, earlier) = page(&history, 3, Some(4)).unwrap();
        assert_eq!((start, indices(&earlier)), (1, vec![1, 2, 3]));
        // An entry with no time says so.
        assert!(earlier[2]["at"].is_null());

        let (start, oldest) = page(&history, 3, Some(1)).unwrap();
        assert_eq!((start, indices(&oldest)), (0, vec![0]));
        assert_eq!(page(&history, 3, Some(0)).unwrap(), (0, Vec::new()));
        assert_eq!(indices(&page(&history, 100, None).unwrap().1).len(), 7);
        assert!(page(&history, 3, Some(8)).is_err());
        assert_eq!(page(&[], 3, None).unwrap(), (0, Vec::new()));
    }

    async fn read(state: &Arc<AppState>, caller: &str, input: Value) -> Result<Value> {
        call(state, caller, "read_session_history", &arguments(input)).await
    }

    /// A session that has had no conversation has an empty history, and one whose record cannot
    /// be found says so rather than answering empty.
    #[tokio::test]
    async fn a_history_that_is_empty_differs_from_one_that_cannot_be_read() {
        let (state, _dir) = shared_console_state("read-session-history");
        let empty = read(&state, "a", json!({ "session": "of-b" }))
            .await
            .unwrap();
        assert_eq!(empty["session"], "of-b");
        assert_eq!(empty["unavailable"], false);
        assert_eq!(empty["total"], 0);
        assert_eq!(empty["entries"], json!([]));

        let mut unreadable = state.session_record("of-b").unwrap();
        unreadable.has_conversation = true;
        unreadable.agent_session_id = None;
        state.save_session(&unreadable).unwrap();
        for caller in ["a", "loose"] {
            let lost = read(&state, caller, json!({ "session": "of-b" }))
                .await
                .unwrap();
            assert_eq!(lost["unavailable"], true, "{caller}");
            assert_eq!(lost["entries"], json!([]), "{caller}");
        }
    }

    /// With `session` left out, a session reads its own history, as it reads its own record.
    #[tokio::test]
    async fn a_session_reads_its_own_history_by_leaving_the_session_out() {
        let (state, _dir) = shared_console_state("read-session-history-own");
        for caller in ["a", "of-b"] {
            let own = read(&state, caller, json!({})).await.unwrap();
            assert_eq!(own["session"], caller);
            assert_eq!(own["unavailable"], false);
        }
        let other = read(&state, "a", json!({ "session": "of-b" }))
            .await
            .unwrap();
        assert_eq!(other["session"], "of-b");
    }

    /// A history is read from the agent's own record and paged from the most recent entry back.
    #[tokio::test]
    async fn the_history_of_a_session_is_read_from_its_record_in_pages() {
        let (state, dir) = shared_console_state("read-session-history-record");
        let thread = "019a0000-0000-7000-8000-000000000001";
        let records = dir.join("codex").join("sessions/2026/10/25");
        std::fs::create_dir_all(&records).unwrap();
        std::fs::copy(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("testdata")
                .join("history_codex_rollout.jsonl"),
            records.join(format!("rollout-2026-10-25T00-00-00-{thread}.jsonl")),
        )
        .unwrap();
        let mut session = state.session_record("of-b").unwrap();
        session.agent = Agent::Codex;
        session.has_conversation = true;
        session.agent_session_id = Some(thread.to_string());
        state.save_session(&session).unwrap();
        // The record is looked for in the account directories of the session's agent.
        state
            .store
            .insert_account(&Account {
                id: "codex-work".to_string(),
                agent: Agent::Codex,
                name: "Work".to_string(),
                config_dir: dir.join("codex").to_string_lossy().into_owned(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
            })
            .unwrap();

        let all = read(&state, "a", json!({ "session": "of-b" }))
            .await
            .unwrap();
        assert_eq!(all["unavailable"], false);
        let total = all["total"].as_u64().unwrap();
        assert!(total >= 3, "{all}");
        assert_eq!(all["older"], 0);
        assert_eq!(all["entries"][0]["index"], 0);
        assert_eq!(all["entries"][0]["speaker"], "user");
        assert_eq!(all["entries"][0]["text"], "Fix the flaky test");

        let last = read(&state, "loose", json!({ "session": "of-b", "limit": 1 }))
            .await
            .unwrap();
        assert_eq!(last["older"], total - 1);
        assert_eq!(last["entries"][0]["index"], total - 1);
        assert_eq!(last["entries"][0], all["entries"][total as usize - 1]);

        let before = read(
            &state,
            "a",
            json!({ "session": "of-b", "limit": 1, "before": total - 1 }),
        )
        .await
        .unwrap();
        assert_eq!(before["entries"][0]["index"], total - 2);
        assert_eq!(before["total"], total);
    }

    /// A session that does not exist, and a `limit` or `before` out of range or not a whole
    /// number, are refused.
    #[tokio::test]
    async fn an_unknown_session_and_a_bad_page_are_refused() {
        let (state, _dir) = shared_console_state("read-session-history-refused");
        for (input, reason) in [
            (json!({ "session": "absent" }), "there is no session"),
            (
                json!({ "session": "of-a", "limit": 0 }),
                "`limit` must be a whole number from 1 to 100",
            ),
            (
                json!({ "session": "of-a", "limit": 101 }),
                "`limit` must be a whole number from 1 to 100",
            ),
            (
                json!({ "session": "of-a", "limit": "5" }),
                "`limit` must be a whole number",
            ),
            (
                json!({ "session": "of-a", "limit": 1.5 }),
                "`limit` must be a whole number",
            ),
            (
                json!({ "session": "of-a", "before": -1 }),
                "`before` must be a whole number, 0 or more",
            ),
            (
                json!({ "session": "of-a", "before": 1 }),
                "the history has 0 entries",
            ),
        ] {
            let err = read(&state, "a", input.clone()).await.expect_err("refused");
            assert!(err.to_string().contains(reason), "{input}: {err}");
        }
    }
}
