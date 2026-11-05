//! Changing an account's sharing of its agent's default configuration as an operation: refused or
//! carried out at once under the account's lock, ending the account's idle sessions first and
//! resuming them afterwards, with its progress told entry by entry; and turning sharing off before
//! an account is removed or repointed. What a change does to the directories, the locks it takes
//! and the progress it tells are `crate::accounts::shared_config`'s.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

use crate::accounts::shared_config::{
    account_in_use, carry_out, default_dir, entries, in_use, lock_account, lock_agent,
    not_shareable, own_conversations, require_file_links, sharing_failed, sharing_in_progress,
    still_sharing, try_lock_agent, Changing, Locked, Refused,
};
use crate::hostfs::dir_key;
use crate::hostfs::is_same_path;
use crate::messaging::texts;
use crate::processes::ledger::HeldForLaunch;
use crate::protocol::{
    error_code, notice_code, Account, CodedError, Notice, ProcessTree, ProviderOperation,
    SessionStatus, SharingCause, SharingFailure, SharingPhase, SharingProgress,
};
use crate::state::{broadcast_settings, AccountSharingLock, AppState};

/// Records `share` as the account's choice and carries it out at once, answering once it has
/// ended; `account` is the account's id, and `None` names the default account, which has no
/// choice to make, being the directory the others share. Asking again for the choice already
/// recorded retries a change that did not complete, and does nothing otherwise.
///
/// Refused, with nothing recorded: turning sharing on where the host cannot make file links
/// (`sharing_unavailable_on_host`), for an account of a provider without the
/// `config_sharing` capability (`provider_unsupported`), for the default account and for an account
/// whose directory is the default account's under another spelling
/// (`default_account_not_shareable`), while sessions of the account run or start or it is being
/// signed in (`account_in_use`), and while a change of it runs already
/// (`account_sharing_in_progress`). A change that fails part-way is answered with what it left
/// where (`account_sharing_failed`) and stays incomplete.
pub async fn set(state: &Arc<AppState>, account: Option<String>, share: bool) -> Result<()> {
    if share {
        require_file_links(crate::platform::fs::file_links_available())?;
    }
    let Some(id) = account else {
        return Err(not_shareable(None));
    };
    loop {
        let account = state.store.require_account(&id)?;
        if share {
            CodedError::require_offered(
                account.agent,
                ProviderOperation::ConfigSharing,
                account
                    .agent
                    .descriptor()
                    .capabilities
                    .config_sharing
                    .then_some(()),
            )?;
        }
        if let Some(locked) = state.sharing_locked(&id) {
            return Err(sharing_in_progress(&locked));
        }
        if is_done(&account, share) {
            return Ok(());
        }
        if share {
            let default = default_dir(state, account.agent).await?;
            let dir = PathBuf::from(&account.config_dir);
            if tokio::task::spawn_blocking(move || is_same_path(&dir, &default)).await? {
                return Err(not_shareable(Some(&account)));
            }
        }
        let mut locked = lock_account(state, &account, &account_in_use)?;
        // Judged before the lock: an account repointed meanwhile is judged again.
        let current = state.store.require_account(&id)?;
        if !same_directory(&current, &account) {
            continue;
        }
        if is_done(&current, share) {
            return Ok(());
        }
        return change(state, &mut locked, &current, share, &account_in_use).await;
    }
}

/// Whether the account's choice is `share` already, the last change toward it complete.
fn is_done(account: &Account, share: bool) -> bool {
    account.share_default_config == share && !account.share_default_config_incomplete
}

/// Whether the two records of an account name the same directory of the same agent.
fn same_directory(a: &Account, b: &Account) -> bool {
    a.agent == b.agent && a.config_dir == b.config_dir
}

