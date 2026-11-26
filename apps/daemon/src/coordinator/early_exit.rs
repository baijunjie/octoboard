//! The early-exit relaunch (`docs/plans/terminal-first-degradation.md`, D6 and milestone 04): a
//! CLI that exits by itself within [`EARLY_EXIT_WINDOW`] of its start, before it showed any sign
//! of an interactive program and with no key typed into it by a client, is started again one tier
//! down, once per tier. The exit watcher asks [`relaunch_if_early`] before it records an exit; an
//! operation that follows its own launch come up under its claim is handed the exit instead and
//! relaunches under that claim (`super::restart::came_up_under_claim`). A lower tier's notices are
//! raised once it has shown startup evidence or outlived the window ([`raise_once_up`]), and a
//! lower tier that stayed up is remembered for its program and version.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::adapter::{EarlyExit, LaunchTier};
use crate::protocol::{Notice, Session, SessionStatus};
use crate::session::relaunch::EarlyExitRelaunch;
use crate::session::LiveSession;
use crate::state::AppState;

/// How long after its start a CLI's exit by itself can count as an exit at start. It only bounds
/// the watch: the evidence of what did not happen carries the decision.
#[cfg_attr(test, allow(dead_code))]
pub(crate) const EARLY_EXIT_WINDOW: Duration = Duration::from_secs(10);

/// How long the exit watcher waits for another operation's claim on the session to be released
/// before it gives the relaunch up: the opening that launched the process still holds its claim
/// for a moment after the process registered.
#[cfg_attr(test, allow(dead_code))]
const CLAIM_WAIT: Duration = Duration::from_secs(5);

/// How much of what the CLI wrote is read, from the end of its output: the head of it holds the
/// CLI's first lines unless it wrote more than this before it exited.
const OUTPUT_READ: usize = 256 * 1024;

/// How much of the plain text read is kept from its start as the early exit's output: a CLI can
/// print its refusal first and a long usage text after it (Qwen Code 0.28.3 prints 17 KB).
const OUTPUT_HEAD_KEPT: usize = 2 * 1024;

/// How much of the plain text read is kept from its end as the early exit's output: where a CLI
/// prints its refusal last, after what it drew.
const OUTPUT_TAIL_KEPT: usize = 4 * 1024;

/// What stands between the head and the tail kept of a longer output, on a line of its own.
const OUTPUT_ELIDED: &str = "…";

#[cfg(test)]
thread_local! {
    static WINDOW_OVERRIDE: std::cell::Cell<Option<Duration>> = const { std::cell::Cell::new(None) };
    static CLAIM_WAIT_OVERRIDE: std::cell::Cell<Option<Duration>> =
        const { std::cell::Cell::new(None) };
}

/// [`EARLY_EXIT_WINDOW`], as long as `PATIENCE` under test unless a test set its own
/// ([`window_for_test`]). Read on the thread that starts the watch.
pub(crate) fn early_exit_window() -> Duration {
    #[cfg(test)]
    {
        WINDOW_OVERRIDE
            .with(std::cell::Cell::get)
            .unwrap_or(crate::test_support::PATIENCE)
    }
    #[cfg(not(test))]
    EARLY_EXIT_WINDOW
}

/// Sets the early-exit window for the watches this thread starts until the guard drops.
#[cfg(test)]
pub(crate) fn window_for_test(window: Duration) -> impl Drop {
    struct Reset(Option<Duration>);
    impl Drop for Reset {
        fn drop(&mut self) {
            WINDOW_OVERRIDE.with(|held| held.set(self.0));
        }
    }
    Reset(WINDOW_OVERRIDE.with(|held| held.replace(Some(window))))
}

/// [`CLAIM_WAIT`], as long as `PATIENCE` under test unless a test set its own
/// ([`claim_wait_for_test`]).
fn claim_wait() -> Duration {
    #[cfg(test)]
    {
        CLAIM_WAIT_OVERRIDE
            .with(std::cell::Cell::get)
            .unwrap_or(crate::test_support::PATIENCE)
    }
    #[cfg(not(test))]
    CLAIM_WAIT
}

/// Sets how long the exit watchers this thread starts wait for a held claim, until the guard
/// drops.
#[cfg(test)]
pub(crate) fn claim_wait_for_test(wait: Duration) -> impl Drop {
    struct Reset(Option<Duration>);
    impl Drop for Reset {
        fn drop(&mut self) {
            CLAIM_WAIT_OVERRIDE.with(|held| held.set(self.0));
        }
    }
    Reset(CLAIM_WAIT_OVERRIDE.with(|held| held.replace(Some(wait))))
}

