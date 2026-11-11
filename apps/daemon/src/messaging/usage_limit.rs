//! Holding what Octoboard writes into a session once a turn of it has stopped on its account's
//! usage limit, and letting it go again. Every message written then would open another turn that
//! fails the same way, so owner instructions, shared information, reports, notices and report panel
//! submissions are queued instead; the user's own message from a client goes past the hold, being
//! their deliberate act. The hold's state is the process's own (`crate::session::usage_limit`), so a
//! restart, a switch, a handoff or a resume leaves it behind. It is armed by the same reading of
//! the conversation record as the automatic account switch, whether or not that switch is on, and
//! lives in memory only. For the rules see docs/product/messages-held-after-a-usage-limit.md.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;

use crate::protocol::AgentId;
use crate::records::resumability::record_needs_shell_env;
use crate::records::switch_continue;
use crate::state::AppState;

/// What a turn of the session `id` that closed amounted to for the hold, `failed` saying whether it
/// ended in an error: a failure is judged for whether the usage limit stopped it, and any other end
/// of a turn started while the session was held lets the hold go. Called with the turn closed, and
/// before anything that end releases can write into the session.
pub(crate) fn turn_closed(state: &Arc<AppState>, id: &str, failed: bool) {
    if failed {
        if let Some(live) = state.live_session(id) {
            judge(state, id, &live);
        }
    } else {
        turn_ran(state, id);
    }
}

/// A turn of the session `id` failed on something other than the usage limit, as its agent made
/// plain (a model at capacity): a turn started while held has run, so the hold goes.
pub(crate) fn turn_failed_otherwise(state: &Arc<AppState>, id: &str) {
    turn_ran(state, id);
}

/// A turn of the session `id` ran, whatever it came to, without stopping on the usage limit: one
/// started while held lets the hold go, and what it held is written.
fn turn_ran(state: &Arc<AppState>, id: &str) {
    if state
        .live_session(id)
        .is_some_and(|live| live.usage_limit.turn_ended())
    {
        state.retry_pending_outbox(id);
    }
}

/// Holds the writes into `live`, the process of the session `id`, while its conversation record is
/// read for whether the turn that just failed stopped on the usage limit, and keeps holding them if
/// it did. The record is read off the caller's path; whatever it says, the queue is retried once
/// it has.
fn judge(state: &Arc<AppState>, id: &str, live: &Arc<crate::session::LiveSession>) {
    let since = crate::time::now_millis();
    let judgement = live.usage_limit.begin_judging(since);
    let Ok(session) = state.session_record(id) else {
        live.usage_limit.judged(judgement, false);
        return;
    };
    let (state, id, live) = (state.clone(), id.to_string(), live.clone());
    tokio::spawn(async move {
        let stopped = stopped_on_usage_limit(&session)
            .await
            .unwrap_or_else(|err| {
                tracing::debug!(
                    session = %id,
                    err = %format!("{err:#}"),
                    "reading whether a failed turn stopped on the usage limit failed"
                );
                false
            });
        live.usage_limit.judged(judgement, stopped);
        state.retry_pending_outbox(&id);
    });
}

/// Whether the conversation record of `session` says its last turn stopped on the usage limit, by
/// the reading the automatic account switch makes, the shell's environment taken first where that
/// reading needs it.
async fn stopped_on_usage_limit(session: &crate::protocol::Session) -> Result<bool> {
    let shell_env = if record_needs_shell_env(session) {
        Some(tokio::task::spawn_blocking(judging_shell).await??)
    } else {
        None
    };
    switch_continue::wait_for_usage_limit(session, shell_env).await
}

/// The environment the default account's directory is resolved from: the user's login shell, which
/// no test runs (`crate::env_shell`), so a test's default account is never judged as limited.
#[cfg(not(test))]
fn judging_shell() -> Result<HashMap<String, String>> {
    crate::env_shell::launch_snapshot()
}

#[cfg(test)]
fn judging_shell() -> Result<HashMap<String, String>> {
    anyhow::bail!("no test reads the user's login shell")
}