/// Runs `account`'s change to `share` under `locked`. It waits for the agent's directory, claims
/// the account's idle sessions (one that turned working refuses the change with `refused`, before
/// anything is stopped or recorded) and ends their processes; then it records the choice
/// as incomplete, waits for the short CLI reads running under the account's directory to end,
/// processes every entry, telling the progress as each is reached, and records the outcome; and
/// last, whatever the outcome, it resumes the sessions it ended, onto the directory as it stands.
async fn change(
    state: &Arc<AppState>,
    locked: &mut Locked,
    account: &Account,
    share: bool,
    refused: &Refused,
) -> Result<()> {
    let start = state.store.account_sharing_start(&account.id)?;
    let conversations = match (&start, share) {
        (Some(_), false) => own_conversations(state, account)?,
        _ => Vec::new(),
    };
    let total = match (&start, share) {
        // Sharing was never started there: there is nothing to undo.
        (None, false) => 0,
        _ => entries(account.agent, conversations.len()),
    };
    let progress = move |phase, done, item: Option<&str>| SharingProgress {
        share,
        phase,
        done,
        total,
        item: item.map(str::to_string),
    };
    let set_phase =
        |phase, done| state.set_sharing_progress(&account.id, Some(progress(phase, done, None)));
    // Until the choice is recorded, the progress is all the change has told: refused or stopped
    // before that, it leaves the last failure as it was.
    let changing = Changing::new(state, &account.id);
    let agent = match try_lock_agent(state, account.agent) {
        Some(agent) => agent,
        None => {
            set_phase(SharingPhase::Waiting, 0);
            lock_agent(state, account.agent).await
        }
    };

    // Judged with the claims, in one step, so the account's idle sessions stay usable while the
    // change waits; one that turned working since the account was locked refuses it.
    locked.lock.claim_idle(|running| {
        let uses = in_use(account, running);
        match uses.working {
            0 => Ok(uses.idle),
            working => Err(refused(account, working, false)),
        }
    })?;
    let to_stop = locked.lock.restarting();
    set_phase(
        match to_stop.is_empty() {
            true => SharingPhase::Changing,
            false => SharingPhase::Stopping,
        },
        0,
    );
    let (stopped, not_stopped) = stop(state, account, &to_stop).await;
    let started = not_stopped.is_none();
    let outcome = match not_stopped {
        Some(err) => Err(err),
        None => {
            async {
                state.store.set_account_sharing(&account.id, share, true)?;
                broadcast_settings(state)?;
                state.start_sharing_change(&account.id, progress(SharingPhase::Changing, 0, None));
                if let Some(hold) = &locked.hold {
                    let waiter = hold.waiter();
                    tokio::task::spawn_blocking(move || waiter.wait_for_reads()).await?;
                }
                let report = {
                    let (state, id) = (state.clone(), account.id.clone());
                    move |done, item: &str| {
                        let progress = progress(SharingPhase::Changing, done, Some(item));
                        state.set_sharing_progress(&id, Some(progress))
                    }
                };
                let changed = Account {
                    share_default_config: share,
                    ..account.clone()
                };
                carry_out(state, &changed, start, conversations, report).await
            }
            .await
        }
    };
    // Given up first: a resume repairs the account's links under it.
    drop(agent);
    if !stopped.is_empty() {
        set_phase(SharingPhase::Resuming, total);
    }
    for session in stopped {
        resume(state, &mut locked.lock, account, session).await;
    }
    if !started {
        // Nothing was recorded or changed: the change did not start.
        drop(changing);
        return outcome.map(|_| ());
    }
    match outcome {
        Ok(Ok(())) => {
            changing.end(None);
            Ok(())
        }
        Ok(Err((failures, cause, detail))) => {
            tracing::warn!(account = %account.id, ?failures, "changing an account's sharing failed part-way");
            let err = sharing_failed(account, share, &failures, cause, &detail);
            changing.end(Some(SharingFailure {
                share,
                cause,
                items: failures,
                detail,
            }));
            Err(err)
        }
        Err(err) => {
            changing.end(Some(SharingFailure {
                share,
                cause: SharingCause::Other,
                items: Vec::new(),
                detail: err.to_string(),
            }));
            Err(err)
        }
    }
}

/// A session the change ended: what its process started is held for the relaunch (`held`, none
/// when ending the process failed), `ended` is whether a running process was stopped, and
/// `ended_trees` the trees it had running that the change ended (see
/// [`super::restart::trees_ended`]).
struct Stopped {
    id: String,
    held: Option<HeldForLaunch>,
    ended: bool,
    ended_trees: Vec<ProcessTree>,
}

