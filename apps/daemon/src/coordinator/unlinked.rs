//! What a start does about the holders whose process runs but that it cannot link to: a refused
//! connection or a timeout says nothing of whether the holder can go on, so what it runs is left
//! running and claimed, never interrupted, resumed or cleaned up after on a guess.
//!
//! - At start ([`claim_at_start`]), the store's record of where each launch runs
//!   (`Store::held_terminals`) says which sessions such a holder runs: each stays claimed
//!   (`AppState::claim_unlinked`), its status kept, its scratch directory kept, and the process
//!   ledger follows its launch (`ProcessLedger::launch_unlinked`) so the reconcile stops none of
//!   what it started. A session in a running status that no record places could be one of theirs
//!   too: it is claimed as well, and resuming the interrupted sessions and the stop of what the
//!   previous run left wait until no such holder is left.
//! - Once such a holder is linked to, its terminals are taken over as at start
//!   ([`super::adopt::adopt_found`]) before the requests its helpers relayed meanwhile reach the
//!   daemon; a session claimed for it that it no longer runs is interrupted.
//!   A holder linked to once a planned stop has begun is not taken over: it is left running, with
//!   what it runs, for the next daemon ([`leave_for_successor`]).
//! - Once it is known to be gone (its registry file or its process is), or was ended for its
//!   version, the sessions claimed for it are interrupted and their launches ended in the ledger.
//! - The daemon's ordinary stop ends those holders by their process (`Holders::shutdown`) and
//!   interrupts what is claimed for them ([`interrupt_at_stop`]).
//!
//! The names users gave shell terminals are kept for a terminal such a holder may run, and
//! forgotten for the rest once no holder is left to say ([`sweep_shell_titles`]).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::adopt::{adopt_found, Adopted};
use crate::holder::client::{EndedTail, Found, HeldAt, HeldRelays, OnResolved, Resolved};
use crate::platform::term::Size;
use crate::state::restart_screens::GoneScreen;
use crate::state::{AppState, UnlinkedFinished};
use crate::store::{HeldKind, HeldTerminalRecord};
use crate::terminal::fan::Fan;
use crate::terminal::pty;
use crate::terminal::saved_screen::PREVIEW_SIZE;

