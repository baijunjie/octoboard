//! Changing an account's sharing: refused or carried out at once under the account's lock, with
//! its progress told entry by entry, and turning sharing off before an account is removed or
//! repointed.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;

use super::link::{self, Cause, DirsError, Failure};
use super::{
    account_in_use, cli_reads, default_dir, describe, in_use, is_same_directory, moves,
    not_shareable, sharing_failed, sharing_in_progress, status, still_sharing, take_back,
};
use crate::account_dirs::account_dir_key;
use crate::protocol::{
    error_code, notice_code, Account, CodedError, Notice, SessionStatus, SharingFailure,
    SharingPhase, SharingProgress,
};
use crate::state::{AccountSharingLock, AppState};
use crate::store::SharingStart;

/// Records `share` as the account's choice and carries it out at once, answering once it has
/// ended; `account` is the account's id, and `None` names the default account, which has no
/// choice to make, being the directory the others share. Asking again for the choice already
/// recorded retries a change that did not complete, and does nothing otherwise.
///
/// Refused, with nothing recorded: for the default account and for an account whose directory is
/// the default account's under another spelling (`default_account_not_shareable`), while
/// sessions of the account run or start or it is being signed in (`account_in_use`), and while a
/// change of it runs already (`account_sharing_in_progress`). A change that fails part-way is
/// answered with what it left where (`account_sharing_failed`) and stays incomplete.
pub async fn set(state: &Arc<AppState>, account: Option<String>, share: bool) -> Result<()> {
    let Some(id) = account else {
        return Err(not_shareable(None));
    };
    loop {
        let account = state
            .store
            .get_account(&id)?
            .ok_or_else(|| CodedError::unknown_account(&id))?;
        if let Some(locked) = state.sharing_locked(&id) {
            return Err(sharing_in_progress(&locked));
        }
        if is_done(&account, share) {
            return Ok(());
        }
        if share {
            let default = default_dir(state, account.agent).await?;
            let dir = PathBuf::from(&account.config_dir);
            if tokio::task::spawn_blocking(move || is_same_directory(&dir, &default)).await? {
                return Err(not_shareable(Some(&account)));
            }
        }
        let mut locked = lock_account(state, &account, &account_in_use)?;
        // Judged before the lock: an account repointed meanwhile is judged again.
        let current = state
            .store
            .get_account(&id)?
            .ok_or_else(|| CodedError::unknown_account(&id))?;
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

/// What a change holds of its account until dropped: the lock its sessions' launches are refused
/// by, and, for a change that moves files, the hold on its directory against agent CLI runs.
pub struct Locked {
    hold: Option<cli_reads::Hold>,
    lock: AccountSharingLock,
}

/// Why a change is refused while its account is in use: given the account, how many of its
/// sessions work, and whether it is being signed in.
type Refused = dyn Fn(&Account, usize, bool) -> anyhow::Error + Sync;

/// Locks `account` for a change, refusing with `refused` while sessions of it work or a long CLI
/// run (a sign-in) runs under its directory, and with `account_sharing_in_progress` while it is
/// locked already. In one step with the check, so a launch claimed after it is refused by the
/// lock. Short CLI reads are held back from then on.
fn lock_account(state: &Arc<AppState>, account: &Account, refused: &Refused) -> Result<Locked> {
    let dir = PathBuf::from(&account.config_dir);
    let (hold, lock) = state.lock_account_sharing(account, |locked, running| {
        if locked {
            return Err(sharing_in_progress(account));
        }
        let uses = in_use(account, running);
        if uses.working > 0 || uses.signing_in {
            return Err(refused(account, uses.working, uses.signing_in));
        }
        // The hold is refused only while a long run is going.
        cli_reads::hold(&dir).ok_or_else(|| refused(account, 0, true))
    })?;
    Ok(Locked {
        hold: Some(hold),
        lock,
    })
}

/// A change's progress, until it ends: dropped without [`Changing::end`] — refused or stopped before
/// the choice was recorded, panicked, or its future dropped — it ends the progress all the same,
/// leaving the last failure as it was.
struct Changing<'a> {
    state: &'a AppState,
    account: &'a str,
    ended: bool,
}

impl Changing<'_> {
    fn end(mut self, failure: Option<SharingFailure>) {
        self.ended = true;
        status::end_change(self.state, self.account, failure);
    }
}

