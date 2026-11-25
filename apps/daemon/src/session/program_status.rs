//! A session's status taken from what the program in its terminal reports about itself through
//! OSC 7501 (`crate::terminal::program_status`), for an agent whose hooks and own signals give no
//! status (`AgentProvider::has_status_integration`). Where they do, they win: every session's
//! terminal still keeps the reports, takes them out of what the clients receive and answers their
//! support query, since supporting the protocol is the terminal's, and the reports stand in for
//! the activity glyphs an agent that reports drops from its title
//! (`crate::session::title_activity`), but they move no status.
//!
//! The root record's state is the session's status: `working` is `Working`, `blocked` is
//! `WaitingUser`, `idle` and `done` are `Idle` with the session not paused, `error` is `Idle` with
//! it paused, and a root record cleared or forgotten is `Idle` too. A turn starts when the root
//! goes to working with none open, and ends when it leaves both working and blocked, closed and
//! reported as a hook's turn end is, with the `msg` of the `done` or `error` report that ended it
//! as its final message, or the message the provider read for the turn from a source of its own
//! ([`turn_message`]). A turn that ends on `idle`, with no `done` or `error`, was interrupted: a
//! program reports `idle` when its turn is cancelled, so the turn is closed as a hook's cancelled
//! turn is, with no report, and the session is left paused. A `blocked` of kind `auth` counts only
//! once it has stood for [`AUTH_SETTLE`]: a program may report it while it checks a sign-in that
//! is there, and one the next report replaces within that time is passed over whole. Dormant and
//! working in background never come from here.
//!
//! **The idle before a launch's first turn.** A launch that gives the agent a prompt on its command
//! line reads as working from the start. A program may report `idle` once it is up and `working`
//! only when it takes the prompt, a moment later: applied, that `idle` would show the session at
//! its prompt and let the messages held for it go into a turn about to start. So an `idle` with
//! no turn open, while the launch's prompt has not started its turn, counts only once it has stood
//! for [`LAUNCH_IDLE_SETTLE`], and is passed over whole where the next report replaces it within
//! that time. One that stands is applied: the agent did not take the prompt. Only the launch
//! itself waits for that turn; a daemon that adopts the launch does not.
//!
//! **A `done` inside a turn.** A program may report `done` between two model calls of one turn,
//! for a moment, and `working` again at the next call: Qwen Code does, for about 0.1 s
//! (`docs/agent-cli-reference.md`, "Qwen Code"). Applied, each such `done` would close and report
//! a turn, show the session at its prompt and release what is held for it into the turn still
//! running. Where the provider declares so (`AgentProvider::reported_done_gap`), a `done` straight
//! after `working` with a turn open is held for the bound it declares: a `working` within it
//! passes the `done` over whole, and the turn goes on; any other report, the bound running out,
//! the end of the process or a daemon that begins to stop have the `done` applied, ending the
//! turn as it says. Unlike the other holds, this one is waited for by whoever settles the moves:
//! the `done` is applied the moment the process ends, so the exit finds its turn ended by it.
//!
//! A provider whose hooks give its status can still declare that its reports say when it waits on
//! the user and when a turn was cancelled ([`signals`]).
//!
//! The moves are taken on the PTY reader thread and applied in order by one blocking task at a
//! time, since applying one writes to the store and may wait on the owner's terminal. The end of
//! the process waits for them ([`LiveSession::settle_program_status`]) before it is recorded.

mod signals;
mod turn_message;

pub(super) use signals::DeclaredSignals;
pub use signals::ReportedSignals;
#[cfg(test)]
pub(crate) use turn_message::shorten_turn_message_bound;
pub(super) use turn_message::TurnMessages;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::Notify;

use super::LiveSession;
use crate::hooks::TurnEnd;
use crate::messaging::reporting;
use crate::protocol::SessionStatus;
use crate::state::AppState;
use crate::terminal::program_status::{BlockedKind, ReportedState, RootMove};