/// Claims, at start, what the holders `pending` run, which run but could not be linked to, and
/// what the holders `ending` run, which are being ended off the start, after the terminals of the
/// others were adopted into `adopted`; adds the sessions claimed to `adopted`, with their scratch
/// directories under `run_dir`. The sessions of a holder being ended are resumed with the rest,
/// once it is gone, when the user asked for that. Forgets what is recorded of the holders that are
/// gone. **Blocks.**
pub(super) fn claim_at_start(
    state: &Arc<AppState>,
    mut pending: Vec<String>,
    ending: Vec<String>,
    run_dir: &Path,
    adopted: &mut Adopted,
) {
    let ending_set: HashSet<String> = ending.iter().cloned().collect();
    pending.extend(ending);
    let mut ending_claimed = Vec::new();
    let (records, records_read) = match state.store.held_terminals() {
        Ok(records) => (records, true),
        Err(err) => {
            tracing::warn!(%err, "reading where the launches run failed");
            (Vec::new(), false)
        }
    };
    let pending_set: HashSet<String> = pending.iter().cloned().collect();
    let adopted_now: HashSet<String> = adopted.sessions.iter().cloned().collect();
    let mut unmatched = Vec::new();
    if !pending.is_empty() {
        for record in records
            .iter()
            .filter(|record| record.kind == HeldKind::Session)
            .filter(|record| pending_set.contains(&record.holder_id))
            .filter(|record| !adopted_now.contains(&record.owner_id))
        {
            // A session it ran that has ended since is not claimed; one that cannot be read is,
            // as nothing says it is not running there.
            let running = match state.store.get_session(&record.owner_id) {
                Ok(Some(session)) => !session.status.has_no_process(),
                Ok(None) => false,
                Err(err) => {
                    tracing::warn!(session = %record.owner_id, %err, "reading the session failed; it stays claimed");
                    true
                }
            };
            if !running {
                continue;
            }
            state.claim_unlinked(
                &record.owner_id,
                Some((record.holder_id.as_str(), record.launch_id.as_str())),
            );
            state
                .process_ledger
                .launch_unlinked(&record.launch_id, &record.owner_id);
            adopted.claimed.push(record.owner_id.clone());
            if ending_set.contains(&record.holder_id) {
                ending_claimed.push(record.owner_id.clone());
            }
        }
        // A session in a running status no record places may run in one of them: never started a
        // second time on a guess.
        let placed: HashSet<&str> = records
            .iter()
            .filter(|record| record.kind == HeldKind::Session)
            .map(|record| record.owner_id.as_str())
            .collect();
        match state.store.list_sessions() {
            Ok(sessions) => {
                for session in sessions {
                    if session.status.has_no_process()
                        || adopted_now.contains(&session.id)
                        || placed.contains(session.id.as_str())
                    {
                        continue;
                    }
                    state.claim_unlinked(&session.id, None);
                    unmatched.push(session.id);
                }
            }
            Err(err) => tracing::warn!(%err, "listing the sessions failed"),
        }
    }
    if !adopted.claimed.is_empty() || !unmatched.is_empty() {
        tracing::info!(
            holders = pending.len(),
            sessions = adopted.claimed.len(),
            unplaced = unmatched.len(),
            "sessions a holder that cannot be linked to runs stay claimed until it is linked to or gone"
        );
    }
    adopted.claimed.extend(unmatched.iter().cloned());
    adopted.scratch_dirs.extend(
        adopted
            .claimed
            .iter()
            .map(|id| run_dir.join(id))
            .filter(|scratch| scratch.is_dir()),
    );
    let still_run: Vec<String> = adopted
        .holders
        .iter()
        .chain(pending.iter())
        .cloned()
        .collect();
    // Read before the records of the gone holders go: the screens saved for the sessions they ran
    // are the previews those sessions are interrupted with.
    let gone = |holder: &str| !still_run.iter().any(|run| run == holder);
    keep_screens_as_previews(state, &records, &[], gone, |session| {
        adopted
            .sessions
            .iter()
            .chain(&adopted.claimed)
            .any(|id| id == session)
    });
    if let Err(err) = state.store.forget_held_terminals_except(&still_run) {
        tracing::warn!(%err, "forgetting where the launches of gone holders ran failed");
    }
    // The sessions of a holder no longer running read as interrupted once the start marks every
    // session nothing runs, and their previews are written: nothing is left for its screens. Not
    // where the records could not be read, which no preview was made from.
    if records_read {
        state.expire_restart_screens(gone);
    }
    sweep_shell_titles(state, &pending_set);
    // The resume waits for a session no record places, which a holder still tried may run, and
    // for the sessions of a holder being ended, which it is to resume once they have no process.
    let deferred = !unmatched.is_empty() || !ending_claimed.is_empty();
    state.mark_unlinked_resumable(ending_claimed);
    state.unlinked_claimed(pending, deferred);
}