/// Ends the processes of the sessions `ids`, which the change has claimed, as a switch does, with
/// what they started carried over to the relaunch, and waits for every one to be recorded down.
/// Answers those it ended, and, for the first that would not end, a `sharing_session_did_not_stop`:
/// the change must not go on then, and the ones ended are resumed.
async fn stop(
    state: &Arc<AppState>,
    account: &Account,
    ids: &[String],
) -> (Vec<Stopped>, Option<anyhow::Error>) {
    let mut ending = Vec::new();
    for id in ids {
        if state.live_session(id).is_some() {
            let (state, id) = (state.clone(), id.clone());
            let task = {
                let id = id.clone();
                tokio::spawn(
                    async move { super::restart::stop_carrying_processes(&state, &id).await },
                )
            };
            ending.push((id, task));
        }
    }
    let mut stopped = Vec::new();
    for (id, handle) in ending {
        let ended = handle.await.unwrap_or_else(|err| {
            Err(anyhow::anyhow!(
                "the task ending the session panicked: {err}"
            ))
        });
        stopped.push(match ended {
            Ok((held, ended, running)) => Stopped {
                id,
                held: Some(held),
                ended,
                // Compared below, once the process is recorded down.
                ended_trees: if ended { running } else { Vec::new() },
            },
            Err(err) => {
                tracing::warn!(session = %id, %err, "ending a session's process for a sharing change failed");
                Stopped {
                    id,
                    held: None,
                    ended: false,
                    ended_trees: Vec::new(),
                }
            }
        });
    }
    let mut first = None;
    for session in &mut stopped {
        if super::restart::wait_until_stopped(state, &session.id)
            .await
            .is_err()
        {
            first.get_or_insert_with(|| session_did_not_stop(account, &session.id));
            session.ended_trees.clear();
            continue;
        }
        let running = std::mem::take(&mut session.ended_trees);
        session.ended_trees = super::restart::trees_ended(state, &session.id, running).await;
    }
    (stopped, first)
}

fn session_did_not_stop(account: &Account, session: &str) -> anyhow::Error {
    CodedError::raised(
        error_code::SHARING_SESSION_DID_NOT_STOP,
        format!(
            "one of account `{}`'s idle sessions did not end in time, so its sharing of the \
             default configuration was not changed",
            account.name
        ),
        &[
            ("account", &account.id),
            ("name", &account.name),
            ("session", session),
        ],
    )
}

/// Resumes `id`, one of the sessions the change ended and waited for, on its conversation, and
/// lets it go. A session archived or deleted meanwhile is let go as it is. One whose relaunch
/// fails is left interrupted, and one never seen down (its process may still be running) is not
/// relaunched; either is told as a `sharing_resume_failed` notice, without failing the change.
/// What the session's process started, held for the relaunch, is given up once that is over: kept
/// among its project's processes when the session was not relaunched after the change ended its
/// process (see [`super::restart::give_up_what_a_restart_held`]).
async fn resume(
    state: &Arc<AppState>,
    lock: &mut AccountSharingLock,
    account: &Account,
    stopped: Stopped,
) {
    let Stopped {
        id,
        held,
        ended,
        ended_trees,
    } = stopped;
    let id = id.as_str();
    let mut relaunched = false;
    let record = state.store.get_session(id).ok().flatten();
    let session_exists = record.is_some();
    let still_running = record.as_ref().is_some_and(|session| {
        state.live_session(id).is_some() || !session.status.has_no_process()
    });
    let resumed = match (record, lock.claim(id)) {
        _ if still_running => Err(anyhow::anyhow!("its process was not seen to end")),
        (Some(session), Some(_)) if session.status == SessionStatus::Interrupted => {
            lock.release_queue_for_relaunch(id);
            match lock.claim(id) {
                Some(claim) => relaunch(state, claim, id)
                    .await
                    .inspect(|_| relaunched = true),
                None => Ok(()),
            }
        }
        // Archived or deleted meanwhile: let go as it is.
        _ => Ok(()),
    };
    // Told to the session once it is relaunched; the change never continues a turn, as the
    // session was idle.
    if relaunched {
        if let Some(ended) = texts::processes_ended_by_restart(&ended_trees) {
            super::restart::tell_what_a_restart_ended(state, id, ended).await;
        }
    }
    if let Some(held) = held {
        // A process not seen to end keeps what it started, as when a switch's wait fails: nothing
        // is kept for the user then, and its trees stay its own, stopped when it ends. A session
        // deleted meanwhile has no project to keep them in.
        let kept = !relaunched && ended && !still_running && session_exists;
        super::restart::give_up_what_a_restart_held(state, id, held, kept).await;
    }
    if let Err(err) = resumed {
        tracing::warn!(session = %id, %err, still_running, "resuming a session after a sharing change failed");
        let detail = format!("{err:#}");
        let message = if still_running {
            format!(
                "the session was being ended while account `{}`'s sharing of the default \
                 configuration changed, and was not seen to end, so it was not resumed",
                account.name
            )
        } else {
            format!(
                "the session was ended while account `{}`'s sharing of the default \
                 configuration changed, and could not be resumed afterwards: {detail}",
                account.name
            )
        };
        let notice = Notice::new(
            notice_code::SHARING_RESUME_FAILED,
            message,
            &[
                ("account", &account.id),
                ("name", &account.name),
                ("detail", &detail),
                ("still_running", &still_running.to_string()),
            ],
        );
        state.broadcast(notice.about(id));
    }
    lock.end_restart(id);
}