/// The windows an exit watcher reads on the thread that starts it, since it runs as a task of its
/// own.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Windows {
    pub early_exit: Duration,
    pub claim_wait: Duration,
}

impl Windows {
    pub(crate) fn read() -> Self {
        Self {
            early_exit: early_exit_window(),
            claim_wait: claim_wait(),
        }
    }
}

/// What the decision on an ended process reads ([`tier_after`]).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ending {
    /// Nobody ended the process (`LiveSession::end`), and no planned stop or shutdown is under
    /// way.
    pub by_itself: bool,
    /// The process is still the session's current one.
    pub current: bool,
    /// The session's record is there and not archived.
    pub recorded: bool,
    /// How long after its start the process was seen gone.
    pub lived: Duration,
    /// The early-exit window.
    pub window: Duration,
    /// The process showed a sign of an interactive program, or a helper of its launch reached the
    /// daemon (`crate::session::startup`).
    pub shown: bool,
    /// A client typed a key into it.
    pub typed: bool,
    /// The tier the process was launched at.
    pub tier: LaunchTier,
}

/// The highest tier the launch after `ending` may use, when the process exited as it started:
/// ended by itself, while still the session's, its record neither archived nor deleted, inside
/// the window, with nothing shown and nothing typed, and a tier below its own left. `None`
/// otherwise.
pub(crate) fn tier_after(ending: &Ending) -> Option<LaunchTier> {
    let early = ending.by_itself
        && ending.current
        && ending.recorded
        && ending.lived <= ending.window
        && !ending.shown
        && !ending.typed;
    if !early {
        return None;
    }
    match ending.tier {
        LaunchTier::Enhanced => Some(LaunchTier::Reduced),
        LaunchTier::Reduced => Some(LaunchTier::Bare),
        LaunchTier::Bare => None,
    }
}

/// The ending of `live`'s process, seen gone at `exited_at`, as [`tier_after`] reads it.
fn ending_of(
    state: &AppState,
    live: &Arc<LiveSession>,
    exited_at: Instant,
    window: Duration,
) -> Ending {
    let recorded = state
        .store
        .get_session(&live.id)
        .ok()
        .flatten()
        .is_some_and(|session| session.status != SessionStatus::Archived);
    Ending {
        by_itself: !live.was_ended_on_purpose()
            && !state.is_handing_over()
            && !state.is_shutting_down(),
        current: state.is_current(live),
        recorded,
        lived: exited_at.saturating_duration_since(live.startup.launched_at()),
        window,
        shown: live.startup.has_shown(),
        typed: live.startup.was_typed(),
        tier: live.integration().tier,
    }
}

/// What the CLI of `live` wrote, as plain text: all of it within a bound, or its head and its tail
/// ([`head_and_tail`]).
fn exit_of(live: &LiveSession) -> EarlyExit {
    let output = String::from_utf8_lossy(&live.recent_output(OUTPUT_READ)).into_owned();
    let plain = crate::terminal::plain_text::strip_escapes(&output);
    EarlyExit {
        output: head_and_tail(plain.trim(), OUTPUT_HEAD_KEPT, OUTPUT_TAIL_KEPT),
        status: None,
    }
}

/// An early exit as [`exit_of`] hands it over, from what a CLI wrote: made plain text and cut to
/// its head and its tail. For providers' tests replaying a captured exit.
#[cfg(test)]
pub(crate) fn exit_of_output(raw: &[u8]) -> EarlyExit {
    let plain = crate::terminal::plain_text::strip_escapes(&String::from_utf8_lossy(raw));
    EarlyExit {
        output: head_and_tail(plain.trim(), OUTPUT_HEAD_KEPT, OUTPUT_TAIL_KEPT),
        status: None,
    }
}