/// Keeps the screen the previous daemon's planned stop saved for each session `records` place in a
/// holder `gone` says is gone as the session's saved output, which is the preview shown for an
/// interrupted session; `kept` says which sessions are not interrupted anyway (adopted or
/// claimed). A terminal whose tail is among `tails` (any refused holder gives them: one out of
/// range at any start, one refused once linked, or one ended after the application's quit note)
/// and still holds the mark of the handover its saved screen was saved at has the saved screen
/// restored and the output since the mark replayed on it, as an adoption does; one whose tail
/// does not has its screen rebuilt from the raw tail, as adopting a crashed daemon's terminal
/// does, which is preferred to an older saved screen; one with no usable tail has the saved
/// screen restored alone. A session interrupted or archived already keeps the preview it has.
/// Called where a holder is known to be gone, before the records of it are forgotten. **Blocks.**
///
/// Nothing in either is answered, and no saved screen is consumed here: the caller expires the
/// gone holders' screens once their sessions are settled. A preview that cannot be written leaves
/// the session without a new one, as a failed write at an ordinary stop does: the record that
/// places the session is forgotten and the session interrupted right after, so no later pass
/// could make it again.
///
/// The only place that turns a saved screen or a tail into a preview, and the only one that
/// selects a saved screen for a holder that is gone.
fn keep_screens_as_previews(
    state: &AppState,
    records: &[HeldTerminalRecord],
    tails: &[EndedTail],
    gone: impl Fn(&str) -> bool,
    kept: impl Fn(&str) -> bool,
) {
    for record in records
        .iter()
        .filter(|record| record.kind == HeldKind::Session)
        .filter(|record| gone(record.holder_id.as_str()))
        .filter(|record| !kept(&record.owner_id))
    {
        match state.store.get_session(&record.owner_id) {
            Ok(Some(session)) if !session.status.has_no_process() => {}
            Ok(_) => continue,
            Err(err) => {
                tracing::warn!(session = %record.owner_id, %err, "reading the session failed; it is left without a new preview");
                continue;
            }
        }
        let at = HeldAt {
            holder_id: record.holder_id.clone(),
            terminal_id: record.terminal_id.clone(),
        };
        let ended = tails
            .iter()
            .find(|ended| ended.terminal_id == record.terminal_id);
        let saved = state.select_restart_screen(
            &at,
            ended.and_then(|ended| ended.handover.as_ref()),
            ended.and_then(|ended| ended.size),
        );
        let (preview, from) = match (ended, saved) {
            (
                Some(ended),
                Some(GoneScreen {
                    saved,
                    followed: true,
                }),
            ) => (
                saved.preview(ended.since_handover()),
                "the screen saved at the handover, restored with the output since it",
            ),
            (ended, saved) => match (ended.and_then(preview_from_tail), saved) {
                (Some(rebuilt), _) => (rebuilt, "rebuilt from the tail the holder kept"),
                (None, Some(selected)) => (selected.saved.preview(&[]), "saved at the handover"),
                (None, None) => continue,
            },
        };
        if state.keep_saved_output(&record.owner_id, &preview) {
            tracing::info!(session = %record.owner_id, from, "kept a screen as the interrupted session's preview");
        }
    }
}

/// The screen a terminal ended with, rebuilt from the tail its holder kept as adopting it would
/// have: the output replayed into a parsed screen of its size. `None` for an empty tail, which
/// shows nothing.
fn preview_from_tail(ended: &EndedTail) -> Option<Vec<u8>> {
    if ended.tail.is_empty() {
        return None;
    }
    let (rows, cols) = ended
        .size
        .map_or(PREVIEW_SIZE, |Size { rows, cols }| (rows, cols));
    let fan = Fan::new(rows, cols, false);
    fan.seed(&ended.tail, pty::READ_BUF);
    Some(fan.output())
}

/// What the holders' client is told a holder that could not be linked to at start turned out to
/// be: its resolution runs on a blocking thread of the runtime this is called inside, with
/// `run_dir` the directory of the sessions' scratch directories.
pub(super) fn on_resolved(state: &Arc<AppState>, run_dir: &Path) -> OnResolved {
    let runtime = tokio::runtime::Handle::current();
    let (state, run_dir) = (state.clone(), run_dir.to_path_buf());
    Arc::new(move |resolved| {
        let (state, run_dir) = (state.clone(), run_dir.clone());
        runtime.spawn_blocking(move || resolve(&state, resolved, &run_dir));
    })
}