/// How long a `blocked` report of kind `auth` has to stand before it is applied. A program can
/// report it while it checks its sign-in, as it starts and as it resumes, and go on to `idle`
/// without the user having had anything to do: Antigravity CLI does at every launch, for 0.1 to
/// 1.7 seconds when signed in (`docs/agent-cli-reference.md`, "Antigravity CLI"). Applied at once,
/// that would show the session waiting for the user and tell its owner so, for a wait that is over
/// before anyone could act on it. A sign-in the user really has to make stays, and is
/// shown and told this much later. The other kinds, and a `blocked` that names none, are waits on a
/// person from the moment they are reported, and are applied at once.
#[cfg_attr(test, allow(dead_code))]
const AUTH_SETTLE: Duration = Duration::from_secs(3);

/// How often a held report is looked at again for the report that replaces it.
const AUTH_POLL: Duration = Duration::from_millis(25);

/// How long an `idle` reported before the turn of the launch's own prompt has started has to stand
/// before it is applied. Pi reports `idle` once started and `working` about 0.3 seconds later
/// (`docs/agent-cli-reference.md`, "Pi"); the settle is several times that, for a loaded machine,
/// and what it costs is only how long a session whose agent did not take its prompt goes on
/// reading as working.
#[cfg_attr(test, allow(dead_code))]
const LAUNCH_IDLE_SETTLE: Duration = Duration::from_secs(5);

/// [`LAUNCH_IDLE_SETTLE`], except under test, as [`auth_settle`] is: `PATIENCE`, or what a test
/// proving the settle runs out sets in `LAUNCH_IDLE_SETTLE_OVERRIDE`.
fn launch_idle_settle() -> Duration {
    #[cfg(test)]
    {
        LAUNCH_IDLE_SETTLE_OVERRIDE
            .with(std::cell::Cell::get)
            .unwrap_or(crate::test_support::PATIENCE)
    }
    #[cfg(not(test))]
    LAUNCH_IDLE_SETTLE
}

#[cfg(test)]
thread_local! {
    pub(super) static LAUNCH_IDLE_SETTLE_OVERRIDE: std::cell::Cell<Option<Duration>> =
        const { std::cell::Cell::new(None) };
}

/// [`AUTH_SETTLE`], except under test: there the report that replaces a passing `auth` comes from
/// the test, through a stand-in's reader, so the window it has to arrive in is `PATIENCE`, and a
/// test proving the settle runs out shortens it with `AUTH_SETTLE_OVERRIDE`. Read on the thread
/// that takes the reports in, before the moves go to the task applying them.
fn auth_settle() -> Duration {
    #[cfg(test)]
    {
        AUTH_SETTLE_OVERRIDE
            .with(std::cell::Cell::get)
            .unwrap_or(crate::test_support::PATIENCE)
    }
    #[cfg(not(test))]
    AUTH_SETTLE
}

#[cfg(test)]
thread_local! {
    pub(super) static AUTH_SETTLE_OVERRIDE: std::cell::Cell<Option<Duration>> =
        const { std::cell::Cell::new(None) };
}

/// The window a `done` held as a possible gap inside a turn waits in for the next model call's
/// `working`: the bound the provider declares (`AgentProvider::reported_done_gap`), except under
/// test. There the reports come from the test, which feeds the `working` that passes the `done`
/// over without regard to time, and a test whose `done` is to be applied ends the window with
/// [`LiveSession::end_reported_done_gaps`]. The window it falls back on is longer than the
/// `PATIENCE` a test waits for the `done`'s effect, so a test that relied on it running out would
/// fail rather than pass by a race.
fn done_gap_window(declared: Duration) -> Duration {
    #[cfg(test)]
    {
        let _ = declared;
        crate::test_support::PATIENCE * 2
    }
    #[cfg(not(test))]
    declared
}

/// Whether `next` is a wait that counts only once it has stood for [`AUTH_SETTLE`].
fn settles_first(next: &RootMove) -> bool {
    next.state == Some(ReportedState::Blocked) && next.kind == Some(BlockedKind::Auth)
}