/// `text` whole when it is no longer than `head + tail` bytes; otherwise the whole lines of its
/// start within `head` bytes and those of its end within `tail` bytes, with [`OUTPUT_ELIDED`] on a
/// line between them. A line longer than its bound alone is cut at a character boundary: a
/// provider reads the output line by line, and a refusal stays on its line.
fn head_and_tail(text: &str, head: usize, tail: usize) -> String {
    if text.len() <= head + tail {
        return text.to_string();
    }
    let mut head_end = 0;
    for line in text.split_inclusive('\n') {
        if head_end + line.len() > head {
            break;
        }
        head_end += line.len();
    }
    if head_end == 0 {
        head_end = floor_char_boundary(text, head);
    }
    let mut tail_start = text.len();
    for line in text.rsplit_terminator('\n') {
        // The line and the newline before it.
        let taken = text.len() - tail_start + line.len() + 1;
        if taken > tail {
            break;
        }
        tail_start -= line.len() + 1;
    }
    if tail_start == text.len() {
        tail_start = ceil_char_boundary(text, text.len() - tail);
    }
    format!(
        "{}\n{OUTPUT_ELIDED}\n{}",
        text[..head_end].trim_end(),
        text[tail_start..].trim_start_matches('\n')
    )
}

/// The highest character boundary of `text` at or below `at`.
fn floor_char_boundary(text: &str, at: usize) -> usize {
    (0..=at.min(text.len()))
        .rev()
        .find(|&at| text.is_char_boundary(at))
        .unwrap_or(0)
}

/// The lowest character boundary of `text` at or above `at`.
fn ceil_char_boundary(text: &str, at: usize) -> usize {
    (at..=text.len())
        .find(|&at| text.is_char_boundary(at))
        .unwrap_or(text.len())
}

/// The relaunch one tier down that `live`'s ending calls for, if its process exited as it started
/// and the launch kept what it was asked; `None` otherwise. `ran_as` is the record as the process
/// ran under it.
fn relaunch_for(
    state: &AppState,
    live: &Arc<LiveSession>,
    exited_at: Instant,
    window: Duration,
    ran_as: Option<&Session>,
) -> Option<EarlyExitRelaunch> {
    let tier = tier_after(&ending_of(state, live, exited_at, window))?;
    let mut asked = live.startup.asked()?;
    // The task the launch queued as its first message is still queued, and goes to the relaunch
    // as it is rather than a second time.
    if live.first_task_queued() {
        asked.task = None;
    }
    let previous_status = state.store.get_session(&live.id).ok().flatten()?.status;
    Some(EarlyExitRelaunch {
        asked,
        exit: exit_of(live),
        tier,
        previous_status,
        ran_as: ran_as.cloned(),
    })
}

/// Sets the session's record back to starting for a relaunch, from `previous`: what waits for it
/// to come up or go down counts it as still coming up. Whether it was still at `previous`.
fn back_to_starting(state: &AppState, id: &str, previous: SessionStatus) -> bool {
    if previous == SessionStatus::Starting {
        return true;
    }
    match state
        .store
        .update_session_lifecycle_if(id, previous, SessionStatus::Starting, None)
    {
        Ok(Some(session)) => {
            state.publish_session(&session);
            true
        }
        Ok(None) => false,
        Err(err) => {
            tracing::warn!(session = %id, %err, "setting a relaunched session back to starting failed");
            false
        }
    }
}