/// Acts on how a holder that could not be linked to at start turned out, once the start has made
/// its claims. **Blocks.**
fn resolve(state: &Arc<AppState>, resolved: Resolved, run_dir: &Path) {
    state.await_unlinked_claimed();
    match resolved {
        Resolved::Linked {
            holder_id: holder,
            found,
            relays,
        } => {
            if state.is_handing_over() {
                tracing::info!(holder = %holder, "a holder was linked to during a planned stop; it is left to the next daemon");
                leave_for_successor(found, relays);
                return;
            }
            tracing::info!(
                holder = %holder,
                terminals = found.terminals.len(),
                "taking over what a holder that could not be linked to at start runs"
            );
            let adopted = adopt_found(state, found.terminals, run_dir);
            // Every terminal it runs is settled, so a screen none of them took is of one it no
            // longer runs, even where it runs none.
            state.expire_restart_screens(|id| id == holder);
            // The requests its helpers relayed meanwhile reach the sessions they name, which
            // exist now.
            drop(relays);
            settle_holder(state, &holder, Some(adopted.sessions.as_slice()), run_dir);
        }
        Resolved::Gone { holder_id, tails } => {
            tracing::info!(holder = %holder_id, "a holder that could not be linked to at start is gone; what it ran is interrupted");
            // Its sessions are interrupted below: the screen saved for each is their preview, and
            // is kept while they still read as running.
            let gone = |holder: &str| holder == holder_id;
            let records_read = match state.store.held_terminals() {
                Ok(records) => {
                    keep_screens_as_previews(state, &records, &tails, gone, |session| {
                        state.live_session(session).is_some()
                    });
                    true
                }
                Err(err) => {
                    tracing::warn!(holder = %holder_id, %err, "reading where the gone holder's launches ran failed; its saved screens are kept");
                    false
                }
            };
            settle_holder(state, &holder_id, None, run_dir);
            if let Err(err) = state.store.forget_held_terminals_of(&holder_id) {
                tracing::warn!(holder = %holder_id, %err, "forgetting where the gone holder's launches ran failed");
            }
            // Its sessions are interrupted and their previews written: nothing is left for its
            // screens.
            if records_read {
                state.expire_restart_screens(gone);
            }
        }
    }
    finish(state, run_dir);
}

/// What a holder linked to during a planned stop gave, kept until the process ends: dropping a
/// held terminal releases it in its holder, which would end its program, and dropping the relays
/// would let the helpers' requests through to sessions that are not taken over.
static LEFT_FOR_SUCCESSOR: Mutex<Vec<(Found, HeldRelays)>> = Mutex::new(Vec::new());

/// Keeps what a holder linked to during a planned stop gave ([`LEFT_FOR_SUCCESSOR`]), so that the
/// holder goes on running it for the next daemon, and notes the holder as left to it, so that an
/// ordinary stop escalating the planned stop ends it all the same. **Blocks** while that stop
/// ends it.
fn leave_for_successor(found: Found, relays: HeldRelays) {
    relays.leave_for_successor();
    LEFT_FOR_SUCCESSOR
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push((found, relays));
}

/// The holder `holder` is no longer tried: linked to, with `adopted` the sessions whose terminals
/// were taken over from it, or gone (`None`). Every session claimed for it that was not taken
/// over is interrupted, then released. **Blocks.**
pub(super) fn settle_holder(
    state: &Arc<AppState>,
    holder: &str,
    adopted: Option<&[String]>,
    run_dir: &Path,
) {
    for (session, launch) in state.take_unlinked_holder(holder) {
        if !adopted.is_some_and(|adopted| adopted.contains(&session)) {
            interrupt(state, &session, Some(&launch), run_dir);
        }
        state.release_unlinked_claim(&session);
    }
}

/// Once no holder is tried any more: interrupts what no record placed and nothing took over,
/// runs what the start put off, and forgets the names of shell terminals no holder runs.
/// **Blocks.**
pub(super) fn finish(state: &Arc<AppState>, run_dir: &Path) {
    let finished: Option<UnlinkedFinished> = state.finish_unlinked();
    if let Some(finished) = finished {
        for session in finished.unmatched {
            if state.live_session(&session).is_none() {
                interrupt(state, &session, None, run_dir);
            }
            state.release_unlinked_claim(&session);
        }
        if finished.run_start_work {
            state.process_ledger.release_previous_run();
            if let Some(among) = finished.resume_among {
                super::resume_on_start::spawn_among(state, Some(among));
            }
        }
    }
    sweep_shell_titles(state, &state.unlinked_holders());
}