/// The moves of a session's root status record on their way to its status, shared with the task
/// applying them.
#[derive(Default)]
pub(super) struct StatusReports {
    moves: Mutex<Moves>,
    /// Woken whenever the task applying the moves stops with none left: all applied, or the rest
    /// dropped (the launch is no longer the session's, or applying one failed).
    settled: Notify,
    /// Held by a test to keep the next move from being applied.
    #[cfg(test)]
    pub(super) gate: Mutex<()>,
    /// Set by a test to end the window of every `done` held as a gap inside a turn, from now on
    /// ([`LiveSession::end_reported_done_gaps`]).
    #[cfg(test)]
    done_gaps_ended: std::sync::atomic::AtomicBool,
}

/// The moves themselves, behind [`StatusReports`]' lock.
#[derive(Default)]
struct Moves {
    /// Taken from the output and not yet applied, oldest first.
    pending: VecDeque<RootMove>,
    /// The root's state as last applied.
    applied: Option<ReportedState>,
    /// A task is applying `pending`.
    draining: bool,
    /// That task is holding the move it took last for its settle ([`AUTH_SETTLE`],
    /// [`LAUNCH_IDLE_SETTLE`]), with nothing to apply until the settle has passed or the next move
    /// has come.
    holding: bool,
}

impl Moves {
    /// Whether every move taken so far has been applied, as far as it is to be for now: none is
    /// pending, and the task applying them has stopped or only holds a move for its settle. A
    /// move held for its settle ([`AUTH_SETTLE`], [`LAUNCH_IDLE_SETTLE`]) is not waited for: it
    /// may never be applied, and whoever settles (the end of the process, a planned stop going
    /// through every session inside one short bound, a report's delivery) would spend its whole
    /// bound on it. A `done` held as a gap inside a turn is waited for: it is applied at once when
    /// the process ends or the daemon begins to stop, and is the last word on the turn.
    fn settled(&self) -> bool {
        self.pending.is_empty() && (!self.draining || self.holding)
    }
}

impl StatusReports {
    fn moves(&self) -> std::sync::MutexGuard<'_, Moves> {
        self.moves.lock().expect("status reports lock poisoned")
    }

    /// Whether a test has ended the window of a held `done` ([`done_gap_window`]); never in the
    /// daemon itself.
    fn done_gaps_ended(&self) -> bool {
        #[cfg(test)]
        {
            self.done_gaps_ended
                .load(std::sync::atomic::Ordering::Acquire)
        }
        #[cfg(not(test))]
        false
    }
}

/// The wire status the root record's state gives.
pub(super) fn wire_status(root: Option<ReportedState>) -> SessionStatus {
    match root {
        Some(ReportedState::Working) => SessionStatus::Working,
        Some(ReportedState::Blocked) => SessionStatus::WaitingUser,
        Some(ReportedState::Idle | ReportedState::Done | ReportedState::Error) | None => {
            SessionStatus::Idle
        }
    }
}

impl LiveSession {
    /// Takes the moves of the root status record the latest chunk of output made, notes each in the
    /// title activity, and has them applied where the reports give this session its status. Called
    /// on the PTY reader thread after each chunk.
    pub(super) fn note_program_status(&self) {
        let moves = self.pty.take_program_status();
        self.note_program_status_moves(moves);
    }

    /// Takes a status report the provider received through the launch's hook and retains it in
    /// the terminal snapshot so adoption can recover it like a report from the terminal.
    pub(crate) fn note_hook_program_status(&self, record: crate::terminal::program_status::Record) {
        if let Some(moved) = self.pty.record_program_status(record) {
            self.note_program_status_moves(vec![moved]);
        }
    }

    fn note_program_status_moves(&self, moves: Vec<RootMove>) {
        if moves.is_empty() {
            return;
        }
        // Every agent's reports say whether a turn runs, as its title did before it took to
        // reporting (`title_activity`), whether or not they give the session its status.
        self.note_reported_activity(&moves);
        if !self.status_from_reports {
            // The hooks give the status; the reports give what the provider declares they say.
            self.note_declared_signals(&moves);
            return;
        }
        self.status_reports.moves().pending.extend(moves);
        self.apply_program_status();
    }

