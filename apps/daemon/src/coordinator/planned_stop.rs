//! The daemon's planned stop: every terminal is handed over to its holder for the next daemon, with
//! its screen saved, and nothing is ended.
//!
//! The account quota runs and logins are stopped, as the ordinary stop stops them; the handover
//! barrier is raised, which refuses whatever would change or end a terminal from then on and waits
//! for what was admitted before (`crate::state::ChangeAdmission`); and the hook events and program
//! status reports already received are applied (`AppState::settle_for_planned_stop`). No client
//! changes a terminal's size, or has the program told of its display or focus, from then on
//! (`PtyCore::hold_client_changes`). Then the holders of the terminals listed are told the
//! terminals are handed over (`Holders::hand_over`): each marks the end of each terminal's output
//! to this daemon and keeps what follows for the next one. Once a handed-over terminal's reader has
//! read up to its mark, the answers to the queries in what it read have gone into the program's
//! input, each write acknowledged by the holder (`PtyCore::answering`, `PtyCore::answers_lost`),
//! and no client owes the program a reply to a query left to the clients
//! (`PtyCore::replies_owed`), its screen is saved (`PtyCore::save_screen`), and every screen is
//! written with the handover's token for the next daemon (`crate::state::restart_screens`),
//! replacing the file whole: the screens an earlier handover saved for holders still pending are
//! carried over, and no other screen of it is left. The queries before a mark are this daemon's
//! and its clients' alone to answer: the holder and the next daemon answer only those after it.
//! Last, the daemon parts from every holder, leaving each running (`Holders::part_planned`), those
//! it could not link to included.
//!
//! Unlike the ordinary stop, it marks no session interrupted, closes no shell terminal and forgets
//! none of their records, ends no launch in the process ledger, which it leaves as a crash would
//! for the next daemon's reconcile, and leaves the sessions claimed for holders it could not link
//! to as they are. A terminal not handed over (its holder did not answer, or takes no handover) is
//! left running too, for the next daemon to rebuild from its tail as after a crash.
//!
//! An ordinary stop asked for while it runs (`AppState::ordinary_stop_asked`) escalates it, and
//! the handover stops where it stands ([`Handover::Escalated`]): before any terminal is marked,
//! nothing is handed over; once some are, the readers and the answers are no longer waited for,
//! no screen is saved from then on, the screens saved by then are written all the same, and the
//! holders are not parted from. The caller, which owns the whole stop, then ends what the planned
//! stop was to leave running as an ordinary stop ends it (`main`'s
//! `stop_all_processes_after_planned`).

use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::holder::client::{HeldAt, Holders};
use crate::session::LiveSession;
use crate::state::restart_screens::{self, CarriedScreen};
use crate::state::AppState;
use crate::terminal::pty::PtyCore;
use crate::terminal::saved_screen::SavedScreen;
use crate::terminal::shell::ShellProcess;

/// How long the readers of the handed-over terminals are waited for, all together, to read what the
/// holder sent before each terminal's mark, their answers' writers to write the answers to the
/// queries in it, and the clients to reply to those left to them.
const READER_WAIT: Duration = Duration::from_secs(3);

/// How often a wait of the handover looks whether an ordinary stop has been asked for meanwhile.
const ESCALATION_POLL: Duration = Duration::from_millis(50);

/// How a planned stop ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handover {
    /// Carried out to the parting: every holder is left running for the next daemon.
    Parted(HandedOver),
    /// An ordinary stop was asked for before the parting, which cut the handover short: the
    /// holders are still linked to, those that handed terminals over known to
    /// `Holders::left_to_successor`, and nothing is ended yet.
    Escalated,
}

/// What a planned stop handed over.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct HandedOver {
    /// The terminals of the daemon's sessions and shell terminals their holders handed over.
    pub terminals: usize,
    /// The screens saved for the next daemon.
    pub screens: usize,
    /// The terminals left running without a handover, which the next daemon rebuilds from their
    /// tails.
    pub not_handed_over: usize,
}