/// Resumes the interrupted sessions, when the user asked for that, and has the process ledger
/// reconcile with the previous run's file and stop what that run left: what the start does once
/// it knows which sessions have a process. While a holder that could not be linked to may run a
/// session no record places (see [`claim_at_start`]), the resume, of the sessions interrupted now
/// and those claimed for want of a record, and the stop of what the previous run left wait until
/// no such holder is left; the ledger follows this run as ever meanwhile. Called inside the
/// runtime.
pub fn start_resume_and_reconcile(state: &Arc<AppState>) {
    let wanted = super::resume_on_start::wanted(state);
    let among = wanted.then(|| {
        let mut among = super::resume_on_start::interrupted_now(state);
        among.extend(state.unlinked_resumable());
        among
    });
    // Asked for before the decision, and let go if nothing is put off, so that a holder resolved
    // in between finds the hold to release.
    state.process_ledger.hold_previous_run();
    if !state.defer_start_work(among) {
        state.process_ledger.release_previous_run();
        if wanted {
            super::resume_on_start::spawn_among(state, None);
        }
    }
    // On a thread of its own: what the previous run's agents left is stopped without holding up
    // anything else. A launch meanwhile is safe from it, since its launch id is reserved before
    // its agent starts.
    state.process_ledger.start(true);
}

/// Marks the session `session` interrupted, its process being gone, and has the ledger end its
/// launch `launch`, when it is known, and its scratch directory removed. **Blocks.**
fn interrupt(state: &Arc<AppState>, session: &str, launch: Option<&str>, run_dir: &Path) {
    if let Err(err) = state.interrupt_session_record(session) {
        tracing::warn!(session, %err, "marking the session interrupted failed");
    }
    if let Some(launch) = launch {
        state.process_ledger.end_launch(launch);
    }
    let scratch: PathBuf = run_dir.join(session);
    if scratch.is_dir() {
        if let Err(err) = std::fs::remove_dir_all(&scratch) {
            tracing::warn!(session, %err, "removing the session's scratch directory failed");
        }
    }
}

/// Marks interrupted every session claimed for a holder that could not be linked to, as the
/// daemon's ordinary stop does every session: the stop ends those holders with what they run.
pub fn interrupt_at_stop(state: &AppState) {
    for session in state.unlinked_sessions() {
        if let Err(err) = state.interrupt_session_record(&session) {
            tracing::warn!(session, %err, "marking the session interrupted failed");
        }
    }
}

/// Forgets the names users gave shell terminals that no longer run: a name is kept while its
/// terminal is held, while one of `unlinked`, the holders that could not be linked to and are
/// still tried, is recorded as running it, and while any such holder is left for a terminal
/// recorded nowhere, which it may run.
fn sweep_shell_titles(state: &AppState, unlinked: &HashSet<String>) {
    let (Ok(ids), Ok(records)) = (
        state.store.shell_terminal_title_ids(),
        state.store.held_terminals(),
    ) else {
        tracing::warn!("reading the names of the shell terminals failed; none is forgotten");
        return;
    };
    // Each shell terminal recorded, with the holder that runs it.
    let placed: HashMap<&str, &str> = records
        .iter()
        .filter(|record| record.kind == HeldKind::ShellTerminal)
        .map(|record| (record.owner_id.as_str(), record.holder_id.as_str()))
        .collect();
    for id in ids {
        if state.shell_terminals.process(&id).is_some() {
            continue;
        }
        let kept = match placed.get(id.as_str()) {
            Some(holder) => unlinked.contains(*holder),
            None => !unlinked.is_empty(),
        };
        if kept {
            continue;
        }
        if let Err(err) = state.store.forget_shell_terminal(&id) {
            tracing::warn!(terminal = %id, %err, "forgetting the shell terminal's name failed");
        }
    }
}