    /// Takes the root status record an adopted terminal was rebuilt with, from the saved screen
    /// (`PtyCore::restore_screen`) or the holder's tail (`PtyCore::seed_output`), as what the
    /// program last reported about its turn
    /// ([`super::title_activity::TitleActivity::note_report`]). The rebuilding drops the moves, and
    /// a program does not repeat a report, so a turn running across the restart would otherwise
    /// never read as running. Its status is the store's, kept across the restart.
    ///
    /// Where the reports give the session its status, the record is also taken as the state last
    /// applied to it, so the program's next move is applied from it rather than from nothing; and
    /// a record that says a turn runs reopens that turn in `state`, whose open turns are held in
    /// memory and lost with the daemon that had them: `working`, and `blocked` on a permission or
    /// a question, which a program asks within a turn. So a `done` after a turn that ran across
    /// the restart closes and reports that turn, and a `blocked` in between does not open another.
    /// A `blocked` that names no kind opens none: it may as well be a screen shown before any
    /// turn, and a turn opened for one would be reported as ended when the screen is answered.
    ///
    /// A `blocked` on a sign-in is not taken as applied at all: the daemon that read it may have
    /// stopped while it still held it ([`AUTH_SETTLE`]), so it is queued as the move it was, to
    /// stand for the settle again before it counts, and is passed over if the program's next
    /// report replaces it. Applied a second time, where the daemon before had applied it, it
    /// changes nothing.
    ///
    /// A tail that starts after the program's last report rebuilds no record, and the turn then
    /// reads as not running until the program reports again. Called before the session is
    /// registered, so no move of the program's is applied first.
    pub(crate) fn note_restored_program_status(&self, state: &AppState) {
        let root = self.pty.program_status_root();
        let Some(standing) = root.state else {
            return;
        };
        self.note_reported_activity(std::slice::from_ref(&root));
        if !self.status_from_reports {
            // A wait the program reported is one it is still in: the hand it raised was kept in
            // memory by the daemon before.
            self.note_declared_signals(std::slice::from_ref(&root));
            return;
        }
        if settles_first(&root) {
            self.status_reports.moves().pending.push_back(root);
            return;
        }
        self.status_reports.moves().applied = Some(standing);
        let in_turn = match standing {
            ReportedState::Working => true,
            ReportedState::Blocked => matches!(
                root.kind,
                Some(BlockedKind::Permission | BlockedKind::Question)
            ),
            ReportedState::Idle | ReportedState::Done | ReportedState::Error => false,
        };
        // Its opening prompt was the daemon's before: none follows.
        if in_turn {
            state.turn_resumed(&self.id);
        }
    }

    /// The launch gave the agent a prompt on its command line, whose turn the reports have yet to
    /// show starting. Nothing where the reports do not give this session its status.
    pub(crate) fn await_launch_turn(&self) {
        self.awaits_launch_turn.store(
            self.status_from_reports,
            std::sync::atomic::Ordering::Release,
        );
    }

    /// Whether the turn of the launch's own prompt is still to start ([`Self::await_launch_turn`]).
    fn awaits_launch_turn(&self) -> bool {
        self.awaits_launch_turn
            .load(std::sync::atomic::Ordering::Acquire)
    }

    /// Notes `moves` of the root status record in the title activity.
    fn note_reported_activity(&self, moves: &[RootMove]) {
        let mut observer = self.title.lock().expect("title observer lock poisoned");
        let now = std::time::Instant::now();
        for root in moves {
            observer.activity.note_report(root.state, now);
        }
    }