impl Drop for Changing<'_> {
    fn drop(&mut self) {
        if !self.ended {
            status::set_progress(self.state, self.account, None);
        }
    }
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
        (Some(_), false) => take_back::own_conversations(state, account)?,
        _ => Vec::new(),
    };
    let total = match (&start, share) {
        // Sharing was never started there: there is nothing to undo.
        (None, false) => 0,
        _ => link::entries(account.agent, conversations.len()),
    };
    let progress = move |phase, done, item: Option<&str>| SharingProgress {
        share,
        phase,
        done,
        total,
        item: item.map(str::to_string),
    };
    let set_phase =
        |phase, done| status::set_progress(state, &account.id, Some(progress(phase, done, None)));
    // Until the choice is recorded, the progress is all the change has told: refused or stopped
    // before that, it leaves the last failure as it was.
    let changing = Changing {
        state,
        account: &account.id,
        ended: false,
    };
    let agent = match state.account_sharing.locks.try_lock(account.agent) {
        Some(agent) => agent,
        None => {
            set_phase(SharingPhase::Waiting, 0);
            state.account_sharing.locks.lock(account.agent).await
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
                crate::coordinator::broadcast_settings(state)?;
                status::start_change(
                    state,
                    &account.id,
                    progress(SharingPhase::Changing, 0, None),
                );
                if let Some(hold) = &locked.hold {
                    let waiter = hold.waiter();
                    tokio::task::spawn_blocking(move || waiter.wait_for_reads()).await?;
                }
                let report = {
                    let (state, id) = (state.clone(), account.id.clone());
                    move |done, item: &str| {
                        let progress = progress(SharingPhase::Changing, done, Some(item));
                        status::set_progress(&state, &id, Some(progress))
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
    for id in &stopped {
        resume(state, &mut locked.lock, account, id).await;
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
                cause: Cause::Other,
                items: Vec::new(),
                detail: err.to_string(),
            }));
            Err(err)
        }
    }
}

/// Ends the processes of the sessions `ids`, which the change has claimed, and waits for every one
/// to be recorded down. Answers those it ended, and, for the first that would not end, a
/// `sharing_session_did_not_stop`: the change must not go on then, and the ones ended are resumed.
async fn stop(
    state: &Arc<AppState>,
    account: &Account,
    ids: &[String],
) -> (Vec<String>, Option<anyhow::Error>) {
    let mut stopped = Vec::new();
    let mut ending = Vec::new();
    for id in ids {
        if let Some(live) = state.live_session(id) {
            ending.push((id.clone(), crate::coordinator::stop_process(live)));
            stopped.push(id.clone());
        }
    }
    for (id, handle) in ending {
        if let Err(err) = handle.await {
            tracing::warn!(session = %id, %err, "ending a session's process for a sharing change failed");
        }
    }
    let mut first = None;
    for id in &stopped {
        if wait_until_down(state, id).await.is_err() && first.is_none() {
            first = Some(session_did_not_stop(account, id));
        }
    }
    (stopped, first)
}