/// Makes the planned stop over `holders`, writing the saved screens to `screens_path`. Called
/// inside the runtime, once the daemon accepts no more connections. The connections already open
/// are served until the process exits, so what they ask for meanwhile is refused where it would
/// end or change a terminal, or remove what one works in (`AppState::admit_change`). What is still
/// queued in the outbox is saved afterwards, once the terminals are the holders'
/// (`AppState::save_restart_outbox`). An ordinary stop asked for before the parting escalates it
/// ([`Handover::Escalated`]); the wait for the operations admitted before the barrier is cut short
/// then too, and leaves the barrier raised.
pub async fn hand_over<H>(state: &Arc<AppState>, holders: H, screens_path: PathBuf) -> Handover
where
    H: Deref<Target = Holders> + Send + 'static,
{
    if state.ordinary_stop_asked() {
        return Handover::Escalated;
    }
    state.account_quotas.stop_cli_runs();
    state.account_logins.stop_all().await;
    // Before the terminals are listed: a holder still pending then is left untouched, and the
    // next daemon that links to it finds the screens of the handover it still reports, while one
    // linked to after all meanwhile is parted from untouched too.
    let pending = holders.begin_planned_stop();
    let carried = state.pending_restart_screens(&pending);
    let sessions = tokio::select! {
        sessions = state.settle_for_planned_stop() => sessions,
        () = state.await_ordinary_stop() => {
            tracing::info!("an ordinary stop was asked for before the terminals were handed over; none is");
            return Handover::Escalated;
        }
    };
    let shells = state.shell_terminals.processes();
    let watching = state.clone();
    let handing = tokio::task::spawn_blocking(move || {
        hand_over_blocking(
            &holders,
            &sessions,
            &shells,
            &screens_path,
            &carried,
            &|| watching.ordinary_stop_asked(),
        )
    });
    handing.await.unwrap_or_else(|err| {
        tracing::error!(%err, "handing the terminals over panicked");
        if state.ordinary_stop_asked() {
            Handover::Escalated
        } else {
            Handover::Parted(HandedOver::default())
        }
    })
}

/// The handover itself, the saving of the screens and the parting, each step given up once
/// `escalated` says an ordinary stop has been asked for. **Blocks.**
fn hand_over_blocking(
    holders: &Holders,
    sessions: &[Arc<LiveSession>],
    shells: &[Arc<ShellProcess>],
    screens_path: &Path,
    carried: &[CarriedScreen],
    escalated: &dyn Fn() -> bool,
) -> Handover {
    let token = uuid::Uuid::new_v4().simple().to_string();
    let terminals: Vec<(&str, &PtyCore)> = sessions
        .iter()
        .map(|live| (live.id.as_str(), &***live))
        .chain(shells.iter().map(|shell| (shell.id.as_str(), &***shell)))
        .collect();
    // An answer lost before the handover is no loss of the handover's: only one lost from here on
    // keeps a screen from being restored as exact. A size change a client asked for before is
    // finished by now, and one asked for later is left to the client's next daemon, so that the
    // parsed screen and the program's terminal keep the size the screen is saved at.
    for (_, pty) in &terminals {
        pty.forget_lost_answers();
        pty.hold_client_changes();
    }
    // Only the holders of the terminals listed: one linked to late, whose terminals were adopted
    // after the list was taken, is left untouched for the next daemon, which rebuilds them from
    // their tails, rather than marked with no screen saved.
    let mut listed: Vec<String> = terminals
        .iter()
        .filter_map(|(_, pty)| pty.held_at())
        .map(|at| at.holder_id)
        .collect();
    listed.sort_unstable();
    listed.dedup();
    // The last point at which nothing is handed over: from the marks on, the terminals marked are
    // the next daemon's, and only ending their holders ends them.
    if escalated() {
        tracing::info!(
            "an ordinary stop was asked for before the terminals were handed over; none is"
        );
        return Handover::Escalated;
    }
    let marked = holders.hand_over(&token, &listed);
    let mut handed: Vec<(&str, &PtyCore, HeldAt)> = Vec::new();
    let mut not_handed_over = 0;
    for (id, pty) in terminals {
        match pty.held_at() {
            Some(at) if marked.contains(&at) => handed.push((id, pty, at)),
            _ => not_handed_over += 1,
        }
    }
    // The readers end on their own once they reach their marks: one deadline for all of them.
    let deadline = Instant::now() + READER_WAIT;
    let saved: Vec<(HeldAt, SavedScreen)> = handed
        .iter()
        .filter_map(|(id, pty, at)| save_screen(id, pty, at, deadline, escalated))
        .collect();
    let screens = saved.len();
    // Replaced even with nothing saved, so that no screen of an earlier handover outlives this
    // one but those carried over. Written too when an ordinary stop cut the saving short: should
    // the daemon not live to end the holders, the next daemon finds the screens saved by then, as
    // an exact screen for a terminal still running and as the preview of one that is gone.
    if let Err(err) = restart_screens::replace(screens_path, &token, &saved, carried) {
        tracing::error!(path = %screens_path.display(), %err, "writing the saved screens failed; the next daemon rebuilds every screen from its tail");
    }
    let handed_over = HandedOver {
        terminals: handed.len(),
        screens,
        not_handed_over,
    };
    if escalated() {
        tracing::info!(
            terminals = handed_over.terminals,
            screens = handed_over.screens,
            "an ordinary stop was asked for during the handover; the holders are not parted from"
        );
        return Handover::Escalated;
    }
    holders.part_planned();
    tracing::info!(
        terminals = handed_over.terminals,
        screens = handed_over.screens,
        not_handed_over = handed_over.not_handed_over,
        "handed the terminals over to the holders for the next daemon"
    );
    Handover::Parted(handed_over)
}