    /// Starts a task applying the pending moves, unless one runs or the session is not yet bound
    /// to the daemon's state (`Self::observe_titles` calls this again once it is).
    pub(super) fn apply_program_status(&self) {
        let (state, runtime) = {
            let observer = self.title.lock().expect("title observer lock poisoned");
            (observer.state.upgrade(), observer.runtime.clone())
        };
        let (Some(state), Some(runtime)) = (state, runtime) else {
            return;
        };
        {
            let mut moves = self.status_reports.moves();
            if moves.draining || moves.pending.is_empty() {
                return;
            }
            moves.draining = true;
        }
        let mut applying = Applying {
            reports: self.status_reports.clone(),
            stopped: false,
        };
        let (id, launch_id) = (self.id.clone(), self.launch_id.clone());
        let auth_settle = auth_settle();
        let launch_idle_settle = launch_idle_settle();
        let done_gap = self.done_gap.map(done_gap_window);
        let message_bound = turn_message::bound();
        runtime.spawn_blocking(move || loop {
            let reports = applying.reports.clone();
            #[cfg(test)]
            let _held = reports.gate.lock().expect("status reports gate poisoned");
            // A session relaunched meanwhile, or gone, has nothing more of this launch applied.
            let current = || {
                state
                    .live_session(&id)
                    .is_some_and(|live| live.launch_id == launch_id)
            };
            let next = {
                let mut moves = reports.moves();
                match moves.pending.pop_front() {
                    Some(next) if current() => Some(next),
                    _ => {
                        moves.pending.clear();
                        moves.draining = false;
                        reports.settled.notify_waiters();
                        applying.stopped = true;
                        None
                    }
                }
            };
            let Some(next) = next else {
                return;
            };
            // A wait on a sign-in is applied only once it has stood for the settle: the report
            // that replaces it within that time is applied from the state before it, as if the
            // wait had never been reported. A session that is no longer this launch's ends the
            // hold, and the next round drops what is left; so does the end of the process. A
            // daemon that has begun to stop drops the held move too, and applies nothing more of
            // it: the daemon that adopts the terminal finds the record and holds it again
            // (`note_restored_program_status`). Whoever waits for the moves to settle does not
            // wait for a held one (`Moves::settled`): the planned stop's settle above all, which
            // has one short bound for every session.
            //
            // An `idle` before the turn of the launch's own prompt has started is held the same
            // way, for its own settle: the `working` that follows it is applied as if the agent
            // had gone from its launch straight into the turn.
            let this_launch = || {
                state
                    .live_session(&id)
                    .filter(|live| live.launch_id == launch_id)
            };
            let before_launch_turn = next.state == Some(ReportedState::Idle)
                && !state.turn_is_open(&id)
                && this_launch().is_some_and(|live| live.awaits_launch_turn());
            let settle = if settles_first(&next) {
                Some(auth_settle)
            } else if before_launch_turn {
                Some(launch_idle_settle)
            } else {
                None
            };
            let ended = || {
                state.is_shutting_down()
                    || state
                        .live_session(&id)
                        .is_none_or(|live| live.launch_id != launch_id || live.has_exited())
            };
            if let Some(settle) = settle {
                let held_until = Instant::now() + settle;
                reports.moves().holding = true;
                reports.settled.notify_waiters();
                let replaced = loop {
                    if !reports.moves().pending.is_empty() || ended() {
                        break true;
                    }
                    if Instant::now() >= held_until {
                        break false;
                    }
                    std::thread::sleep(AUTH_POLL);
                };
                reports.moves().holding = false;
                if replaced {
                    continue;
                }
            }
            // A `done` straight after `working`, inside a turn, of a provider that declares such a
            // `done` may be a gap between model calls: passed over where `working` follows it
            // within the bound, applied otherwise. A launch that is no longer the session's has it
            // dropped by the next round; the end of the process and a daemon that begins to stop
            // have it applied at once, as the last word on the turn. `holding` stays clear, so
            // whoever settles the moves waits for this one.
            let in_turn_done = next.state == Some(ReportedState::Done)
                && reports.moves().applied == Some(ReportedState::Working)
                && state.turn_is_open(&id);
            if let Some(bound) = done_gap.filter(|_| in_turn_done) {
                let held_until = Instant::now() + bound;
                let passed_over = loop {
                    if let Some(following) = reports.moves().pending.front() {
                        break following.state == Some(ReportedState::Working);
                    }
                    if this_launch().is_none() {
                        break true;
                    }
                    if ended() || reports.done_gaps_ended() || Instant::now() >= held_until {
                        break false;
                    }
                    std::thread::sleep(AUTH_POLL);
                };
                if passed_over {
                    continue;
                }
            }
            // The launch's turn has started, or its agent stayed at the prompt without taking it:
            // either way an `idle` from now on is what it says.
            if before_launch_turn || next.state == Some(ReportedState::Working) {
                if let Some(live) = this_launch() {
                    live.awaits_launch_turn
                        .store(false, std::sync::atomic::Ordering::Release);
                }
            }
            let previous = std::mem::replace(&mut reports.moves().applied, next.state);
            apply_within(&state, &id, previous, next, message_bound);
        });
    }