/// Relaunches `live`'s session one tier down if its process, seen gone at `exited_at`, exited as
/// it started, and answers whether it took the exit over: the caller then records nothing of it.
/// Called by the exit watcher before it records the exit, with the process still registered.
///
/// An operation that holds the session's claim and follows its launch come up is handed the exit
/// and the process unregistered, for it to relaunch under its claim. Otherwise the relaunch takes
/// the session's claim, waiting `windows.claim_wait` for another holder to let go (the work starts
/// from an exit that will not come again), checks again under it, keeps what is queued for the
/// session, and relaunches through the shared launch entry; whoever cannot take the claim does
/// not relaunch. A relaunch that fails leaves the session interrupted and its owner told, as an
/// exit nobody asked for, and what the process left of a sharing account's registries put back,
/// as for any exit, from `ran_as`, the record as the process ran under it ([`relaunch_failed`]).
pub(crate) async fn relaunch_if_early(
    state: &Arc<AppState>,
    live: &Arc<LiveSession>,
    exited_at: Instant,
    windows: Windows,
    ran_as: Option<&Session>,
) -> bool {
    if !super::launch::can_relaunch(state)
        || relaunch_for(state, live, exited_at, windows.early_exit, ran_as).is_none()
    {
        return false;
    }
    if live.is_followed_under_claim() {
        let Some(relaunch) = relaunch_for(state, live, exited_at, windows.early_exit, ran_as)
        else {
            return false;
        };
        if !back_to_starting(state, &live.id, relaunch.previous_status) {
            return false;
        }
        tracing::info!(
            session = %live.id,
            from = ?live.integration().tier,
            to = ?relaunch.tier,
            "the agent's CLI exited as it started; its launch is relaunched one tier down by the operation following it"
        );
        state.hand_early_exit(&live.id, relaunch);
        super::exit_watch::unregister_live(state, live);
        return true;
    }
    let deadline = Instant::now() + windows.claim_wait;
    let claim = loop {
        match state.begin_switch(&live.id) {
            Ok(claim) => break claim,
            Err(_) if Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(err) => {
                tracing::info!(session = %live.id, %err, "the agent's CLI exited as it started, but another operation holds the session; not relaunched");
                return false;
            }
        }
    };
    // Everything decided before the claim, read again under it.
    let Some(relaunch) = relaunch_for(state, live, exited_at, windows.early_exit, ran_as) else {
        return false;
    };
    // Kept across the end of the process for the relaunch, as a restart keeps it; not a hold
    // another operation already has on it.
    let hold = (!state.outbox.is_held(&live.id)).then(|| state.hold_outbox_for_restart(&live.id));
    if !back_to_starting(state, &live.id, relaunch.previous_status) {
        return false;
    }
    tracing::info!(
        session = %live.id,
        from = ?live.integration().tier,
        to = ?relaunch.tier,
        "the agent's CLI exited as it started; relaunching it one tier down"
    );
    super::exit_watch::unregister_live(state, live);
    let id = live.id.clone();
    let ran_as = relaunch.ran_as.clone();
    if let Err(err) = super::launch::relaunch_after_early_exit(state, &claim, &id, relaunch).await {
        tracing::warn!(session = %id, %err, "relaunching the agent one tier down failed");
        drop(hold);
        relaunch_failed(state, &id, ran_as, true).await;
        return true;
    }
    // Released into the relaunched process once it is registered.
    drop(hold);
    drop(claim);
    true
}

/// Leaves a session whose relaunch after an early exit failed interrupted, tells its owner of an
/// end nobody asked for where `tell_owner` (an operation following its launch reports the failure
/// itself), and, last, puts back what the exited process left of a sharing account's registries,
/// from `ran_as`, the record as it ran under it, as the exit watcher does after any exit. Shared by
/// the exit watcher's relaunch and an operation's (`super::restart::came_up_under_claim`).
pub(crate) async fn relaunch_failed(
    state: &Arc<AppState>,
    id: &str,
    ran_as: Option<Session>,
    tell_owner: bool,
) {
    match state.interrupt_session_record(id) {
        Err(err) => {
            tracing::warn!(session = %id, %err, "marking a session whose relaunch failed interrupted failed");
        }
        Ok(()) if tell_owner => {
            if let Ok(session) = state.session_record(id) {
                if session.bound_to.is_some() {
                    let state = state.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        crate::messaging::owner_notices::tell_owner_it_ended(&state, &session)
                    })
                    .await;
                }
            }
        }
        Ok(()) => {}
    }
    if let Some(session) = ran_as {
        crate::accounts::shared_config::keep_registries_after_exit(state, &session).await;
    }
}

/// Raises the notices of `live`, a launch one tier down after an early exit, once it has shown
/// startup evidence or outlived `window`, and remembers its tier for its program and version
/// then; if it exits at start too, nothing is raised for it.
pub(crate) fn raise_once_up(
    state: &Arc<AppState>,
    live: &Arc<LiveSession>,
    notices: Vec<Notice>,
    notices_at: i64,
    window: Duration,
) {
    let (state, live) = (state.clone(), live.clone());
    tokio::spawn(async move {
        let left = window.saturating_sub(live.startup.launched_at().elapsed());
        tokio::select! {
            () = live.startup.until_shown() => {}
            () = tokio::time::sleep(left) => {}
        }
        if !state.is_current(&live) || live.has_exited() {
            return;
        }
        for notice in notices {
            if notice.standing {
                state.raise_standing_notice(&live.id, notice, notices_at);
            } else {
                state.broadcast(notice.about(&live.id));
            }
        }
        let exit = live.startup.asked().and_then(|asked| asked.early_exit);
        if let (Some(key), Some(exit)) = (live.startup.remembered_as(), exit) {
            state
                .lower_tiers
                .remember(key, live.integration().tier, exit);
        }
    });
}

#[cfg(test)]
mod tests;
