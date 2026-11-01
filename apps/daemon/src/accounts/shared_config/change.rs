//! The mechanism of a change of an account's sharing, which `crate::coordinator::account_sharing`
//! runs as an operation: locking the account for it, the progress it tells until it ends, and
//! carrying it out on the account's directory.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;

use super::link::{self, Cause, DirsError, Failure};
use super::{
    cli_reads, default_dir, describe, in_use, moves, not_shareable, sharing_in_progress, take_back,
};
use crate::protocol::{Account, SharingFailure};
use crate::state::{broadcast_settings, AccountSharingLock, AppState};
use crate::store::SharingStart;

/// What a change holds of its account until dropped: the lock its sessions' launches are refused
/// by, and, for a change that moves files, the hold on its directory against agent CLI runs.
pub struct Locked {
    pub(crate) hold: Option<cli_reads::Hold>,
    pub(crate) lock: AccountSharingLock,
}

/// Why a change is refused while its account is in use: given the account, how many of its
/// sessions work, and whether it is being signed in.
pub(crate) type Refused = dyn Fn(&Account, usize, bool) -> anyhow::Error + Sync;

/// Locks `account` for a change, refusing with `refused` while sessions of it work or a long CLI
/// run (a sign-in) runs under its directory, and with `account_sharing_in_progress` while it is
/// locked already. In one step with the check, so a launch claimed after it is refused by the
/// lock. Short CLI reads are held back from then on.
pub(crate) fn lock_account(
    state: &Arc<AppState>,
    account: &Account,
    refused: &Refused,
) -> Result<Locked> {
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
pub(crate) struct Changing<'a> {
    state: &'a AppState,
    account: &'a str,
    ended: bool,
}

impl<'a> Changing<'a> {
    pub(crate) fn new(state: &'a AppState, account: &'a str) -> Self {
        Self {
            state,
            account,
            ended: false,
        }
    }

    pub(crate) fn end(mut self, failure: Option<SharingFailure>) {
        self.ended = true;
        self.state.end_sharing_change(self.account, failure);
    }
}

impl Drop for Changing<'_> {
    fn drop(&mut self) {
        if !self.ended {
            self.state.set_sharing_progress(self.account, None);
        }
    }
}

/// What a change left undone: each entry not brought in line, why as a whole (the entries' one
/// cause, or why the change could not start), and the whole of it in words.
pub(crate) type Undone = (Vec<Failure>, Cause, String);

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
pub(crate) async fn carry_out(
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
    broadcast_settings(state)?;
    Ok(Ok(()))
}