    /// Ends the window every `done` this launch's reports hold as a gap inside a turn waits in, the
    /// one held now and those held from now on ([`done_gap_window`]): for a test, the generic ones
    /// and a provider's own alike, whose turn's last `done` is to be applied. A `working` already
    /// taken in when the `done` is examined still passes it over, so a test that has fed every
    /// report before calling this keeps the gaps between them passed over.
    #[cfg(test)]
    pub(crate) fn end_reported_done_gaps(&self) {
        self.status_reports
            .done_gaps_ended
            .store(true, std::sync::atomic::Ordering::Release);
    }

    /// Waits until every move the program's reports made so far has been applied: for the end of
    /// the process, where a report read before the exit, the program's last `done` above all,
    /// decides how its turn ended before the exit is recorded, and for whatever else reads the
    /// turn those moves change. Bounded as a hook queue's settling is (`HookQueues::settled`);
    /// where the bound runs out, the exit finds the turn cut short, and a move applied after it
    /// is refused ([`apply`]). Only what the PTY reader has taken in counts, so the end of the
    /// process calls this once the reader has drained, or its own bounded wait for that ran out.
    pub(crate) async fn settle_program_status(&self) {
        let reports = self.status_reports.clone();
        let wait = async {
            loop {
                let woken = reports.settled.notified();
                tokio::pin!(woken);
                // Registered before the check, so a task settling between the two is not missed.
                woken.as_mut().enable();
                {
                    if reports.moves().settled() {
                        return;
                    }
                }
                woken.await;
            }
        };
        let _ = tokio::time::timeout(crate::hooks::queue::settle_bound(), wait).await;
    }
}

/// The task applying a launch's moves. However it stops without saying so, a panic in applying a
/// move or a runtime refusing to start it, the moves left are dropped and `draining` cleared, so
/// no settling waits on a task that is gone and the next move starts another.
struct Applying {
    reports: Arc<StatusReports>,
    /// The task stopped as it should, having cleared `draining` itself.
    stopped: bool,
}

impl Drop for Applying {
    fn drop(&mut self) {
        if self.stopped {
            return;
        }
        let mut moves = self
            .reports
            .moves
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        moves.pending.clear();
        moves.draining = false;
        moves.holding = false;
        self.reports.settled.notify_waiters();
    }
}

/// Applies one move of the root record, from `previous` to `next`: the turn it starts or ends,
/// the pause and the status, and what the status sets going, in the order a hook's are. A turn
/// that ends on a `done` or an `error` report takes the report's `msg`, where it has one, as its
/// final message: the reply for `done`, the failure's text for `error`, as a hook's turn end
/// carries them, unless the provider supplied the turn's message from a source of its own, which
/// then stands in its place ([`turn_message`]), or declares that its report's `msg` is not the
/// turn's message, which then never is. A turn that ends on `idle` was interrupted: it is
/// closed with no report, as a hook's cancelled turn is, and leaves the session paused. A turn
/// whose record went away has no message.
#[cfg(test)]
pub(super) fn apply(
    state: &Arc<AppState>,
    id: &str,
    previous: Option<ReportedState>,
    next: impl Into<RootMove>,
) {
    apply_within(state, id, previous, next, turn_message::bound());
}