/// Relaunches `id` as a resume does, under `claim`.
async fn relaunch(
    state: &Arc<AppState>,
    claim: &crate::state::LaunchClaim,
    id: &str,
) -> Result<()> {
    #[cfg(test)]
    {
        let launch = state
            .account_sharing
            .relaunch
            .lock()
            .expect("relaunch seam lock poisoned")
            .clone();
        if let Some(launch) = launch {
            return super::launch::relaunch_session_with(
                state,
                claim,
                id,
                None,
                false,
                None,
                None,
                move |request| launch(request),
            )
            .await;
        }
    }
    super::launch::relaunch_session_with(
        state,
        claim,
        id,
        None,
        false,
        None,
        None,
        crate::session::term::launch,
    )
    .await
}

/// Before account `id` is deleted: an account that shares, or whose last change is incomplete,
/// has its sharing turned off first, so its directory is not left linked into the default one
/// with nothing to undo it. Turned off at once, under the account's lock and with its progress
/// told, as `set_account_sharing` does; refused while sessions of it run or start or it is being
/// signed in (`account_still_sharing`), while a change of it runs, and when turning it off
/// fails. The answer keeps the account locked, for the deletion to hold while it removes it.
pub async fn before_delete(state: &Arc<AppState>, id: &str) -> Result<Option<Locked>> {
    turn_off_first(state, id, "remove it").await
}

/// Before account `id` is pointed at `config_dir`: as [`before_delete`], when the directory
/// changes, the sharing being undone in the directory it was carried out on.
pub async fn before_repoint(
    state: &Arc<AppState>,
    id: &str,
    config_dir: &str,
) -> Result<Option<Locked>> {
    let unchanged = state
        .store
        .get_account(id)?
        .is_none_or(|account| dir_key(&account.config_dir) == dir_key(config_dir));
    if unchanged {
        return Ok(None);
    }
    turn_off_first(state, id, "point it at another directory").await
}

async fn turn_off_first(
    state: &Arc<AppState>,
    id: &str,
    then: &'static str,
) -> Result<Option<Locked>> {
    loop {
        let Some(account) = state.store.get_account(id)? else {
            return Ok(None);
        };
        let shares = !is_done(&account, false);
        let locked = if shares {
            lock_account(state, &account, &|account: &Account, _, _| {
                still_sharing(account, then)
            })?
        } else {
            // Nothing to undo; locked only so that no change of it starts before it is let go.
            let (_, lock) = state.lock_account_sharing(&account, |locked, _| match locked {
                true => Err(sharing_in_progress(&account)),
                false => Ok(()),
            })?;
            Locked { hold: None, lock }
        };
        // Judged before the lock: an account changed meanwhile is judged again.
        let mut locked = locked;
        let Some(current) = state.store.get_account(id)? else {
            return Ok(Some(locked));
        };
        let shares_now = !is_done(&current, false);
        if !same_directory(&current, &account) || shares_now != shares {
            continue;
        }
        if shares {
            let refused = |account: &Account, _, _| still_sharing(account, then);
            change(state, &mut locked, &current, false, &refused).await?;
        }
        return Ok(Some(locked));
    }
}
