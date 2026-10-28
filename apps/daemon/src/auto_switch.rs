//! The account switch Octoboard makes by itself: a session whose turn stopped because its account
//! reached the plan's usage limit is moved to another account of its agent, while the setting
//! `auto_switch_on_usage_limit` is on. What the switch does and refuses is
//! `coordinator::switch_session_account_with`'s, the one an owner's tool and the user's request
//! run, so the conversation comes along and the turn the limit stopped is continued (see
//! `crate::switch_continue`).
//!
//! **When.** At the end of a failed turn (`reporting::close_turn_and_report`), and only when the
//! session's conversation record says the turn stopped on the usage limit
//! (`switch_continue::stopped_on_usage_limit`), read again for a few seconds ahead of the switch's
//! claim, since the record may lag the report, and once more under the claim. A quota reading
//! triggers nothing: it only chooses the target. A session is never moved because its account
//! reads as used up, a working or an idle one included; each turn that hits the limit is its own
//! one-shot trigger, so no wait is kept between switches. A turn that fails while the session is
//! still under a claim (the continued turn of a switch that has just relaunched it) is judged when
//! the claim ends, waiting for it up to [`CLAIM_WAIT`]; the trigger is dropped if the session was
//! moved to another account meanwhile.
//!
//! **Which sessions.** Those with a process at their prompt or in a turn (`working`, `idle`). A
//! session waiting for the user, by a hook, a request or what the terminal shows (a trust prompt, a
//! dialog of the agent), is left alone: moving it would end the prompt it waits at. The exception
//! is a session whose only wait is its agent's usage-limit dialog (Grok's): ending that dialog is
//! the point of the switch. A session still under a launch, a resume or a switch once the claim
//! wait is over is left alone too, and so is one a change of its account's sharing holds (the
//! switch refuses both). A try refused only because the time was wrong is not an error and tells
//! nobody.
//! A locked session is switched like any other: the lock keeps it from being archived or deleted,
//! not from moving between accounts, which neither removes it nor changes what it holds.
//!
//! **Which account.** `coordinator::preferred_weekly_account_except`, the one a new session would
//! be given, without the account the session is on and without any it was moved off for a limit
//! less than [`LEFT_FOR_LIMIT_FOR`] ago, so that two accounts that are both at their limit are not
//! swapped between at every turn. It skips every account that clearly cannot take the session: one
//! used up, turned off, logged out, in a sharing change, unknown, or whose quota cannot be read
//! now or has not been read yet. No minimum of quota left is asked. With none left, nothing moves.
//!
//! **Who is told.** When the switch succeeds, the session, by the prompt it is relaunched with, its
//! owner if it has one, and the user, by a `session_notice` (`account_switched_automatically`).
//! When it fails after its process was ended, Octoboard brings the session back up on the account
//! it had (`coordinator::restore_after_failed_switch`) and tells nobody. Only when that fails too
//! are the owner and the user told, by a `session_notice`
//! (`account_switch_automatically_failed`). A failure before the process was ended leaves the
//! session as it was and is only logged. A switch for a session the user archived or deleted
//! meanwhile, or cut short by the daemon shutting down, is neither told nor logged.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::coordinator::{self, LaunchEdit, NoLongerDue, SwitchOnlyIf};
use crate::mcp;
use crate::protocol::{
    error_code, error_parts, notice_code, Agent, CodedError, Notice, Session, SessionStatus,
    DEFAULT_ACCOUNT_NAME,
};
use crate::reporting::{self, WhenBlocked, NOTICE_PREFIX};
use crate::sharing;
use crate::state::AppState;
use crate::term::{Launch, LaunchRequest};

/// How long after moving a session off an account for its usage limit the session is not moved
/// back onto that account.
const LEFT_FOR_LIMIT_FOR: Duration = Duration::from_secs(30 * 60);

/// How long a failed turn waits for another claim on its session to end before it is judged. A
/// switch keeps its claim past the relaunch until the process has stayed up a few seconds, and the
/// turn it continues may fail within that time.
const CLAIM_WAIT: Duration = Duration::from_secs(15);

/// The sessions [`after_failed_turn`] was asked to switch under test.
#[cfg(test)]
pub(crate) static ASKED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Moves the session `id` to another account if its turn that has just failed stopped on the
/// usage limit and the setting is on. Runs in the background, off the turn's path.
pub fn after_failed_turn(state: &Arc<AppState>, id: &str) {
    match state.store.auto_switch_on_usage_limit() {
        Ok(true) => {}
        Ok(false) => return,
        Err(err) => {
            tracing::warn!(%err, "reading whether sessions are switched automatically failed");
            return;
        }
    }
    // The switch itself starts an agent, which a test cannot; it only sees that one was asked for.
    #[cfg(test)]
    ASKED.lock().unwrap().push(id.to_string());
    #[cfg(not(test))]
    {
        let (state, id) = (state.clone(), id.to_string());
        tokio::spawn(async move {
            switch_after_failed_turn(
                &state,
                &id,
                crate::env_shell::launch_snapshot,
                crate::term::launch,
            )
            .await;
        });
    }
}

/// [`after_failed_turn`], waiting for the switch, with the shell environment `snapshot` gives and
/// the processes `launch` starts: `env_shell::launch_snapshot` and `term::launch` outside the
/// tests, which cannot start an agent.
async fn switch_after_failed_turn(
    state: &Arc<AppState>,
    id: &str,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
    launch: impl Fn(LaunchRequest) -> Result<Launch> + Clone + Send + 'static,
) {
    match try_switch(state, id, snapshot, launch).await {
        Ok(()) => {}
        // Archived or deleted while it was being switched, or the daemon shutting down.
        Err(err) if is_quiet(&err) || state.is_shutting_down() => {}
        Err(err) => tracing::warn!(
            session = %id,
            err = %format!("{err:#}"),
            "switching a session off the account whose usage limit it reached failed"
        ),
    }
}

/// Switches the session `id` to the account a new session would be given, if the setting is on,
/// it is in a state to be switched and an account qualifies, and tells those concerned. Nothing
/// when not. The turn's stop on the usage limit is waited for here, ahead of any claim: a failed
/// turn that did not stop on it (an overload, a network error) must not leave the session shown as
/// switching and refuse everything else meanwhile. Neither the wait nor the shell snapshot it may
/// need is started unless some account could take the session. A claim held on the session is
/// waited for first ([`wait_for_claim`]); a trigger from a turn on one account is dropped if the
/// session was moved to another during that wait. What was decided on the records read here is
/// held to under the switch's claim as well: the account the limit was found on (the session is
/// left alone if it changed during the wait), that it has a process, that it does not wait for the
/// user, and that its last turn stopped on the usage limit.
async fn try_switch(
    state: &Arc<AppState>,
    id: &str,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
    launch: impl Fn(LaunchRequest) -> Result<Launch> + Clone + Send + 'static,
) -> Result<()> {
    if !state.store.auto_switch_on_usage_limit()? {
        return Ok(());
    }
    // The account the turn that triggered this ran on, which the wait for a claim may change.
    let Some(triggered_on) = state
        .store
        .get_session(id)?
        .map(|session| session.account_id)
    else {
        return Ok(());
    };
    wait_for_claim(state, id).await;
    let Some(session) = switchable_session(state, id)? else {
        return Ok(());
    };
    if session.account_id != triggered_on {
        return Ok(());
    }
    if pick_target(state, &session)?.is_none() {
        return Ok(());
    }
    // The snapshot is taken for the record's sake only where finding it needs one (the default
    // account has no directory of its own); the switch is given it in place of the original, which
    // is spent. Otherwise the switch takes the original itself.
    type Snapshot = Box<dyn FnOnce() -> Result<HashMap<String, String>> + Send>;
    let (shell_env, snapshot): (_, Snapshot) = if coordinator::record_needs_shell_env(&session) {
        let env = tokio::task::spawn_blocking(snapshot)
            .await?
            .context("snapshotting the user's shell environment")?;
        (Some(env.clone()), Box::new(move || Ok(env)))
    } else {
        (None, Box::new(snapshot))
    };
    if !coordinator::wait_for_usage_limit(&session, shell_env).await? {
        return Ok(());
    }
    // The wait took time: what the session is doing is read again, and the account the limit was
    // found on must be the one it is on still.
    let Some(waited) = switchable_session(state, id)? else {
        return Ok(());
    };
    if waited.account_id != session.account_id {
        return Ok(());
    }
    let Some(target) = pick_target(state, &waited)? else {
        return Ok(());
    };
    switch_to(state, &waited, target, snapshot, launch).await
}