/// [`apply`], with a turn end waiting up to `message_bound` for the message its provider supplies.
fn apply_within(
    state: &Arc<AppState>,
    id: &str,
    previous: Option<ReportedState>,
    next: impl Into<RootMove>,
    message_bound: Duration,
) {
    let RootMove {
        state: next, msg, ..
    } = next.into();
    // A move the program made just before its process ended reaches a session without one: the
    // end has settled its turn, and nothing of the move may open another.
    if state
        .session_record(id)
        .is_ok_and(|session| session.status.has_no_process())
    {
        return;
    }
    let was_held = previous.is_some_and(ReportedState::holds_the_session);
    let held = next.is_some_and(ReportedState::holds_the_session);
    // `working` opens the turn. A `blocked` reported with no turn open is a wait outside one, a
    // screen the program shows as it starts (a folder to trust, a sign-in): it opens none, so
    // going back to `idle` from it ends none and reports nothing.
    let live = state.live_session(id);
    let turn_was_open = state.turn_is_open(id);
    let mut interrupted = false;
    if next == Some(ReportedState::Working) && !state.turn_is_open(id) {
        if let Err(err) = state.mark_conversation_started(id) {
            tracing::debug!(session = %id, %err, "recording the session's first turn failed");
        }
        if let Some(live) = &live {
            live.turn_messages.turn_boundary();
            // The calls a prompt can be matched to are the turn's, as a hook's turn start keeps
            // them.
            live.interaction.turn_started();
        }
        state.turn_started(id, None);
    } else if was_held && !held && next == Some(ReportedState::Idle) && state.turn_is_open(id) {
        // Back to `idle` with no `done` or `error`: the turn was cancelled. No report is owed, the
        // user or the owner did it and knows, but the turn still has to be closed, and the session
        // reads paused, as after a cancel a hook reports. An `idle` with no turn open (the one a
        // program reports as it starts, or after a screen it showed before any turn) ends none.
        interrupted = true;
        if let Some(live) = &live {
            live.turn_messages.turn_boundary();
        }
        state.abandon_turn(id);
        state.write_dropped_messages_again(id);
        crate::hooks::apply::start_own_queue(state, id);
    } else if was_held && !held {
        let ended = matches!(next, Some(ReportedState::Done | ReportedState::Error));
        // What the provider read for the turn replaces the report's `msg`, waited for only where a
        // turn is there to take it.
        let msg_stands = live
            .as_ref()
            .is_none_or(|live| live.turn_messages.report_msg_stands());
        let supplied = live.as_ref().and_then(|live| {
            if ended && state.turn_is_open(id) {
                live.turn_messages.take(message_bound, || {
                    live.has_exited() || state.is_shutting_down()
                })
            } else {
                live.turn_messages.turn_boundary();
                None
            }
        });
        reporting::close_turn_and_report(
            state,
            id,
            TurnEnd {
                last_assistant_message: supplied.or_else(|| {
                    msg.filter(|_| ended && msg_stands)
                        .filter(|msg| !msg.trim().is_empty())
                }),
                failed: next == Some(ReportedState::Error),
                backstop: false,
            },
        );
    } else if held {
        state.touch_turn(id);
    }
    let status = wire_status(next);
    // The pause is how the last turn ended, so only a turn's end sets or clears it (an `error`
    // pauses whether or not a turn was open): a move to the prompt that ends no turn, the `idle`
    // after a screen answered outside one say, leaves it as that turn left it.
    if status == SessionStatus::Idle {
        let failed = next == Some(ReportedState::Error);
        if interrupted || failed {
            state.set_paused(id, true);
        } else if turn_was_open && was_held {
            state.set_paused(id, false);
        }
    }
    // The prompt is read off the screen for as long as the session waits on the user, as on a
    // hook's wait.
    if status == SessionStatus::WaitingUser {
        if let Some(live) = &live {
            crate::session::interaction::watch::ensure(state, live);
        }
    }
    match state.apply_hook_status(id, status) {
        // Telling the owner writes into its PTY, which blocks.
        Ok(true) if status == SessionStatus::WaitingUser => {
            let (state, id) = (state.clone(), id.to_string());
            tokio::task::spawn_blocking(move || {
                crate::messaging::owner_notices::tell_owner_it_waits(&state, &id)
            });
        }
        Ok(_) => {}
        Err(err) => {
            tracing::debug!(session = %id, %err, "applying the program's reported status failed");
        }
    }
    state.spawn_flush_outbox(id, status);
}

#[cfg(test)]
mod tests;