/// What a successful reading of the `agent` account `account` (kept in `config_dir`), begun at
/// `started_at`, does to the holds: once `switching`, the automatic account switch's own
/// reconsideration of the same reading, has run, every session still held on that account whose
/// hold was armed before the reading began is let go, if the reading shows the account recovered by
/// the switch's own rule (docs/product/automatic-account-switch.md's "When no account qualifies").
/// Waiting on the switch first keeps the two from acting on one session at once: a session the
/// switch restarted is no longer held, and its relaunch takes what was queued. The task that does
/// both, or `switching` alone when nothing is held.
pub(crate) fn after_quota_reading(
    state: &Arc<AppState>,
    agent: AgentId,
    account: Option<String>,
    config_dir: Option<String>,
    started_at: i64,
    switching: Option<tokio::task::JoinHandle<()>>,
) -> Option<tokio::task::JoinHandle<()>> {
    // When the hold of `live` was armed, if it is one of this agent's armed before the reading.
    let armed_before = move |live: &crate::session::LiveSession| {
        live.usage_limit
            .armed_since()
            .filter(|since| live.agent == agent && *since < started_at)
    };
    if !state
        .live_sessions()
        .iter()
        .any(|live| armed_before(live).is_some())
    {
        return switching;
    }
    let state = state.clone();
    Some(tokio::spawn(async move {
        if let Some(switching) = switching {
            let _ = switching.await;
        }
        for live in state.live_sessions() {
            let Some(since) = armed_before(&live) else {
                continue;
            };
            let on_account = state
                .session_record(&live.id)
                .is_ok_and(|session| session.account_id == account);
            if on_account
                && state.account_quotas.verified_available(
                    agent,
                    account.as_deref(),
                    config_dir.as_deref(),
                    since + 1,
                )
                && live.usage_limit.release_armed_at(since)
            {
                state.retry_pending_outbox(&live.id);
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::TurnEnd;
    use crate::messaging::reporting::close_turn_and_report;
    use crate::protocol::{Account, AccountQuota, QuotaWindow, QuotaWindowKind, SessionStatus};
    use crate::session::LiveSession;
    use crate::state::test_support::insert_record;
    use crate::test_support::{
        app_state, idle_stand_in, runtime_settled, within, AcceptingServer, ScratchDir, StandIn,
        PATIENCE,
    };
    use crate::time::now_millis;
    use std::time::Duration;

    const AGENT_SESSION: &str = "a0000000-0000-4000-8000-000000000001";

    /// A Claude Code session `s` on the account `claude-a`, with a conversation, and a process
    /// whose own server takes whatever is written into it, so what was delivered can be read.
    fn session(label: &str) -> (Arc<AppState>, ScratchDir, StandIn, Arc<AcceptingServer>) {
        let (state, dir) = app_state(label);
        insert_record(&state, &dir, "s", SessionStatus::Idle);
        let account_dir = dir.join("claude-a");
        state
            .store
            .insert_account(&Account {
                id: "claude-a".to_string(),
                agent: AgentId::CLAUDE,
                name: "Work A".to_string(),
                config_dir: account_dir.to_string_lossy().into_owned(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
                versions: Default::default(),
            })
            .unwrap();
        state
            .set_session_account("s", Some("claude-a"), account_dir.to_str())
            .unwrap();
        state.set_agent_session_id("s", AGENT_SESSION).unwrap();
        state.mark_conversation_started("s").unwrap();
        let server = AcceptingServer::new();
        let live = idle_stand_in("s").with_transport(server.clone());
        state.register_live(live.clone());
        (state, dir, live, server)
    }

    /// Writes the conversation of [`session`], ending on the usage limit as Claude Code writes it,
    /// or on an ordinary answer.
    fn end_conversation(dir: &ScratchDir, on_limit: bool) {
        let path = dir
            .join("claude-a")
            .join(format!("projects/slug/{AGENT_SESSION}.jsonl"));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let last = if on_limit {
            concat!(
                r#"{"type":"assistant","isApiErrorMessage":true,"error":"rate_limit","#,
                r#""apiError":"usage_limit_reached","message":{"role":"assistant","#,
                r#""stop_reason":"stop_sequence","content":[{"type":"text","text":"limit"}]}}"#
            )
        } else {
            concat!(
                r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"end_turn","#,
                r#""content":[{"type":"text","text":"done"}]}}"#
            )
        };
        let user = r#"{"type":"user","message":{"role":"user","content":"hello"}}"#;
        std::fs::write(path, format!("{user}\n{last}\n")).unwrap();
    }

    /// Arms the hold as a failed turn the record said the usage limit stopped does, at `since`.
    fn arm(live: &LiveSession, since: i64) {
        let judgement = live.usage_limit.begin_judging(since);
        live.usage_limit.judged(judgement, true);
    }

    /// Waits until the verdict on a failed turn is in: the hold is either let go or confirmed.
    async fn judged(live: &Arc<LiveSession>) {
        let live = live.clone();
        assert!(
            within(PATIENCE, move || {
                !live.usage_limit.holds() || live.usage_limit.confirmed()
            })
            .await,
            "the failed turn was never judged"
        );
    }

    /// Waits until the server has been offered `count` messages.
    async fn offered(server: &Arc<AcceptingServer>, count: usize) -> Vec<String> {
        let waiting = server.clone();
        assert!(
            within(PATIENCE, move || waiting.offered().len() >= count).await,
            "only {:?} was delivered",
            server.offered()
        );
        server.offered()
    }

    fn reading_of(windows: Vec<(QuotaWindowKind, f64)>) -> AccountQuota {
        AccountQuota {
            reset_credits: None,
            agent: AgentId::CLAUDE,
            account: Some("claude-a".to_string()),
            fetched_at: 1,
            plan: None,
            email: None,
            windows: windows
                .into_iter()
                .map(|(kind, used_percent)| QuotaWindow {
                    kind,
                    label: (kind == QuotaWindowKind::WeeklyModel).then(|| "Fable".to_string()),
                    used_percent,
                    resets_at: Some(now_millis() + 86_400_000),
                    window_minutes: None,
                })
                .collect(),
            unavailable: None,
        }
    }

    fn reading(used: f64) -> AccountQuota {
        reading_of(vec![(QuotaWindowKind::Weekly, used)])
    }

    fn failed() -> TurnEnd {
        TurnEnd {
            last_assistant_message: Some("API Error".to_string()),
            failed: true,
            backstop: false,
        }
    }

    fn finished() -> TurnEnd {
        TurnEnd {
            last_assistant_message: Some("done".to_string()),
            failed: false,
            backstop: false,
        }
    }

    /// A failed turn holds Octoboard's writes while its record is read, and goes on holding them,
    /// reported to owners, once the record says the usage limit stopped it. Messages queued
    /// meanwhile are not written, whether or not the automatic account switch is on.
    #[tokio::test]
    async fn a_turn_the_usage_limit_stopped_holds_the_writes() {
        let (state, dir, live, server) = session("usage-limit-armed");
        assert!(
            !state.store.auto_switch_on_usage_limit().unwrap(),
            "the setting is off"
        );
        end_conversation(&dir, true);
        turn_closed(&state, "s", true);
        assert!(live.usage_limit.holds(), "held while it is judged");
        assert_eq!(
            state.write_hold("s"),
            None,
            "not reported before the verdict"
        );
        state.queue_message("s", "from the owner");
        judged(&live).await;
        assert!(live.usage_limit.confirmed());
        assert_eq!(
            state.write_hold("s"),
            Some(crate::state::WriteHold::UsageLimit)
        );
        state.queue_message("s", "shared information");
        state.retry_pending_outbox("s");
        runtime_settled().await;
        assert!(server.offered().is_empty(), "{:?}", server.offered());
        assert_eq!(state.outbox.len("s"), 2);
        assert_eq!(
            state.session_record("s").unwrap().status,
            SessionStatus::Idle,
            "the status is left as it was"
        );
    }

    /// Any other failure lets the writes go once the record has been read, and what was queued
    /// meanwhile is written, oldest first.
    #[tokio::test]
    async fn a_turn_that_failed_otherwise_lets_the_writes_go() {
        switch_continue::USAGE_LIMIT_RECORD_WAIT_OVERRIDE.set(Some(Duration::from_millis(100)));
        let (state, dir, live, server) = session("usage-limit-other-failure");
        end_conversation(&dir, false);
        turn_closed(&state, "s", true);
        assert!(live.usage_limit.holds());
        state.queue_message("s", "one");
        state.queue_message("s", "two");
        judged(&live).await;
        switch_continue::USAGE_LIMIT_RECORD_WAIT_OVERRIDE.set(None);
        assert!(!live.usage_limit.holds());
        assert_eq!(state.write_hold("s"), None);
        assert_eq!(offered(&server, 2).await, ["one", "two"]);
    }

    /// A record that cannot be read for the verdict, or a session with no conversation, is not
    /// held.
    #[tokio::test]
    async fn a_failure_whose_record_cannot_be_read_lets_the_writes_go() {
        let (state, _dir, live, server) = session("usage-limit-no-record");
        turn_closed(&state, "s", true);
        state.queue_message("s", "one");
        judged(&live).await;
        assert!(!live.usage_limit.holds());
        assert_eq!(state.write_hold("s"), None);
        assert_eq!(offered(&server, 1).await, ["one"]);
    }

    /// The end of a turn the agent reports as failed holds the writes from the hook's own path,
    /// whether the automatic account switch is on or off; an end of a turn that was not open is
    /// no failed turn of the session.
    #[tokio::test]
    async fn a_failed_turn_end_arms_the_hold_whatever_the_automatic_switch_setting() {
        for switching in [false, true] {
            let (state, dir, live, _server) = session(&format!("usage-limit-setting-{switching}"));
            state
                .store
                .set_auto_switch_on_usage_limit(switching)
                .unwrap();
            end_conversation(&dir, true);
            state.turn_started("s", Some("work"));
            close_turn_and_report(&state, "s", failed());
            judged(&live).await;
            assert!(live.usage_limit.confirmed(), "switch on: {switching}");
        }

        let (state, dir, live, _server) = session("usage-limit-no-turn");
        end_conversation(&dir, true);
        close_turn_and_report(&state, "s", failed());
        runtime_settled().await;
        assert!(!live.usage_limit.holds(), "no turn was open");
    }

    /// A turn that ends without failing, while nothing is held, holds nothing.
    #[tokio::test]
    async fn a_turn_that_ends_well_holds_nothing() {
        let (state, _dir, live, _server) = session("usage-limit-ends-well");
        state.turn_started("s", Some("work"));
        close_turn_and_report(&state, "s", finished());
        assert!(!live.usage_limit.holds());
        assert_eq!(state.write_hold("s"), None);
    }

    /// A turn started while held decides the hold by how it ends: a turn that ends without failing
    /// lets it go and writes what waited, one that fails on the limit again arms it again.
    #[tokio::test]
    async fn a_turn_started_while_held_decides_by_how_it_ends() {
        let (state, dir, live, server) = session("usage-limit-probe");
        end_conversation(&dir, true);
        turn_closed(&state, "s", true);
        judged(&live).await;
        state.queue_message("s", "waits");
        state.turn_started("s", Some("typed in the terminal"));
        assert!(live.usage_limit.holds(), "held until the turn ends");
        assert!(live.usage_limit.confirmed());
        turn_closed(&state, "s", true);
        judged(&live).await;
        assert!(live.usage_limit.confirmed(), "armed again");
        state.turn_started("s", Some("typed again"));
        assert!(live.usage_limit.holds(), "not judged at its start");
        turn_closed(&state, "s", false);
        assert!(!live.usage_limit.holds());
        assert_eq!(offered(&server, 1).await, ["waits"]);
    }

    /// A turn the user cancels while held ran, so the hold goes when the agent reports the cancel;
    /// a cancel only the title showed leaves the hold to the turn's real end, since the agent may
    /// still report the turn as stopped on the limit.
    #[tokio::test]
    async fn a_cancelled_turn_started_while_held_lets_the_hold_go() {
        let (state, dir, live, _server) = session("usage-limit-cancel");
        end_conversation(&dir, true);
        turn_closed(&state, "s", true);
        judged(&live).await;

        state.turn_started("s", Some("typed"));
        assert!(live.usage_limit.holds());
        state.abandon_turn("s");
        assert!(!live.usage_limit.holds(), "cancelled by the agent's report");

        arm(&live, now_millis());
        state.turn_started("s", Some("typed"));
        state.end_turn_off_title("s");
        assert!(live.usage_limit.holds(), "only the title showed a cancel");
        turn_closed(&state, "s", false);
        assert!(!live.usage_limit.holds(), "the turn's real end lets it go");
    }

    /// A Codex turn that failed on something the agent made plain, a model at capacity, ends the
    /// probing turn without failing the account: the hold goes and what waited is written.
    #[tokio::test]
    async fn a_turn_failing_at_capacity_while_held_lets_the_hold_go() {
        let (state, _dir, live, server) = session("usage-limit-capacity");
        state
            .capacity_retries
            .set_delay_for_test(Duration::from_secs(600));
        arm(&live, now_millis());
        state.queue_message("s", "waits");
        state.turn_started("s", Some("typed"));
        crate::adapter::codex::rollout_watch::end_failed_turn(
            &state,
            "s",
            Some("Selected model is at capacity. Please try a different model.".to_string()),
            true,
        );
        assert!(!live.usage_limit.holds());
        assert_eq!(offered(&server, 1).await, ["waits"]);
    }

    /// A fresh reading of the session's own account showing it recovered lets the hold go; an
    /// older reading, one still used up, or one of another account does not.
    #[tokio::test]
    async fn a_fresh_reading_of_the_recovered_account_lets_the_hold_go() {
        let (state, dir, live, server) = session("usage-limit-recovered");
        let account_dir = dir.join("claude-a").to_string_lossy().into_owned();
        let since = now_millis() - 10_000;
        arm(&live, since);
        state.queue_message("s", "one");
        state.queue_message("s", "two");
        let read = |used, account: Option<&str>, started_at| {
            let mut quota = reading(used);
            quota.account = account.map(str::to_string);
            state
                .account_quotas
                .hold_in_dir_for_test(&quota, &account_dir);
            after_quota_reading(
                &state,
                AgentId::CLAUDE,
                quota.account.clone(),
                Some(account_dir.clone()),
                started_at,
                None,
            )
        };

        assert!(read(20.0, Some("claude-a"), since).is_none(), "older");
        if let Some(task) = read(100.0, Some("claude-a"), now_millis()) {
            task.await.unwrap();
        }
        assert!(live.usage_limit.holds(), "still used up");
        if let Some(task) = read(20.0, Some("claude-b"), now_millis()) {
            task.await.unwrap();
        }
        assert!(live.usage_limit.holds(), "another account");
        runtime_settled().await;
        assert!(server.offered().is_empty());

        let mut quota = reading(20.0);
        quota.account = Some("claude-a".to_string());
        let task =
            crate::accounts::quotas::finish_probe_for_test(&state, quota, Some(&account_dir))
                .expect("the hold is reconsidered");
        task.await.unwrap();
        assert!(!live.usage_limit.holds(), "recovered");
        assert_eq!(offered(&server, 2).await, ["one", "two"]);
    }

    /// What a reading shows is judged by the account-wide windows only: a model-specific window
    /// does not count either way, a reading with none of the others or a failed reading shows
    /// nothing, and one window at its limit is enough to keep the hold.
    #[tokio::test]
    async fn only_the_account_wide_windows_of_a_reading_say_it_recovered() {
        use QuotaWindowKind::{CreditPeriod, Session, Weekly, WeeklyModel};
        let (state, dir, live, _server) = session("usage-limit-windows");
        let account_dir = dir.join("claude-a").to_string_lossy().into_owned();
        let cases = [
            (vec![], false, "no window"),
            (vec![(WeeklyModel, 10.0)], false, "only a model's window"),
            (
                vec![(Weekly, 20.0), (Session, 100.0)],
                false,
                "5 hours used up",
            ),
            (
                vec![(Weekly, 100.0), (Session, 20.0)],
                false,
                "week used up",
            ),
            (vec![(CreditPeriod, 100.0)], false, "credits used up"),
            (vec![(Weekly, f64::NAN)], false, "no figure"),
            (
                vec![(Weekly, 20.0), (WeeklyModel, 100.0)],
                true,
                "a model's alone used up",
            ),
            (
                vec![(Weekly, 20.0), (Session, 99.0)],
                true,
                "both below the limit",
            ),
        ];
        for (windows, released, name) in cases {
            arm(&live, now_millis() - 10_000);
            let quota = reading_of(windows);
            state
                .account_quotas
                .hold_in_dir_for_test(&quota, &account_dir);
            if let Some(task) = after_quota_reading(
                &state,
                AgentId::CLAUDE,
                Some("claude-a".to_string()),
                Some(account_dir.clone()),
                now_millis(),
                None,
            ) {
                task.await.unwrap();
            }
            assert_eq!(!live.usage_limit.holds(), released, "{name}");
        }

        arm(&live, now_millis() - 10_000);
        let mut quota = reading(20.0);
        quota.unavailable = Some(crate::protocol::QuotaUnavailable::new(
            crate::protocol::quota_unavailable_code::NOT_SIGNED_IN,
            "x",
            &[("agent", "Claude Code")],
        ));
        state
            .account_quotas
            .hold_in_dir_for_test(&quota, &account_dir);
        if let Some(task) = after_quota_reading(
            &state,
            AgentId::CLAUDE,
            Some("claude-a".to_string()),
            Some(account_dir),
            now_millis(),
            None,
        ) {
            task.await.unwrap();
        }
        assert!(live.usage_limit.holds(), "a failed reading");
    }

    /// A session of another agent than the one read is not reconsidered.
    #[tokio::test]
    async fn a_reading_of_another_agent_is_not_for_the_session() {
        let (state, dir, live, _server) = session("usage-limit-other-agent");
        let account_dir = dir.join("claude-a").to_string_lossy().into_owned();
        arm(&live, now_millis() - 10_000);
        assert!(after_quota_reading(
            &state,
            AgentId::CODEX,
            Some("claude-a".to_string()),
            Some(account_dir),
            now_millis(),
            None,
        )
        .is_none());
        assert!(live.usage_limit.holds());
    }

    /// The automatic account switch acts on a reading first: until it has, a session it may be
    /// about to restart is left held, and one it did restart, whose relaunch takes what was
    /// queued, is not written to again.
    #[tokio::test]
    async fn the_hold_waits_for_the_switch_that_acts_on_the_same_reading() {
        let (state, dir, live, server) = session("usage-limit-after-switch");
        let account_dir = dir.join("claude-a").to_string_lossy().into_owned();
        arm(&live, now_millis() - 10_000);
        state.queue_message("s", "one");
        state
            .account_quotas
            .hold_in_dir_for_test(&reading(20.0), &account_dir);

        // Not yet done: still held although the reading shows recovery.
        let proceed = Arc::new(tokio::sync::Notify::new());
        let after = AcceptingServer::new();
        let relaunch = idle_stand_in("s").with_transport(after.clone());
        let switching = {
            let (state, live, proceed) = (state.clone(), live.clone(), proceed.clone());
            let relaunched: Arc<LiveSession> = (*relaunch).clone();
            tokio::spawn(async move {
                proceed.notified().await;
                // What the switch does to the session: its queue is kept, the process goes and its
                // relaunch, which no hold holds, takes the queue.
                let queue = state.hold_outbox_for_restart("s");
                let was_current = state.remove_live(&live);
                state.after_live_removed("s", was_current);
                let claim = state.begin_launch("s").unwrap();
                state.register_live(relaunched.clone());
                let mut record = state.session_record("s").unwrap();
                record.status = SessionStatus::Idle;
                state.save_session(&record).unwrap();
                queue.forward_to("s");
                drop(queue);
                drop(claim);
            })
        };
        let task = after_quota_reading(
            &state,
            AgentId::CLAUDE,
            Some("claude-a".to_string()),
            Some(account_dir),
            now_millis(),
            Some(switching),
        )
        .expect("the hold is reconsidered after the switch");
        runtime_settled().await;
        assert!(live.usage_limit.holds(), "the switch has not finished");

        proceed.notify_one();
        task.await.unwrap();
        // The switch's relaunch took the queue, and the reading's reconsideration of the session,
        // which no longer holds anything, added nothing to it.
        assert_eq!(offered(&after, 1).await, ["one"]);
        runtime_settled().await;
        assert_eq!(after.offered(), ["one"], "written once");
        assert!(
            server.offered().is_empty(),
            "nothing went into the old process"
        );
        assert_eq!(state.outbox.len("s"), 0);
        assert!(!state.live_session("s").unwrap().usage_limit.holds());
    }
}