/// Waits up to [`CLAIM_WAIT`] for the claim on the session `id` to be released, so that a turn that
/// failed under one (the continued turn of a switch that has just relaunched it) is judged once
/// the session is free, and not dropped. A claim still held then leaves the session to be refused
/// as busy.
async fn wait_for_claim(state: &Arc<AppState>, id: &str) {
    let deadline = Instant::now() + CLAIM_WAIT;
    while state.is_launching(id) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// The account `session` would be moved to, if any qualifies: the one a new session would be given,
/// without the account it is on and the ones it was moved off for a limit lately. The outer
/// `Option` is whether one qualifies, the inner one the account (`None` the default account).
fn pick_target(state: &Arc<AppState>, session: &Session) -> Result<Option<Option<String>>> {
    let mut except = state.recently_left_for_limit(&session.id, LEFT_FOR_LIMIT_FOR);
    except.push(session.account_id.clone());
    coordinator::preferred_weekly_account_except(state, session.agent, &except, true)
}

/// The session `id` as it stands if it is in a state to be switched now: a process at its prompt or
/// in a turn, not waiting for the user and not under a launch. `None` for one deleted since.
fn switchable_session(state: &Arc<AppState>, id: &str) -> Result<Option<Session>> {
    // Deleted since: nothing left to switch.
    let Some(session) = state.store.get_session(id)? else {
        return Ok(None);
    };
    let switchable = matches!(
        state.status_for_switch(&session),
        SessionStatus::Working | SessionStatus::Idle
    ) && state.live_session(id).is_some()
        && !state.is_launching(id);
    Ok(switchable.then_some(session))
}

/// Switches `session` to `target`, the account picked for it, then tells those concerned; on a
/// failure after its process was ended, brings it back up on the account it had.
async fn switch_to(
    state: &Arc<AppState>,
    session: &Session,
    target: Option<String>,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
    launch: impl Fn(LaunchRequest) -> Result<Launch> + Clone + Send + 'static,
) -> Result<()> {
    let id = session.id.as_str();
    let from = account_name(state, session, session.account_id.as_deref());
    let to = account_name(state, session, target.as_deref());
    let note = note_to_session(session.agent, &from, &to);
    let only_if = SwitchOnlyIf {
        on_account: Some(session.account_id.as_deref()),
        not_waiting: true,
        running: true,
        usage_limited: true,
        watch_launch: true,
        note: Some(&note),
    };
    let continued = match coordinator::switch_session_account_if(
        state,
        id,
        Some(target.clone()),
        None,
        only_if,
        None,
        LaunchEdit::default(),
        snapshot,
        launch.clone(),
    )
    .await
    {
        Ok(continued) => continued,
        Err(failed) => {
            let err = failed.error;
            if !failed.ended_process {
                // The session is as it was (or, for a process that did not stop in time, may still
                // run): a refusal of the time is no error, any other failure is logged.
                return if is_wrong_time(&err) {
                    Ok(())
                } else {
                    Err(err)
                };
            }
            if is_quiet(&err) {
                return Err(err);
            }
            // The restore would be refused, and the switch is neither told nor logged.
            if state.is_shutting_down() {
                return Ok(());
            }
            tracing::warn!(
                session = %id,
                err = %format!("{err:#}"),
                "switching a session off the account whose usage limit it reached failed after its \
                 process was ended; bringing it back up on the account it had"
            );
            return restore(state, session, target.as_deref(), launch).await;
        }
    };
    let moved = state.session_record(id)?;
    // The default account has no id, so its param is left out.
    let mut params = vec![
        ("agent", moved.agent.label()),
        ("from", from.as_str()),
        ("account", to.as_str()),
        ("session", moved.id.as_str()),
    ];
    params.extend(session.account_id.as_deref().map(|id| ("from_id", id)));
    params.extend(moved.account_id.as_deref().map(|id| ("account_id", id)));
    state.broadcast(
        Notice::new(
            notice_code::ACCOUNT_SWITCHED_AUTOMATICALLY,
            format!(
                "{from} ({}) reached its usage limit, so {} switched this session to {to}.",
                moved.agent.label(),
                crate::APP_NAME
            ),
            &params,
        )
        .about(&moved.id),
    );
    let state = state.clone();
    // Delivery writes into the sessions' PTYs, which blocks.
    tokio::task::spawn_blocking(move || {
        tell_sessions(&state, &moved, &from, &to, &note, continued)
    })
    .await?;
    Ok(())
}

/// Brings `session` back up on the account it had after a switch to `target` failed past ending its
/// process, and tells the owner and the user only if that fails too. Somebody having brought it
/// back meanwhile, the user having archived or deleted it, or the daemon shutting down, is nothing
/// to tell.
async fn restore(
    state: &Arc<AppState>,
    session: &Session,
    target: Option<&str>,
    launch: impl FnOnce(LaunchRequest) -> Result<Launch> + Send + 'static,
) -> Result<()> {
    match coordinator::restore_after_failed_switch(state, session, target, launch).await {
        Ok(()) => Ok(()),
        Err(err) if is_quiet(&err) || is_already_up(&err) || state.is_shutting_down() => Ok(()),
        Err(err) => {
            tell_switch_failed(state, session, target, &err).await;
            Err(err)
        }
    }
}

/// What the account `id` of `session`'s agent is called (`None` the default account), for a
/// notice. A name that cannot be read is replaced by the account's id, or by the default account's
/// standard name, so that a notice of what happened is not lost for it.
fn account_name(state: &Arc<AppState>, session: &Session, id: Option<&str>) -> String {
    state
        .store
        .account_display_name(session.agent, id)
        .unwrap_or_else(|lookup| {
            tracing::warn!(session = %session.id, %lookup, "reading an account name failed");
            fallback_account_name(id)
        })
}

/// What stands in for the name of the account `id` (`None` the default account).
fn fallback_account_name(id: Option<&str>) -> String {
    id.map_or_else(|| DEFAULT_ACCOUNT_NAME.to_string(), str::to_string)
}

/// Tells the user and the owner that the switch of `session` to `target` failed after its process
/// was ended and that bringing it back on its account failed with `err`.
async fn tell_switch_failed(
    state: &Arc<AppState>,
    session: &Session,
    target: Option<&str>,
    err: &anyhow::Error,
) {
    let (reason_code, failure, reason) = error_parts(err);
    let (from, to) = (
        account_name(state, session, session.account_id.as_deref()),
        account_name(state, session, target),
    );
    let mut params: Vec<(&str, &str)> = vec![
        ("agent", session.agent.label()),
        ("from", from.as_str()),
        ("account", to.as_str()),
        ("reason", reason.as_str()),
        ("reason_code", reason_code.as_str()),
        ("session", session.id.as_str()),
    ];
    params.extend(session.account_id.as_deref().map(|id| ("from_id", id)));
    params.extend(target.map(|id| ("account_id", id)));
    // The failure's own params come after ours and only where ours have no param of the name, the
    // ids of the default account included, which are left out.
    let ours = ["from_id", "account_id"];
    let others: Vec<_> = failure
        .iter()
        .filter(|(name, _)| {
            !params.iter().any(|(own, _)| own == name) && !ours.contains(&name.as_str())
        })
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    params.extend(others);
    state.broadcast(
        Notice::new(
            notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED,
            format!(
                "{from} ({}) reached its usage limit, and {} could neither switch this session to \
                 {to} nor bring it back up on {from}: {reason}",
                session.agent.label(),
                crate::APP_NAME
            ),
            &params,
        )
        .about(&session.id),
    );
    let (state, session, reason) = (state.clone(), session.clone(), format!("{err:#}"));
    // Delivery writes into the owner's PTY, which blocks.
    let _ = tokio::task::spawn_blocking(move || {
        reporting::tell_owner_the_switch_failed(&state, &session, &reason)
    })
    .await;
}

/// Whether the switch failed for a reason nobody is told of and that is not logged: the session was
/// deleted or archived before or while it ran, which is the user's word on the matter, or the
/// daemon is shutting down.
fn is_quiet(err: &anyhow::Error) -> bool {
    err.downcast_ref::<CodedError>().is_some_and(|coded| {
        [
            error_code::UNKNOWN_SESSION,
            error_code::SESSION_ARCHIVED,
            error_code::DAEMON_SHUTTING_DOWN,
        ]
        .contains(&coded.code)
    })
}

/// Whether restoring found the session running already: the user or an owner resumed it meanwhile.
/// A session another claim still held when the restore gave up waiting is not: it is down, and
/// that is told.
fn is_already_up(err: &anyhow::Error) -> bool {
    err.downcast_ref::<CodedError>()
        .is_some_and(|coded| coded.code == error_code::SESSION_ALREADY_RUNNING)
}

/// Whether a refused switch only found the time wrong, which is no error: the session was busy or
/// had changed, its turn was not (or no longer) one the limit stopped, or the account picked for it
/// had been turned off, logged out, used up or removed since. Only for a refusal that left the
/// session as it was.
fn is_wrong_time(err: &anyhow::Error) -> bool {
    err.is::<NoLongerDue>()
        || err.downcast_ref::<CodedError>().is_some_and(|coded| {
            [
                error_code::SESSION_ALREADY_STARTING,
                error_code::ACCOUNT_SHARING_IN_PROGRESS,
                error_code::ACCOUNT_DISABLED,
                error_code::ACCOUNT_QUOTA_EXHAUSTED,
                error_code::ACCOUNT_NOT_SIGNED_IN,
                error_code::UNKNOWN_ACCOUNT,
            ]
            .contains(&coded.code)
        })
}

/// What the switched session is told, put ahead of the prompt it is relaunched with to carry on:
/// a message written into it would open a turn of its own, which its owner would be sent a report
/// of.
fn note_to_session(agent: Agent, from: &str, to: &str) -> String {
    format!(
        "{NOTICE_PREFIX}: your {} account `{from}` reached its usage limit, so Octoboard switched \
         you to `{to}` by itself. Your conversation came with you, and nothing else about the \
         session changed.",
        agent.label()
    )
}

/// Tells the switched session, when its relaunch did not carry the note, and its owner if it has
/// one, what became of it. A message to a session that cannot take one now is queued; one that
/// cannot be delivered at all is logged and goes no further.
fn tell_sessions(
    state: &Arc<AppState>,
    moved: &Session,
    from: &str,
    to: &str,
    note: &str,
    continued: bool,
) {
    if !continued {
        // The turn this opens is the notice's own, which needs no report back.
        state.credit_turn_opened_by(&moved.id, NOTICE_PREFIX.to_string());
        let own = format!(
            "{note} Carry on with what you were doing, or wait for your next instruction if you \
             had none."
        );
        if !tell(state, &moved.id, &own) {
            state.clear_turn_credit(&moved.id, NOTICE_PREFIX);
        }
    }
    let Some(owner_id) = &moved.bound_to else {
        return;
    };
    let owner = match state.store.get_session(owner_id) {
        Ok(Some(owner)) => owner,
        _ => return,
    };
    let text = format!(
        "{NOTICE_PREFIX}: {session} reached the usage limit of its {} account `{from}`, so \
         Octoboard switched it to `{to}` by itself. Its conversation came with it. {}",
        moved.agent.label(),
        if continued {
            "Its turn had stopped on the limit, so it was sent a prompt to continue that task: do \
             not send it one yourself."
                .to_string()
        } else {
            format!(
                "It is back awaiting instructions, so send it the next step with `{}` if it has \
                 one pending.",
                mcp::qualified_tool_name(owner.agent, "instruct_session")
            )
        },
        session = sharing::named_session(moved),
    );
    tell(state, &owner.id, &text);
}

/// Writes `text` into the session `id`, queued until it can take it. Whether it was accepted.
fn tell(state: &Arc<AppState>, id: &str, text: &str) -> bool {
    match reporting::write_message(state, id, text, WhenBlocked::Queue) {
        Ok(_) => true,
        Err(err) => {
            tracing::debug!(session = %id, %err, "telling a session of its switch failed");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{now_millis, Account, AccountQuota, Event, QuotaWindow, QuotaWindowKind};
    use crate::test_support::{
        app_state, console, insert_project, output_containing, running, stand_in_launch, Launched,
        ScratchDir, StandIn,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    type Launches = Arc<Mutex<Vec<Launched>>>;
    type Started = Arc<Mutex<Vec<StandIn>>>;

    const CONVERSATION: &str = "projects/slug/agent-1.jsonl";

    /// A project session `w` of Claude Code, owned by the console session `owner` (which has a
    /// process that takes messages), with its conversation in the default account's directory,
    /// ending on a usage limit, and a process watched for its end; two accounts, `claude-a`
    /// ("Work A") and `claude-b`; and the setting that switches sessions automatically on. The
    /// default account's weekly quota has 60% left and `claude-a`'s 80%; `claude-b`, which the
    /// owner is on, has not been read.
    struct Fixture {
        state: Arc<AppState>,
        dir: ScratchDir,
        owner: StandIn,
        worker_process: StandIn,
        launches: Launches,
        started: Started,
    }

    impl Fixture {
        fn new(label: &str, status: SessionStatus) -> Self {
            let (state, dir) = app_state(label);
            state
                .store
                .insert_console(&crate::protocol::Console {
                    workdir: dir.to_string_lossy().into_owned(),
                    ..console(&dir)
                })
                .unwrap();
            let project_dir = dir.join("project");
            std::fs::create_dir_all(&project_dir).unwrap();
            insert_project(&state, &project_dir);
            state
                .store
                .insert_session(&crate::test_support::console_session(
                    "owner",
                    SessionStatus::Idle,
                ))
                .unwrap();
            state
                .store
                .insert_session(&Session {
                    project_id: Some("p1".to_string()),
                    bound_to: Some("owner".to_string()),
                    has_conversation: true,
                    agent_session_id: Some("agent-1".to_string()),
                    ..crate::test_support::project_session("w", Agent::Claude, status)
                })
                .unwrap();
            for (id, name) in [("claude-a", "Work A"), ("claude-b", "Work B")] {
                state
                    .store
                    .insert_account(&Account {
                        id: id.to_string(),
                        agent: Agent::Claude,
                        name: name.to_string(),
                        config_dir: dir.join(id).to_string_lossy().into_owned(),
                        share_default_config: false,
                        share_default_config_incomplete: false,
                        disabled: false,
                    })
                    .unwrap();
            }
            std::fs::create_dir_all(
                dir.join("default-claude")
                    .join(CONVERSATION)
                    .parent()
                    .unwrap(),
            )
            .unwrap();
            state.store.set_auto_switch_on_usage_limit(true).unwrap();
            // Not on the account the worker is on, or the owner would be a candidate as well.
            state
                .set_session_account("owner", Some("claude-b"), dir.join("claude-b").to_str())
                .unwrap();
            let fixture = Self {
                owner: running(&state, "owner"),
                worker_process: {
                    let worker = running(&state, "w");
                    state.watch_exit(worker.clone());
                    worker
                },
                state,
                dir,
                launches: Default::default(),
                started: Default::default(),
            };
            fixture.hold(None, 40.0);
            fixture.hold(Some("claude-a"), 20.0);
            fixture.end_on_usage_limit();
            fixture
        }

        /// Holds a reading of `account` with a weekly window `used` percent used, resetting
        /// tomorrow.
        fn hold(&self, account: Option<&str>, used: f64) {
            let window = QuotaWindow {
                kind: QuotaWindowKind::Weekly,
                label: None,
                used_percent: used,
                resets_at: Some(now_millis() + 86_400_000),
                window_minutes: None,
            };
            self.hold_reading(account, vec![window], None);
        }

        fn hold_reading(
            &self,
            account: Option<&str>,
            windows: Vec<QuotaWindow>,
            unavailable: Option<crate::protocol::QuotaUnavailable>,
        ) {
            let quota = AccountQuota {
                agent: Agent::Claude,
                account: account.map(str::to_string),
                fetched_at: 1,
                plan: None,
                windows,
                unavailable,
            };
            match account {
                None => self.state.account_quotas.hold_for_test(&quota),
                Some(id) => self
                    .state
                    .account_quotas
                    .hold_in_dir_for_test(&quota, self.dir.join(id).to_str().unwrap()),
            }
        }

        /// Puts a dialog of Claude Code's on the worker's screen, which shows it as waiting for
        /// the user without any hook saying so.
        fn show_dialog(&self) {
            self.worker_process.on_output(
                "\n  Resume this conversation?\n  This conversation has been inactive for 1h and \
                 is 42k tokens long.\n  \u{276f} Resume\n    Start a new conversation\n\n  \
                 Enter to confirm \u{b7} Esc to dismiss\n"
                    .as_bytes(),
            );
            assert!(self.worker_process.look_for_dialog().current.is_some());
        }

        fn clear_screen(&self) {
            self.worker_process.on_output(b"\x1b[2J\x1b[H");
            assert!(self.worker_process.look_for_dialog().current.is_none());
        }

        fn worker(&self) -> Session {
            self.state.session_record("w").unwrap()
        }

        fn env(&self) -> HashMap<String, String> {
            HashMap::from([(
                "CLAUDE_CONFIG_DIR".to_string(),
                self.dir
                    .join("default-claude")
                    .to_string_lossy()
                    .into_owned(),
            )])
        }

        /// The end of a turn that failed, run as the daemon runs it, with a relaunch that starts
        /// a stand-in.
        async fn failed_turn(&self) {
            let (started, launches) = (self.started.clone(), self.launches.clone());
            let env = self.env();
            switch_after_failed_turn(
                &self.state,
                "w",
                move || Ok(env),
                move |request| stand_in_launch(&started, &launches, || {})(request),
            )
            .await;
        }

        /// A relaunch that fails its first `failures` calls and starts a stand-in on the rest.
        fn launch_failing(
            &self,
            failures: usize,
        ) -> impl Fn(LaunchRequest) -> Result<Launch> + Clone + Send + 'static {
            let (started, launches) = (self.started.clone(), self.launches.clone());
            let calls = Arc::new(AtomicUsize::new(0));
            move |request| {
                if calls.fetch_add(1, Ordering::SeqCst) < failures {
                    anyhow::bail!("the agent cannot be started");
                }
                stand_in_launch(&started, &launches, || {})(request)
            }
        }

        fn launched(&self) -> usize {
            self.launches.lock().unwrap().len()
        }

        /// The prompt the `n`th relaunch was given.
        fn prompt_of(&self, n: usize) -> Option<String> {
            self.launches.lock().unwrap()[n].2.clone()
        }

        /// Makes the conversation end on the plan's usage limit, as Claude Code writes it.
        fn end_on_usage_limit(&self) {
            self.write_conversation(concat!(
                r#"{"type":"user","message":{"role":"user","content":"hello"}}"#,
                "\n",
                r#"{"type":"assistant","isApiErrorMessage":true,"error":"rate_limit","#,
                r#""apiError":"usage_limit_reached","message":{"role":"assistant","#,
                r#""stop_reason":"stop_sequence","content":[{"type":"text","text":"limit"}]}}"#,
                "\n"
            ));
        }

        /// Makes the conversation end on a tool call that never got its result, as a turn that is
        /// running does.
        fn end_on_a_tool_call(&self) {
            self.write_conversation(concat!(
                r#"{"type":"user","message":{"role":"user","content":"run it"}}"#,
                "\n",
                r#"{"type":"assistant","message":{"role":"assistant","stop_reason":"tool_use","#,
                r#""content":[{"type":"tool_use","id":"t","name":"Bash","input":{}}]}}"#,
                "\n"
            ));
        }

        fn write_conversation(&self, text: &str) {
            std::fs::write(self.dir.join("default-claude").join(CONVERSATION), text).unwrap();
        }
    }

    /// The `session_notice` events of `code` among those received, with the session each is
    /// about.
    fn notices(
        events: &mut tokio::sync::broadcast::Receiver<Event>,
        code: &str,
    ) -> Vec<(String, crate::protocol::Params)> {
        let mut found = Vec::new();
        while let Ok(event) = events.try_recv() {
            if let Event::SessionNotice {
                session,
                code: sent,
                params,
                ..
            } = event
            {
                if sent == code {
                    found.push((session, params));
                }
            }
        }
        found
    }

    /// What the tracing of the test body wrote, for asserting that nothing was logged.
    #[derive(Clone, Default)]
    struct LogBuffer(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for LogBuffer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogBuffer {
        type Writer = LogBuffer;

        fn make_writer(&'a self) -> LogBuffer {
            self.clone()
        }
    }

    impl LogBuffer {
        /// Captures what the current thread logs until the guard drops; a current-thread test
        /// runtime runs the code under test on it.
        fn capture() -> (Self, tracing::subscriber::DefaultGuard) {
            let logs = Self::default();
            let guard = tracing::subscriber::set_default(
                tracing_subscriber::fmt()
                    .with_writer(logs.clone())
                    .with_ansi(false)
                    .finish(),
            );
            (logs, guard)
        }

        fn text(&self) -> String {
            String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
        }
    }

    /// What the owner has been written once a marker has reached it.
    async fn owner_heard(fixture: &Fixture) -> String {
        let (state, owner) = (fixture.state.clone(), (*fixture.owner).clone());
        tokio::task::spawn_blocking(move || crate::test_support::heard_after_marker(&state, &owner))
            .await
            .unwrap()
    }

    /// A session whose turn stopped on the usage limit is switched to another account whatever
    /// its quota reading says of its own, working or idle: it resumes its conversation there with
    /// one prompt that tells it of the switch and carries on, and the user and its owner are told.
    #[tokio::test]
    async fn a_session_whose_turn_stopped_on_the_usage_limit_is_switched_and_everyone_told() {
        for (label, status) in [
            ("auto-switch-idle", SessionStatus::Idle),
            ("auto-switch-working", SessionStatus::Working),
        ] {
            let fixture = Fixture::new(label, status);
            let mut events = fixture.state.subscribe();

            fixture.failed_turn().await;

            assert_eq!(fixture.worker().account_id.as_deref(), Some("claude-a"));
            assert_eq!(fixture.launched(), 1);
            let prompt = fixture.prompt_of(0).expect("a prompt to carry on");
            assert!(
                prompt.contains("your Claude Code account `Default` reached its usage limit"),
                "{prompt}"
            );
            assert!(
                prompt.contains("switched you to `Work A` by itself"),
                "{prompt}"
            );
            assert!(
                prompt.ends_with(crate::switch_continue::CONTINUE_PROMPT),
                "{prompt}"
            );
            let owner = output_containing(&fixture.owner, "switched it to");
            assert!(
                owner.contains("`w`") && owner.contains("switched it to `Work A` by itself"),
                "{owner}"
            );
            assert!(owner.contains("do not send it one yourself"), "{owner}");
            let told = notices(&mut events, notice_code::ACCOUNT_SWITCHED_AUTOMATICALLY);
            assert_eq!(told.len(), 1);
            assert_eq!(told[0].0, "w");
            assert_eq!(told[0].1["from"], "Default");
            assert_eq!(told[0].1["account"], "Work A");
            assert_eq!(told[0].1["account_id"], "claude-a");
            assert_eq!(told[0].1["session"], "w");
            assert!(!told[0].1.contains_key("from_id"));
        }
    }

    /// A reading never moves a session: one whose turn did not stop on the limit stays on an
    /// account that reads as used up, a turn that is only running or cut off included.
    #[tokio::test]
    async fn a_session_on_a_used_up_account_is_not_switched_unless_its_turn_hit_the_limit() {
        let fixture = Fixture::new("auto-switch-reading-only", SessionStatus::Working);
        fixture.hold(None, 100.0);
        fixture.end_on_a_tool_call();

        fixture.failed_turn().await;

        assert_eq!(fixture.worker().account_id, None);
        assert_eq!(fixture.launched(), 0);
        assert!(
            fixture.state.live_session("w").is_some(),
            "its process was left alone"
        );
    }

    /// The switch happens only while its own setting is on, which neither automatic selection nor
    /// the approval of an agent's requests turns on.
    #[tokio::test]
    async fn nothing_is_switched_while_the_setting_is_off() {
        let fixture = Fixture::new("auto-switch-off", SessionStatus::Idle);
        fixture
            .state
            .store
            .set_auto_switch_on_usage_limit(false)
            .unwrap();
        fixture.state.store.set_auto_select_account(true).unwrap();
        fixture
            .state
            .store
            .set_auto_approve_account_switches(true)
            .unwrap();

        fixture.failed_turn().await;
        after_failed_turn(&fixture.state, "setting-off");

        assert_eq!(fixture.worker().account_id, None);
        assert_eq!(fixture.launched(), 0);
        assert!(!ASKED.lock().unwrap().contains(&"setting-off".to_string()));
    }

    /// An account that is clearly unusable is never the target: used up, turned off, logged out,
    /// whose quota cannot be read now or has not been read yet, or the one the session is on. With
    /// none left the session stays; once one is usable it is switched.
    #[tokio::test]
    async fn no_account_that_is_clearly_unusable_is_chosen() {
        let fixture = Fixture::new("auto-switch-none", SessionStatus::Idle);
        let mut events = fixture.state.subscribe();
        let unavailable = |code: &'static str| {
            Some(crate::protocol::QuotaUnavailable::new(
                code,
                "x",
                &[("agent", "Claude Code")],
            ))
        };
        let weekly = |used: f64| QuotaWindow {
            kind: QuotaWindowKind::Weekly,
            label: None,
            used_percent: used,
            resets_at: Some(now_millis() + 86_400_000),
            window_minutes: None,
        };
        // The default account is the session's own and reads fine; `claude-b` is not read yet.
        fixture.hold(Some("claude-a"), 100.0);
        fixture.failed_turn().await;
        assert_eq!(fixture.launched(), 0, "used up, and nothing else qualifies");

        fixture.hold(Some("claude-a"), 30.0);
        fixture
            .state
            .store
            .set_account_disabled("claude-a", true)
            .unwrap();
        fixture.failed_turn().await;
        assert_eq!(fixture.launched(), 0, "turned off");
        fixture
            .state
            .store
            .set_account_disabled("claude-a", false)
            .unwrap();

        for code in [
            crate::protocol::quota_unavailable_code::NOT_SIGNED_IN,
            crate::protocol::quota_unavailable_code::CONFIG_DIR_MISSING,
            crate::protocol::quota_unavailable_code::COMMAND_FAILED,
        ] {
            fixture.hold_reading(Some("claude-a"), vec![weekly(30.0)], unavailable(code));
            fixture.failed_turn().await;
            assert_eq!(
                fixture.launched(),
                0,
                "{code}: cannot be read now or is logged out"
            );
        }
        assert!(
            notices(
                &mut events,
                notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED
            )
            .is_empty(),
            "not switching is not a failure"
        );

        fixture.hold(Some("claude-a"), 30.0);
        fixture.failed_turn().await;
        assert_eq!(fixture.worker().account_id.as_deref(), Some("claude-a"));
    }

    /// An account that is gone when the switch is asked for it is a wrong time, not a failure:
    /// nothing changes and nobody is told.
    #[tokio::test]
    async fn an_unknown_target_is_not_a_failure() {
        let fixture = Fixture::new("auto-switch-unknown", SessionStatus::Idle);
        let mut events = fixture.state.subscribe();
        let session = fixture.worker();
        let env = fixture.env();

        switch_to(
            &fixture.state,
            &session,
            Some("no-such-account".to_string()),
            move || Ok(env),
            |_| anyhow::bail!("not reached"),
        )
        .await
        .expect("a wrong time is not an error");

        assert_eq!(fixture.worker().account_id, None);
        assert!(fixture.state.live_session("w").is_some());
        assert!(notices(
            &mut events,
            notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED
        )
        .is_empty());
    }

    /// Switching a session twice for its limit never goes back to the account it left: with the
    /// two other accounts both at their limit it is not swapped between them at every turn, and
    /// no wait is kept after a switch that worked.
    #[tokio::test]
    async fn a_session_is_not_moved_back_onto_the_account_it_just_left() {
        let fixture = Fixture::new("auto-switch-back", SessionStatus::Idle);
        fixture.hold(Some("claude-b"), 50.0);

        fixture.failed_turn().await;
        assert_eq!(fixture.worker().account_id.as_deref(), Some("claude-a"));
        assert_eq!(
            fixture
                .state
                .recently_left_for_limit("w", LEFT_FOR_LIMIT_FOR),
            [None],
            "noted by the switch itself, before its claim was released"
        );

        // The next turn hits the limit on `claude-a` as well, its conversation having come with
        // it. The default account still reads as having quota, but it was left a moment ago, so
        // `claude-b` is the one left.
        fixture.failed_turn().await;

        assert_eq!(fixture.worker().account_id.as_deref(), Some("claude-b"));
    }

    /// A session waiting for the user (by a hook or by what its screen shows), one a launch or
    /// switch holds, and one with no process are left alone.
    #[tokio::test]
    async fn sessions_that_cannot_be_switched_now_are_left_alone() {
        let fixture = Fixture::new("auto-switch-busy", SessionStatus::WaitingUser);
        fixture.failed_turn().await;
        assert_eq!(fixture.launched(), 0, "a prompt it waits at would end");

        fixture
            .state
            .apply_hook_status("w", SessionStatus::Idle)
            .unwrap();
        fixture.show_dialog();
        fixture.failed_turn().await;
        assert_eq!(fixture.launched(), 0, "a prompt the screen shows would end");
        fixture.clear_screen();

        let claim = fixture.state.begin_account_switch("w", false).unwrap();
        fixture.failed_turn().await;
        assert_eq!(fixture.launched(), 0, "a claim is held");
        drop(claim);

        let mut down = fixture.worker();
        down.status = SessionStatus::Interrupted;
        fixture.state.store.update_session(&down).unwrap();
        fixture.failed_turn().await;
        assert_eq!(fixture.launched(), 0, "no process to switch");
        assert_eq!(fixture.worker().account_id, None);
    }

    /// What the decision on a record read earlier is held to under the switch's claim: a session
    /// that is on another account by then, shown as waiting, without a running process, or whose
    /// last turn did not stop on the limit is refused with nothing changed.
    #[tokio::test]
    async fn a_switch_is_refused_under_its_claim_when_the_record_it_was_decided_on_is_stale() {
        let fixture = Fixture::new("auto-switch-stale", SessionStatus::Idle);
        fixture
            .state
            .store
            .insert_session(&Session {
                project_id: Some("p1".to_string()),
                ..crate::test_support::project_session(
                    "no-process",
                    Agent::Claude,
                    SessionStatus::Idle,
                )
            })
            .unwrap();
        let attempt = |id: &'static str, only_if: SwitchOnlyIf<'static>| {
            let fixture = &fixture;
            let env = fixture.env();
            async move {
                coordinator::switch_session_account_if(
                    &fixture.state,
                    id,
                    Some(Some("claude-a".to_string())),
                    None,
                    only_if,
                    None,
                    LaunchEdit::default(),
                    move || Ok(env),
                    |_| anyhow::bail!("not reached"),
                )
                .await
                .expect_err("refused")
            }
        };
        let refusal = |failed: &coordinator::SwitchFailed| {
            assert!(!failed.ended_process);
            failed.error.downcast_ref::<NoLongerDue>().map(|due| due.0)
        };

        let moved_by_hand = attempt(
            "w",
            SwitchOnlyIf {
                on_account: Some(Some("claude-b")),
                ..SwitchOnlyIf::default()
            },
        )
        .await;
        assert_eq!(
            refusal(&moved_by_hand),
            Some("the session is on another account now")
        );

        fixture.show_dialog();
        let waiting = attempt(
            "w",
            SwitchOnlyIf {
                not_waiting: true,
                ..SwitchOnlyIf::default()
            },
        )
        .await;
        assert_eq!(
            refusal(&waiting),
            Some("the session waits for the user now")
        );
        fixture.clear_screen();

        fixture.end_on_a_tool_call();
        let limit = SwitchOnlyIf {
            usage_limited: true,
            ..SwitchOnlyIf::default()
        };
        let cut_off = attempt("w", limit).await;
        assert_eq!(
            refusal(&cut_off),
            Some("the session's last turn did not stop on a usage limit")
        );
        fixture.end_on_usage_limit();

        let mut stopped = fixture.worker();
        stopped.status = SessionStatus::Interrupted;
        fixture.state.store.update_session(&stopped).unwrap();
        let running = SwitchOnlyIf {
            running: true,
            ..SwitchOnlyIf::default()
        };
        let stopped = attempt("w", running).await;
        assert_eq!(
            refusal(&stopped),
            Some("the session has no running process now")
        );
        let exited = attempt("no-process", running).await;
        assert_eq!(
            refusal(&exited),
            Some("the session has no running process now")
        );

        assert_eq!(fixture.worker().account_id, None);
        assert_eq!(fixture.launched(), 0);
        assert!(!fixture.state.is_switching_account("w"));
    }

    /// A switch that fails after the process was ended is undone by Octoboard itself: the session
    /// is relaunched on the account it had, as it was and without a prompt, and nobody is told of
    /// a failure.
    #[tokio::test]
    async fn a_failed_switch_is_undone_by_bringing_the_session_back_up() {
        let fixture = Fixture::new("auto-switch-restored", SessionStatus::Idle);
        let mut events = fixture.state.subscribe();
        let env = fixture.env();

        let outcome = try_switch(
            &fixture.state,
            "w",
            move || Ok(env),
            fixture.launch_failing(1),
        )
        .await;

        outcome.expect("restored, so not an error");
        assert!(fixture
            .state
            .recently_left_for_limit("w", LEFT_FOR_LIMIT_FOR)
            .is_empty());
        let worker = fixture.worker();
        assert_eq!(worker.account_id, None, "on the account it had");
        assert_ne!(worker.status, SessionStatus::Interrupted);
        assert!(fixture.state.live_session("w").is_some(), "it runs again");
        assert_eq!(fixture.launched(), 1, "the one relaunch that worked");
        assert_eq!(
            fixture.prompt_of(0),
            None,
            "no prompt: its turn is not carried on"
        );
        assert!(notices(
            &mut events,
            notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED
        )
        .is_empty());
        assert!(notices(&mut events, notice_code::ACCOUNT_SWITCHED_AUTOMATICALLY).is_empty());
        let heard = owner_heard(&fixture).await;
        assert!(!heard.contains("could neither switch"), "{heard}");
    }

    /// Only when bringing the session back up fails too is an error raised: the user and the
    /// owner are told the session is interrupted on its own account, and why.
    #[tokio::test]
    async fn a_session_that_cannot_be_brought_back_is_told_of() {
        let fixture = Fixture::new("auto-switch-not-restored", SessionStatus::Idle);
        let mut events = fixture.state.subscribe();
        let env = fixture.env();

        let err = try_switch(
            &fixture.state,
            "w",
            move || Ok(env),
            fixture.launch_failing(usize::MAX),
        )
        .await
        .expect_err("neither the switch nor the restoring worked");

        assert!(
            format!("{err:#}").contains("the agent cannot be started"),
            "{err:#}"
        );
        let worker = fixture.worker();
        assert_eq!(worker.account_id, None);
        assert_eq!(worker.status, SessionStatus::Interrupted);
        let mut told = notices(
            &mut events,
            notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED,
        );
        assert_eq!(told.len(), 1, "the user was told");
        let params = told.remove(0).1;
        assert_eq!(params["agent"], "Claude Code");
        assert_eq!(params["from"], "Default");
        assert_eq!(params["account"], "Work A");
        assert_eq!(params["account_id"], "claude-a");
        assert_eq!(params["session"], "w");
        assert!(!params.contains_key("from_id"));
        assert!(!params["reason"].is_empty());
        assert!(params.contains_key("reason_code"));
        let heard = output_containing(&fixture.owner, "could neither switch");
        assert!(
            heard.contains("could neither switch it to another account"),
            "{heard}"
        );
        assert!(!heard.to_lowercase().contains("retry"), "{heard}");
    }

    /// A failure before the switch ended the process leaves the session as it was and tells
    /// nobody, and nothing is relaunched to restore it.
    #[tokio::test]
    async fn a_failure_before_the_process_was_ended_tells_nobody() {
        let fixture = Fixture::new("auto-switch-early-failure", SessionStatus::Idle);
        let mut events = fixture.state.subscribe();
        let session = fixture.worker();

        let err = switch_to(
            &fixture.state,
            &session,
            Some("claude-a".to_string()),
            // The login shell cannot be read: before anything is ended.
            || anyhow::bail!("no shell"),
            fixture.launch_failing(usize::MAX),
        )
        .await
        .expect_err("returned to be logged");

        assert!(format!("{err:#}").contains("no shell"), "{err:#}");
        assert!(notices(
            &mut events,
            notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED
        )
        .is_empty());
        assert!(
            fixture.state.live_session("w").is_some(),
            "its process runs on"
        );
        assert_eq!(fixture.worker().account_id, None);
    }

    /// A process that exits by itself between the running check and the stop was not ended by the
    /// switch: a later failure is not the switch's to report, nothing is restored and nobody is
    /// told.
    #[tokio::test]
    async fn a_process_that_ended_by_itself_before_the_stop_is_not_the_switchs_to_report() {
        let fixture = Fixture::new("auto-switch-exits-itself", SessionStatus::Idle);
        let mut events = fixture.state.subscribe();
        let env = fixture.env();
        let (state, worker) = (fixture.state.clone(), fixture.worker_process.clone());
        let snapshot = move || {
            // Runs after the running check and before the stop: the agent exits on its own.
            worker.terminate();
            assert!(
                crate::trust::wait_for(crate::test_support::PATIENCE, || {
                    state.live_session("w").is_none()
                }),
                "the exit was recorded"
            );
            Ok(env)
        };

        let err = switch_to(
            &fixture.state,
            &fixture.worker(),
            Some("claude-a".to_string()),
            snapshot,
            fixture.launch_failing(usize::MAX),
        )
        .await
        .expect_err("the relaunch failed");

        assert!(
            format!("{err:#}").contains("the agent cannot be started"),
            "{err:#}"
        );
        assert!(notices(
            &mut events,
            notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED
        )
        .is_empty());
        let heard = owner_heard(&fixture).await;
        assert!(!heard.contains("could neither switch"), "{heard}");
    }

    /// The same when the exit has not been recorded yet by the time the switch stops the process:
    /// the process is still registered, but it was gone before the stop.
    #[tokio::test]
    async fn a_process_that_exited_but_is_still_registered_was_not_ended_by_the_switch() {
        let fixture = Fixture::new("auto-switch-exited-registered", SessionStatus::Idle);
        let mut events = fixture.state.subscribe();
        let env = fixture.env();
        let worker = fixture.worker_process.clone();
        // The agent exits during the plan; the exit watcher has not recorded it by the stop.
        let snapshot = move || {
            worker.terminate();
            Ok(env)
        };

        let _ = switch_to(
            &fixture.state,
            &fixture.worker(),
            Some("claude-a".to_string()),
            snapshot,
            fixture.launch_failing(usize::MAX),
        )
        .await;

        assert!(notices(
            &mut events,
            notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED
        )
        .is_empty());
        let heard = owner_heard(&fixture).await;
        assert!(!heard.contains("could neither switch"), "{heard}");
    }

    /// The user archiving the session while it is switched is their word on the matter: the
    /// switch fails after the process was ended, and nobody is told it did, nor is it restored.
    #[tokio::test]
    async fn archiving_the_session_during_the_switch_tells_nobody() {
        let fixture = Fixture::new("auto-switch-archived-meanwhile", SessionStatus::Idle);
        let mut events = fixture.state.subscribe();
        let env = fixture.env();
        let (state, archived) = (fixture.state.clone(), fixture.worker());
        let snapshot = move || {
            // Runs before the process is ended.
            let mut archived = archived;
            archived.status = SessionStatus::Archived;
            state.store.update_session(&archived)?;
            Ok(env)
        };

        let err = switch_to(
            &fixture.state,
            &fixture.worker(),
            Some("claude-a".to_string()),
            snapshot,
            fixture.launch_failing(usize::MAX),
        )
        .await
        .expect_err("the session was archived");

        assert!(is_quiet(&err), "{err:#}");
        assert!(fixture.state.live_session("w").is_none());
        assert!(notices(
            &mut events,
            notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED
        )
        .is_empty());
        let heard = owner_heard(&fixture).await;
        assert!(!heard.contains("could neither switch"), "{heard}");
    }

    /// A session deleted since is skipped without an error.
    #[tokio::test]
    async fn a_session_deleted_meanwhile_is_skipped_quietly() {
        let fixture = Fixture::new("auto-switch-deleted", SessionStatus::Idle);
        try_switch(
            &fixture.state,
            "gone",
            || Ok(HashMap::new()),
            |_| anyhow::bail!("not reached"),
        )
        .await
        .expect("nothing to switch");
    }

    /// A change of the target's sharing that begins after the switch has ended the session's
    /// process refuses the switch too late to be a wrong time: the session is brought back up on
    /// the account it had, and nobody is told.
    #[tokio::test]
    async fn a_refusal_after_the_process_was_ended_is_undone_quietly() {
        let fixture = Fixture::new("auto-switch-late-refusal", SessionStatus::Idle);
        let mut events = fixture.state.subscribe();
        let target = fixture
            .state
            .store
            .get_account("claude-a")
            .unwrap()
            .unwrap();
        let lock = Arc::new(Mutex::new(None));
        let (state, held) = (fixture.state.clone(), lock.clone());
        let env = fixture.env();
        // Runs before the process is ended, after the checks that would have refused.
        let snapshot = move || {
            *held.lock().unwrap() = Some(state.lock_account_sharing(&target, |_, _| Ok(()))?.1);
            Ok(env)
        };

        switch_to(
            &fixture.state,
            &fixture.worker(),
            Some("claude-a".to_string()),
            snapshot,
            fixture.launch_failing(0),
        )
        .await
        .expect("brought back up");

        assert_eq!(fixture.launched(), 1, "the restore launched it");
        assert_eq!(fixture.worker().account_id, None);
        assert!(fixture.state.live_session("w").is_some());
        assert!(notices(
            &mut events,
            notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED
        )
        .is_empty());
        drop(lock);
    }

    /// When the relaunch did not carry the note (the turn could not be carried on after all), the
    /// session is told in a message of its own, which opens a turn that needs no report back.
    #[tokio::test]
    async fn a_note_written_instead_of_the_prompt_opens_a_turn_that_owes_no_report() {
        let fixture = Fixture::new("auto-switch-note-written", SessionStatus::Idle);
        let worker = fixture.worker();
        fixture.state.turn_started("w", Some("work"));

        let (state, note) = (
            fixture.state.clone(),
            note_to_session(Agent::Claude, "A", "B"),
        );
        tokio::task::spawn_blocking(move || tell_sessions(&state, &worker, "A", "B", &note, false))
            .await
            .unwrap();

        let heard = output_containing(&fixture.worker_process, "switched you to `B`");
        assert!(heard.contains("switched you to `B` by itself"), "{heard}");
        fixture.state.close_turn("w", false);
        fixture
            .state
            .turn_started("w", Some(&note_to_session(Agent::Claude, "A", "B")));
        assert_eq!(
            fixture.state.close_turn("w", false),
            crate::state::TurnClose::Reported
        );
    }

    /// A Grok Build session whose only wait is its usage-limit dialog is not waiting as far as the
    /// switch is concerned, at the first check and under the claim: ending the modal is the point.
    /// Another dialog, or the same one on another agent, still is a wait.
    #[tokio::test]
    async fn a_grok_session_showing_its_usage_limit_dialog_passes_every_wait_check() {
        const GROK_MODAL: &str = "You hit your weekly limit.\n1 (o) Upgrade tier\n2 (o) Buy more \
                                  credits\n3 (o) Try Again\nEnter :submit / Shift+x :dismiss\n";
        let fixture = Fixture::new("auto-switch-grok-modal", SessionStatus::Idle);
        let grok_session = Session {
            project_id: Some("p1".to_string()),
            has_conversation: true,
            agent_session_id: Some("grok-1".to_string()),
            ..crate::test_support::project_session("g", Agent::Grok, SessionStatus::Idle)
        };
        // Its turn stopped on the limit, as Grok 1.0.70 writes it.
        let record = fixture.dir.join("grok-default/sessions/proj/grok-1");
        std::fs::create_dir_all(&record).unwrap();
        std::fs::write(
            record.join("updates.jsonl"),
            concat!(
                r#"{"params":{"update":{"sessionUpdate":"user_message_chunk"}}}"#,
                "\n",
                r#"{"params":{"update":{"sessionUpdate":"turn_completed","#,
                r#""stop_reason":"error","agent_result":"API error (status 402)"}}}"#,
                "\n"
            ),
        )
        .unwrap();
        let grok_env = HashMap::from([(
            "GROK_HOME".to_string(),
            fixture
                .dir
                .join("grok-default")
                .to_string_lossy()
                .into_owned(),
        )]);
        fixture.state.store.insert_session(&grok_session).unwrap();
        let grok = crate::test_support::fake_live_session("g", Agent::Grok, 80, 24, "sleep 30");
        fixture.state.register_live(grok.clone());
        grok.on_output(GROK_MODAL.as_bytes());
        assert!(grok
            .look_for_dialog()
            .current
            .is_some_and(|dialog| dialog.is_usage_limit()));

        fixture
            .state
            .store
            .insert_account(&Account {
                id: "grok-a".to_string(),
                agent: Agent::Grok,
                name: "Grok A".to_string(),
                config_dir: fixture.dir.join("grok-a").to_string_lossy().into_owned(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
            })
            .unwrap();
        fixture.state.account_quotas.hold_in_dir_for_test(
            &AccountQuota {
                agent: Agent::Grok,
                account: Some("grok-a".to_string()),
                fetched_at: 1,
                plan: None,
                windows: vec![QuotaWindow {
                    kind: QuotaWindowKind::Weekly,
                    label: None,
                    used_percent: 10.0,
                    resets_at: Some(now_millis() + 86_400_000),
                    window_minutes: None,
                }],
                unavailable: None,
            },
            fixture.dir.join("grok-a").to_str().unwrap(),
        );

        assert_eq!(
            fixture.state.shown_status(&grok_session),
            SessionStatus::WaitingUser
        );
        assert_eq!(
            fixture.state.status_for_switch(&grok_session),
            SessionStatus::Idle
        );

        // The whole decision goes on to the switch itself, which refuses the Grok target that is
        // no initialised home: it is the wait that would have stopped it before that.
        let env = grok_env.clone();
        let err = try_switch(
            &fixture.state,
            "g",
            move || Ok(env),
            |_| anyhow::bail!("not reached"),
        )
        .await
        .expect_err("went on to the switch");
        assert!(
            format!("{err:#}").contains("is not a Grok home yet"),
            "{err:#}"
        );

        // Under the claim the wait is not what refuses it: the switch goes on to its own checks,
        // and refuses the Grok target there.
        let err = coordinator::switch_session_account_if(
            &fixture.state,
            "g",
            Some(Some("grok-a".to_string())),
            None,
            SwitchOnlyIf {
                not_waiting: true,
                running: true,
                usage_limited: true,
                ..SwitchOnlyIf::default()
            },
            None,
            LaunchEdit::default(),
            move || Ok(grok_env),
            |_| anyhow::bail!("not reached"),
        )
        .await
        .expect_err("refused further on");
        assert!(
            format!("{:#}", err.error).contains("is not a Grok home yet"),
            "{:#}",
            err.error
        );

        // Without the usage-limit part, and for a dialog of another kind, it is still a wait.
        let plain = coordinator::switch_session_account_if(
            &fixture.state,
            "g",
            Some(Some("grok-a".to_string())),
            None,
            SwitchOnlyIf {
                not_waiting: true,
                ..SwitchOnlyIf::default()
            },
            None,
            LaunchEdit::default(),
            || Ok(HashMap::new()),
            |_| anyhow::bail!("not reached"),
        )
        .await
        .expect_err("refused");
        assert!(plain.error.is::<NoLongerDue>());
        fixture.show_dialog();
        assert_eq!(
            fixture.state.status_for_switch(&fixture.worker()),
            SessionStatus::WaitingUser,
            "Claude Code's resume dialog is not a usage limit"
        );
    }

    /// The usage limit may reach the record a moment after the failed turn is reported: the record
    /// is read again for a while before the turn is judged not to have stopped on the limit.
    #[tokio::test]
    async fn a_usage_limit_that_reaches_the_record_late_still_switches() {
        let fixture = Fixture::new("auto-switch-late-record", SessionStatus::Idle);
        let record = fixture.dir.join("default-claude").join(CONVERSATION);
        fixture.end_on_usage_limit();
        let limit_text = std::fs::read_to_string(&record).unwrap();
        fixture.end_on_a_tool_call();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(500));
            std::fs::write(record, limit_text).unwrap();
        });

        fixture.failed_turn().await;

        assert_eq!(fixture.worker().account_id.as_deref(), Some("claude-a"));
    }

    /// Nothing is snapshotted or waited for while no account could take the session.
    #[tokio::test]
    async fn no_snapshot_is_taken_while_no_account_qualifies() {
        let fixture = Fixture::new("auto-switch-no-target", SessionStatus::Idle);
        fixture.hold(Some("claude-a"), 100.0);
        let taken = Arc::new(AtomicUsize::new(0));
        let counted = taken.clone();
        let env = fixture.env();

        try_switch(
            &fixture.state,
            "w",
            move || {
                counted.fetch_add(1, Ordering::SeqCst);
                Ok(env)
            },
            fixture.launch_failing(0),
        )
        .await
        .expect("nothing to do");

        assert_eq!(taken.load(Ordering::SeqCst), 0);
        assert_eq!(fixture.launched(), 0);
    }

    /// A session moved to another account while the record was waited for is left alone: the limit
    /// was found on the account it is no longer on.
    #[tokio::test]
    async fn a_session_moved_during_the_wait_is_left_alone() {
        let fixture = Fixture::new("auto-switch-moved-meanwhile", SessionStatus::Idle);
        let record = fixture.dir.join("default-claude").join(CONVERSATION);
        fixture.end_on_usage_limit();
        let limit_text = std::fs::read_to_string(&record).unwrap();
        fixture.end_on_a_tool_call();
        let (state, to) = (fixture.state.clone(), fixture.dir.join("claude-a"));
        let moved_meanwhile = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(500));
            // Moved first, then the limit shows: the wait ends on an account that has changed.
            state
                .set_session_account("w", Some("claude-a"), to.to_str())
                .unwrap();
            std::fs::write(record, limit_text).unwrap();
        });
        let env = fixture.env();

        try_switch(
            &fixture.state,
            "w",
            move || Ok(env),
            fixture.launch_failing(0),
        )
        .await
        .expect("left alone, not switched from the account it moved to");

        moved_meanwhile.join().unwrap();
        assert_eq!(fixture.worker().account_id.as_deref(), Some("claude-a"));
        assert_eq!(fixture.launched(), 0);
    }

    /// A relaunch that starts and dies within the settling time is the switch's failure to report;
    /// the owner is not also told that the process "ended on its own", only that the session could
    /// not be brought back, if it could not.
    #[tokio::test]
    async fn a_relaunch_that_dies_is_reported_as_the_outcome_not_as_ending_on_its_own() {
        let fixture = Fixture::new("auto-switch-dies", SessionStatus::Idle);
        let (started, launches) = (fixture.started.clone(), fixture.launches.clone());
        let env = fixture.env();
        // The first process starts and dies; the restore's starts and stays.
        let calls = Arc::new(AtomicUsize::new(0));
        let launch = move |request: LaunchRequest| {
            let launched = stand_in_launch(&started, &launches, || {})(request)?;
            if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                let process = launched.session.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(300));
                    process.terminate();
                });
            }
            Ok(launched)
        };

        try_switch(&fixture.state, "w", move || Ok(env), launch)
            .await
            .expect("brought back up, so not an error");

        assert_eq!(
            fixture.worker().account_id,
            None,
            "on its own account again"
        );
        assert!(fixture.state.live_session("w").is_some());
        let died = (*fixture.started.lock().unwrap()[0]).clone();
        for process in [&*fixture.worker_process, &died] {
            crate::test_support::exit_settled(process).await;
        }
        let heard = owner_heard(&fixture).await;
        assert!(!heard.contains("ended on its own"), "{heard}");
        assert!(!heard.contains("could neither switch"), "{heard}");
    }

    /// A turn that failed for another reason than the usage limit (an overload, a network error)
    /// is judged before any claim is taken: the session is never shown as switching, which would
    /// refuse an interrupt, a handoff or a rebinding for the seconds the record is waited on.
    #[tokio::test]
    async fn a_failed_turn_that_did_not_hit_the_limit_takes_no_claim() {
        let fixture = Fixture::new("auto-switch-no-claim", SessionStatus::Idle);
        fixture.end_on_a_tool_call();
        let mut events = fixture.state.subscribe();

        fixture.failed_turn().await;

        let mut shown_switching = false;
        while let Ok(event) = events.try_recv() {
            if let Event::SessionUpserted { session } = event {
                shown_switching |= session.switching_account;
            }
        }
        assert!(!shown_switching, "shown as switching its account");
        assert_eq!(fixture.launched(), 0);
        assert_eq!(fixture.worker().account_id, None);
    }

    /// Only Octoboard's own switch watches the process it launches: a switch someone asked for,
    /// whose relaunch dies, still leaves the owner to be told that the session ended on its own.
    #[tokio::test]
    async fn a_relaunch_that_dies_after_a_switch_asked_for_still_tells_the_owner() {
        let fixture = Fixture::new("auto-switch-dies-by-hand", SessionStatus::Idle);
        let (started, launches) = (fixture.started.clone(), fixture.launches.clone());
        let env = fixture.env();

        let failed = coordinator::switch_session_account_if(
            &fixture.state,
            "w",
            Some(Some("claude-a".to_string())),
            None,
            SwitchOnlyIf::default(),
            None,
            LaunchEdit::default(),
            move || Ok(env),
            move |request| {
                let launched = stand_in_launch(&started, &launches, || {})(request)?;
                let process = launched.session.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(300));
                    process.terminate();
                });
                Ok(launched)
            },
        )
        .await
        .expect_err("the relaunch did not stay up");
        let coded = failed.error.downcast_ref::<CodedError>().expect("coded");
        assert_eq!(coded.code, error_code::SWITCH_DID_NOT_COME_UP);

        let heard = output_containing(&fixture.owner, "ended on its own");
        assert!(
            heard.contains("`w`") && heard.contains("ended on its own"),
            "{heard}"
        );
    }

    /// A restore that finds another claim on the session waits for it to end rather than giving up
    /// and leaving the session down.
    #[tokio::test]
    async fn a_restore_waits_for_another_claim_on_the_session() {
        let fixture = Fixture::new("auto-switch-restore-claim", SessionStatus::Idle);
        let before = fixture.worker();
        let live = fixture.state.live_session("w").unwrap();
        let ending = tokio::task::spawn_blocking(move || live.end())
            .await
            .unwrap();
        assert!(ending);
        assert!(
            crate::test_support::within(crate::test_support::PATIENCE, {
                let state = fixture.state.clone();
                move || state.live_session("w").is_none()
            })
            .await
        );
        // A handover of branches holds the session for a moment.
        let claim = fixture.state.begin_switch("w").unwrap();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(600));
            drop(claim);
        });
        let (started, launches) = (fixture.started.clone(), fixture.launches.clone());

        coordinator::restore_after_failed_switch(&fixture.state, &before, None, move |request| {
            stand_in_launch(&started, &launches, || {})(request)
        })
        .await
        .expect("restored once the other claim ended");

        assert!(fixture.state.live_session("w").is_some());
    }

    /// A restore finds a session that runs already, or is archived, before it puts anything back:
    /// the account it is on stays.
    #[tokio::test]
    async fn a_restore_touches_nothing_of_a_session_that_is_not_down() {
        let fixture = Fixture::new("auto-switch-restore-running", SessionStatus::Idle);
        let before = Session {
            account_id: Some("claude-b".to_string()),
            ..fixture.worker()
        };

        let err = coordinator::restore_after_failed_switch(&fixture.state, &before, None, |_| {
            anyhow::bail!("not reached")
        })
        .await
        .expect_err("it runs");

        assert!(is_already_up(&err), "{err:#}");
        assert_eq!(fixture.worker().account_id, None);
        let mut archived = fixture.worker();
        archived.status = SessionStatus::Archived;
        fixture.state.store.update_session(&archived).unwrap();
        let live = fixture.state.live_session("w").unwrap();
        tokio::task::spawn_blocking(move || live.end())
            .await
            .unwrap();
        assert!(
            crate::test_support::within(crate::test_support::PATIENCE, {
                let state = fixture.state.clone();
                move || state.live_session("w").is_none()
            })
            .await
        );
        let err = coordinator::restore_after_failed_switch(&fixture.state, &before, None, |_| {
            anyhow::bail!("not reached")
        })
        .await
        .expect_err("archived");
        assert!(is_quiet(&err), "{err:#}");
        assert_eq!(fixture.worker().account_id, None);
    }

    /// A restore finds the session on neither the account it had nor the one the failed switch was
    /// to move it to: somebody moved it meanwhile, and the restore leaves it as it is.
    #[tokio::test]
    async fn a_restore_does_not_undo_an_account_change_somebody_else_made() {
        let fixture = Fixture::new("auto-switch-restore-moved", SessionStatus::Idle);
        let before = fixture.worker();
        let live = fixture.state.live_session("w").unwrap();
        tokio::task::spawn_blocking(move || live.end())
            .await
            .unwrap();
        assert!(
            crate::test_support::within(crate::test_support::PATIENCE, {
                let state = fixture.state.clone();
                move || state.live_session("w").is_none()
            })
            .await
        );
        let moved_dir = fixture.dir.join("claude-b");
        fixture
            .state
            .set_session_account("w", Some("claude-b"), moved_dir.to_str())
            .unwrap();

        coordinator::restore_after_failed_switch(&fixture.state, &before, Some("claude-a"), |_| {
            anyhow::bail!("not reached")
        })
        .await
        .expect("nothing to restore");

        assert_eq!(fixture.worker().account_id.as_deref(), Some("claude-b"));
        assert!(fixture.state.live_session("w").is_none());
    }

    /// A turn that fails while a claim on its session is held (the continued turn of a switch that
    /// has just relaunched it) is judged once the claim ends, and moves the session on.
    #[tokio::test]
    async fn a_turn_that_fails_under_a_claim_is_switched_once_it_ends() {
        let fixture = Fixture::new("auto-switch-claimed", SessionStatus::Idle);
        let claim = fixture.state.begin_switch("w").unwrap();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(600));
            drop(claim);
        });

        fixture.failed_turn().await;

        assert_eq!(fixture.worker().account_id.as_deref(), Some("claude-a"));
        assert_eq!(fixture.launched(), 1);
    }

    /// Failure params that share a name with ours are overridden, those of the default account's
    /// ids included, which are left out.
    #[tokio::test]
    async fn the_params_of_a_failed_switch_override_those_of_its_failure() {
        let fixture = Fixture::new("auto-switch-params", SessionStatus::Idle);
        let mut events = fixture.state.subscribe();
        let session = fixture.worker();
        let err = CodedError::raised(
            error_code::SWITCH_DID_NOT_COME_UP,
            "down",
            &[
                ("session", "other"),
                ("from_id", "theirs"),
                ("account_id", "theirs"),
                ("detail", "kept"),
            ],
        );

        tell_switch_failed(&fixture.state, &session, None, &err).await;

        let told = notices(
            &mut events,
            notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED,
        );
        let params = &told[0].1;
        assert_eq!(params.get("session").map(String::as_str), Some("w"));
        assert_eq!(params.get("detail").map(String::as_str), Some("kept"));
        assert!(!params.contains_key("from_id") && !params.contains_key("account_id"));
    }

    /// What is held of the accounts a session was switched off for a limit goes with the session
    /// when it is archived.
    #[tokio::test]
    async fn the_accounts_left_for_a_limit_are_forgotten_with_an_archived_session() {
        let fixture = Fixture::new("auto-switch-forget", SessionStatus::Idle);
        fixture.state.note_left_for_limit("w", None);
        assert_eq!(
            fixture
                .state
                .recently_left_for_limit("w", LEFT_FOR_LIMIT_FOR),
            [None]
        );

        coordinator::archive_session(&fixture.state, "w").unwrap();

        assert!(fixture
            .state
            .recently_left_for_limit("w", LEFT_FOR_LIMIT_FOR)
            .is_empty());
    }

    /// While the daemon shuts down nothing is told of a switch that could not be completed or
    /// undone, and nothing is started again.
    #[tokio::test]
    async fn nothing_is_told_of_a_switch_cut_short_by_the_shutdown() {
        let fixture = Fixture::new("auto-switch-shutdown", SessionStatus::Idle);
        let (logs, _guard) = LogBuffer::capture();
        let mut events = fixture.state.subscribe();
        let env = fixture.env();
        let state = fixture.state.clone();
        // The daemon starts to shut down once the switch has ended the process and is relaunching.
        let launch = move |_: LaunchRequest| -> Result<Launch> {
            let stopping = state.clone();
            std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .build()
                    .unwrap();
                runtime.block_on(stopping.stop_all_sessions());
            })
            .join()
            .unwrap();
            anyhow::bail!("the agent cannot be started")
        };

        try_switch(&fixture.state, "w", move || Ok(env), launch)
            .await
            .expect("cut short, and nobody is told");

        assert!(notices(
            &mut events,
            notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED
        )
        .is_empty());
        assert!(
            fixture.state.live_session("w").is_none(),
            "nothing was started again"
        );
        let logged = logs.text();
        assert!(!logged.contains("bringing it back up"), "{logged}");
    }

    /// The daemon shutting down during the restore's own relaunch ends that process too: the
    /// session is down, and nobody is told or logged to, since it is resumed at the next start.
    #[tokio::test]
    async fn nothing_is_told_of_a_restore_cut_short_by_the_shutdown() {
        let fixture = Fixture::new("auto-switch-shutdown-restore", SessionStatus::Idle);
        let (logs, _guard) = LogBuffer::capture();
        let mut events = fixture.state.subscribe();
        let env = fixture.env();
        let (state, calls) = (fixture.state.clone(), Arc::new(AtomicUsize::new(0)));
        let (started, launches) = (fixture.started.clone(), fixture.launches.clone());
        // The switch's relaunch fails; the restore's starts a process as the daemon begins to shut
        // down, which ends it.
        let launch = move |request: LaunchRequest| -> Result<Launch> {
            if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                anyhow::bail!("the agent cannot be started");
            }
            let stopping = state.clone();
            std::thread::spawn(move || {
                tokio::runtime::Builder::new_current_thread()
                    .build()
                    .unwrap()
                    .block_on(stopping.stop_all_sessions());
            })
            .join()
            .unwrap();
            stand_in_launch(&started, &launches, || {})(request)
        };

        try_switch(&fixture.state, "w", move || Ok(env), launch)
            .await
            .expect("cut short, and nobody is told");

        assert!(notices(
            &mut events,
            notice_code::ACCOUNT_SWITCH_AUTOMATICALLY_FAILED
        )
        .is_empty());
        // Only the switch's own failure, which came before the shutdown, is logged.
        let logged = logs.text();
        assert_eq!(logged.matches("WARN").count(), 1, "{logged}");
    }

    /// A turn that failed on one account is not judged against the account somebody moved the
    /// session to while it waited for a claim.
    #[tokio::test]
    async fn a_trigger_is_dropped_when_the_session_was_moved_during_the_claim_wait() {
        let fixture = Fixture::new("auto-switch-moved-in-claim", SessionStatus::Idle);
        let claim = fixture.state.begin_switch("w").unwrap();
        let (state, dir) = (fixture.state.clone(), fixture.dir.join("claude-b"));
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            state
                .set_session_account("w", Some("claude-b"), dir.to_str())
                .unwrap();
            drop(claim);
        });

        fixture.failed_turn().await;

        assert_eq!(fixture.worker().account_id.as_deref(), Some("claude-b"));
        assert_eq!(fixture.launched(), 0);
    }

    /// A second trigger that arrives while a real automatic switch still holds its claim waits for
    /// it, and then does not pick the account the first one left.
    #[tokio::test]
    async fn a_second_trigger_under_a_switchs_claim_does_not_pick_the_account_just_left() {
        let fixture = Fixture::new("auto-switch-second", SessionStatus::Idle);
        fixture.hold(Some("claude-b"), 50.0);

        let second = async {
            // The turn it continues fails once the session is relaunched on its new account, while
            // the claim is still held.
            while !(fixture.state.is_launching("w")
                && fixture.worker().account_id.as_deref() == Some("claude-a"))
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            fixture.failed_turn().await;
        };
        tokio::join!(fixture.failed_turn(), second);

        // The default account still reads as having quota, but the first switch left it.
        assert_eq!(fixture.worker().account_id.as_deref(), Some("claude-b"));
        assert_eq!(fixture.launched(), 2);
    }

    /// The owner is not told that a session waits for the user while the dialog on its screen is
    /// its agent's usage limit, which is no wait; any other dialog is.
    #[tokio::test]
    async fn the_owner_is_not_told_of_a_wait_by_a_usage_limit_dialog() {
        const GROK_MODAL: &str = "You hit your weekly limit.\n1 (o) Upgrade tier\n2 (o) Buy more \
                                  credits\n3 (o) Try Again\nEnter :submit / Shift+x :dismiss\n";
        let fixture = Fixture::new("auto-switch-limit-dialog", SessionStatus::Idle);
        fixture
            .state
            .store
            .insert_session(&Session {
                project_id: Some("p1".to_string()),
                bound_to: Some("owner".to_string()),
                ..crate::test_support::project_session("g", Agent::Grok, SessionStatus::Idle)
            })
            .unwrap();
        let grok = crate::test_support::fake_live_session("g", Agent::Grok, 80, 24, "sleep 30");
        fixture.state.register_live(grok.clone());
        grok.on_output(GROK_MODAL.as_bytes());
        let limit = grok.look_for_dialog().current;
        assert!(limit.is_some_and(|dialog| dialog.is_usage_limit()));

        crate::agent_dialogs::react_to_dialog(&fixture.state, "g", limit);
        tokio::time::sleep(Duration::from_millis(300)).await;

        assert!(
            fixture.state.claim_waiting_notice("g"),
            "no notice was taken for it"
        );
        let heard = owner_heard(&fixture).await;
        assert!(!heard.contains("is waiting for the user"), "{heard}");

        // Another dialog is a wait, and its owner is told.
        fixture.show_dialog();
        let resume = fixture.worker_process.look_for_dialog().current;
        crate::agent_dialogs::react_to_dialog(&fixture.state, "w", resume);
        let heard = output_containing(&fixture.owner, "is waiting for the user");
        assert!(heard.contains("is waiting for the user"), "{heard}");
    }

    /// A name that cannot be read is replaced by the account's id, or by the standard name of the
    /// default account, which has none.
    #[test]
    fn an_account_name_that_cannot_be_read_falls_back_to_its_id() {
        assert_eq!(fallback_account_name(Some("claude-a")), "claude-a");
        assert_eq!(fallback_account_name(None), DEFAULT_ACCOUNT_NAME);
    }

    /// Only a refusal that says the time was wrong is silent.
    #[test]
    fn only_a_refusal_of_the_time_is_silent() {
        for code in [
            error_code::SESSION_ALREADY_STARTING,
            error_code::ACCOUNT_SHARING_IN_PROGRESS,
            error_code::ACCOUNT_DISABLED,
            error_code::ACCOUNT_QUOTA_EXHAUSTED,
            error_code::ACCOUNT_NOT_SIGNED_IN,
            error_code::UNKNOWN_ACCOUNT,
        ] {
            assert!(is_wrong_time(&CodedError::raised(code, "busy", &[])));
        }
        assert!(is_wrong_time(&NoLongerDue("moved").into()));
        assert!(!is_wrong_time(&CodedError::raised(
            error_code::SESSION_ARCHIVED,
            "archived",
            &[]
        )));
        assert!(!is_wrong_time(&anyhow::anyhow!(
            "the agent cannot be started"
        )));
    }
}