/// Waits for `id`, a session the change ended, to be recorded down.
async fn wait_until_down(state: &Arc<AppState>, id: &str) -> Result<()> {
    #[allow(unused_mut)]
    let mut timeout = crate::coordinator::SWITCH_STOP_TIMEOUT;
    #[cfg(test)]
    if let Some(shorter) = *state
        .account_sharing
        .stop_timeout
        .lock()
        .expect("stop timeout seam lock poisoned")
    {
        timeout = shorter;
    }
    crate::coordinator::wait_until_down_within(state, id, timeout).await
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
async fn resume(state: &Arc<AppState>, lock: &mut AccountSharingLock, account: &Account, id: &str) {
    let record = state.store.get_session(id).ok().flatten();
    let still_running = record
        .as_ref()
        .is_some_and(|session| state.live_session(id).is_some() || !session.status.is_dormant());
    let resumed = match (record, lock.claim(id)) {
        _ if still_running => Err(anyhow::anyhow!("its process was not seen to end")),
        (Some(session), Some(_)) if session.status == SessionStatus::Interrupted => {
            lock.release_queue_for_relaunch(id);
            match lock.claim(id) {
                Some(claim) => relaunch(state, claim, id).await,
                None => Ok(()),
            }
        }
        // Archived or deleted meanwhile: let go as it is.
        _ => Ok(()),
    };
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
            return crate::coordinator::relaunch_session_with(
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
    crate::coordinator::relaunch_session_with(
        state,
        claim,
        id,
        None,
        false,
        None,
        None,
        crate::term::launch,
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
        .is_none_or(|account| account_dir_key(&account.config_dir) == account_dir_key(config_dir));
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

/// What a change left undone: each entry not brought in line, why as a whole (the entries' one
/// cause, or why the change could not start), and the whole of it in words.
type Undone = (Vec<Failure>, Cause, String);

/// Carries out `account`'s change to its recorded choice; the caller holds the account's lock and
/// the agent's. `start` is what turning sharing on recorded, and `conversations` those of the
/// account's own sessions, for turning it off. `report` is told each entry as it is reached.
///
/// Turning sharing on records when it starts (the stamp its backups are named with, and the
/// default directory its links lead into) before touching anything, so a retry backs up under
/// the same names and turning it off restores exactly those. A retry into another default
/// directory than the recorded one is refused: the links already made lead into that one.
/// Turning it off works from that record: with none, nothing was ever linked and there is nothing
/// to undo; with one, the links are recognised by where that default directory is, even when the
/// default has moved since or is gone.
async fn carry_out(
    state: &Arc<AppState>,
    account: &Account,
    start: Option<SharingStart>,
    conversations: Vec<take_back::Conversation>,
    mut report: impl FnMut(usize, &str) + Send + 'static,
) -> Result<Result<(), Undone>> {
    let dir = PathBuf::from(&account.config_dir);
    let share = account.share_default_config;
    let agent = account.agent;
    let current = default_dir(state, agent).await?;
    let changed_meanwhile = || {
        Ok(Err((
            Vec::new(),
            Cause::Other,
            "the account was changed while its sharing was being changed".to_string(),
        )))
    };
    let outcome = if share {
        let start = match start {
            Some(start) => start,
            None => {
                let resolved = {
                    let current = current.clone();
                    tokio::task::spawn_blocking(move || moves::resolve_or_spelled(&current)).await?
                };
                let start = SharingStart {
                    stamp: moves::new_stamp(),
                    default_dir: resolved.to_string_lossy().into_owned(),
                    completed: false,
                };
                if !state
                    .store
                    .start_account_sharing(&account.id, &account.config_dir, &start)?
                {
                    return changed_meanwhile();
                }
                start
            }
        };
        tokio::task::spawn_blocking(move || {
            let recorded = moves::resolve_or_spelled(Path::new(&start.default_dir));
            if moves::resolve_or_spelled(&current) != recorded {
                return Err(DirsError::DefaultMoved(format!(
                    "the default configuration is now `{}`, not `{}`, the one sharing was \
                     started into; turn sharing off, then on again",
                    current.display(),
                    recorded.display()
                )));
            }
            let dirs = link::Dirs::new(&dir, &recorded, true)?;
            Ok(dirs.map_or_else(Vec::new, |dirs| {
                link::turn_on(agent, &dirs, &start.stamp, start.completed, &mut report)
            }))
        })
        .await?
    } else {
        match start {
            None => Ok(Vec::new()),
            Some(start) => {
                tokio::task::spawn_blocking(move || {
                    match link::Dirs::for_undoing(&dir, Path::new(&start.default_dir), &current) {
                        // The directory never linked anything to itself: nothing to undo.
                        Err(DirsError::SameAsDefault) | Ok(None) => Ok(Vec::new()),
                        Err(err) => Err(err),
                        Ok(Some(dirs)) => Ok(link::turn_off(
                            agent,
                            &dirs,
                            &start.stamp,
                            &conversations,
                            &mut report,
                        )),
                    }
                })
                .await?
            }
        }
    };
    let failures = match outcome {
        Ok(failures) => failures,
        Err(DirsError::SameAsDefault) => return Err(not_shareable(Some(account))),
        Err(err) => {
            let (cause, detail) = err.cause_and_detail();
            return Ok(Err((Vec::new(), cause, detail)));
        }
    };
    if !failures.is_empty() {
        let (cause, detail) = (Cause::of_all(&failures), describe(&failures));
        return Ok(Err((failures, cause, detail)));
    }
    if !state
        .store
        .finish_account_sharing(&account.id, &account.config_dir, share)?
    {
        return changed_meanwhile();
    }
    crate::coordinator::broadcast_settings(state)?;
    Ok(Ok(()))
}