/// The screen of the handed-over terminal `pty` (of the session or shell terminal `id`), once its
/// reader has read everything sent before the terminal's mark and the answers to the queries in it
/// have gone into the program's input, the daemon's own and the replies of the clients to those
/// left to them; `None` when, at `deadline`, the reader is still reading, the daemon's answers are
/// still being written or a client still owes a reply, when an answer or a reply did not go in, or
/// when the screen cannot be saved, which leaves the next daemon to rebuild it from the tail. A
/// terminal whose answers did not all go in is not restored as exact: the program may still be
/// waiting on one of them, which the next daemon cannot give, and the rebuilt screen is marked
/// inexact and the program asked to draw it again. `None` too, and no longer waited for, once
/// `escalated` says an ordinary stop has been asked for. **Blocks.**
fn save_screen(
    id: &str,
    pty: &PtyCore,
    at: &HeldAt,
    deadline: Instant,
    escalated: &dyn Fn() -> bool,
) -> Option<(HeldAt, SavedScreen)> {
    wait_unless(
        deadline,
        escalated,
        || pty.reader_running(),
        |slice| pty.wait_for_reader(slice),
    );
    if escalated() {
        return None;
    }
    if pty.reader_running() {
        tracing::warn!(terminal = id, "the terminal's reader did not reach its handover mark in time; its screen is not saved");
        return None;
    }
    wait_unless(
        deadline,
        escalated,
        || pty.answering(),
        |slice| pty.wait_for_answers(slice),
    );
    wait_unless(
        deadline,
        escalated,
        || pty.replies_owed(),
        |slice| pty.wait_for_replies(slice),
    );
    if escalated() {
        return None;
    }
    if pty.answering() || pty.replies_owed() || pty.answers_lost() {
        tracing::warn!(terminal = id, "the answers to the queries before the terminal's handover mark did not all go in in time; its screen is not saved");
        return None;
    }
    match pty.save_screen() {
        Some(screen) => Some((at.clone(), screen)),
        None => {
            tracing::info!(
                terminal = id,
                "the terminal's screen cannot be saved; the next daemon rebuilds it from its tail"
            );
            None
        }
    }
}

/// Waits with `wait`, given at most [`ESCALATION_POLL`] at a time, while `busy` says so, until
/// `deadline` or until `escalated` says so. **Blocks.**
fn wait_unless(
    deadline: Instant,
    escalated: impl Fn() -> bool,
    busy: impl Fn() -> bool,
    wait: impl Fn(Duration),
) {
    while busy() && !escalated() {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return;
        }
        wait(left.min(ESCALATION_POLL));
    }
}

#[cfg(test)]
mod tests;
