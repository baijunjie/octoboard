//! A run of Ctrl-U presses that empties an agent's input box, one press a write, each seen on the
//! screen before the next: how an interrupt clears the prompt the agent puts back
//! (`crate::session::interrupt`), and how a draft is cleared out of the way of a message past the
//! draft's grace (`crate::session::draft_clear`). Only for an agent whose Ctrl-U is measured to
//! delete a line of its box a press
//! (`crate::session::screen::ScreenReader::clears_input_box_with_ctrl_u`). Each press goes through
//! `LiveSession::write_keys_if` alone, with its gate re-checked under the input lock, and is never
//! repeated blind: a press that does not show ends the run.

use std::time::{Duration, Instant};

use crate::platform::term::Input;
use crate::session::wait::wait_for;
use crate::session::LiveSession;

const CTRL_U: &[Input] = &[Input::Ctrl('u')];

#[cfg(test)]
thread_local! {
    pub(super) static PRESS_OVERRIDE: std::cell::Cell<Option<Duration>> = const { std::cell::Cell::new(None) };
}

/// How long a Ctrl-U has to show on the screen, as a change in the input box's rows. The wait ends
/// as soon as it shows, so the figure only bounds how long a press that changes nothing takes to
/// say so; and since a press not seen ends its run ([`clear_input_box`]), it is spent at most once
/// a run. It is the agent's pace on the slowest host that sets it: on an ARM64 Windows machine
/// running an x64 daemon under emulation, Claude Code took up to 288 ms to move its cursor after a
/// Down at its trust screen, and an agent mid-turn, or a busier machine, can take longer (the
/// figures are under "What they draw and read on a pseudoconsole" in
/// `docs/agent-cli-reference.md`). A bound shorter than the agent's answer reads as a press that
/// did nothing.
#[cfg_attr(test, allow(dead_code))]
const PRESS_TIMEOUT: Duration = Duration::from_secs(5);

/// [`PRESS_TIMEOUT`], except under test. A stand-in draws each press from a thread polling what a
/// `cat` on a PTY has written, which under the suite's load can take longer than a real agent, so
/// tests wait up to `PATIENCE` (the wait returns as soon as the press shows), and a test proving
/// that a press nobody draws ends the run lowers it with `PRESS_OVERRIDE`. Read on the caller's
/// thread, before the press moves onto a blocking one. Under test the run's own budget
/// ([`clear_allowance`]) is sized so that it never cuts such a wait short.
pub(super) fn press_timeout() -> Duration {
    #[cfg(test)]
    {
        PRESS_OVERRIDE
            .with(std::cell::Cell::get)
            .unwrap_or(crate::test_support::PATIENCE)
    }
    #[cfg(not(test))]
    PRESS_TIMEOUT
}

/// The most Ctrl-U presses spent on an input box, however many rows it takes up.
const MAX_PRESSES: usize = 40;

/// The least a run of presses on an input box is given in all, however few it plans.
const CLEAR_FLOOR: Duration = Duration::from_secs(15);

/// What a run of presses on an input box is given for each press it plans: the
/// run's budget is the larger of [`CLEAR_FLOOR`] and this times its presses, so that an agent
/// which shows each press late, but within [`PRESS_TIMEOUT`], cannot keep the run, and the
/// session's paused queue, going for [`MAX_PRESSES`] of them, while a long text on a slow host
/// (21 presses at three quarters of a second each) still gets all the presses it needs. A second
/// is well above the slowest key echo measured, 288 ms (see [`PRESS_TIMEOUT`]). A run that
/// reaches its budget ends as a press not seen does.
#[cfg_attr(test, allow(dead_code))]
const CLEAR_PER_PRESS: Duration = Duration::from_secs(1);

/// [`CLEAR_PER_PRESS`], except under test, where it is `PATIENCE`: the budget then covers every
/// press waiting as long as [`press_timeout`] allows it. Read on the caller's thread, next to
/// [`press_timeout`], before the press moves onto a blocking one.
pub(super) fn clear_allowance() -> Duration {
    #[cfg(test)]
    {
        crate::test_support::PATIENCE
    }
    #[cfg(not(test))]
    CLEAR_PER_PRESS
}

/// How a run of Ctrl-U presses on an input box ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Cleared {
    /// The screen shows the box empty.
    Empty,
    /// A press was not written: someone else wrote, or the run's `allowed` check failed.
    Refused,
    /// The presses ran out, a press did not show, or the box could no longer be read, with text
    /// still in it.
    Left,
}

/// Presses Ctrl-U into the agent's input box, one at a time, each written only while nobody else
/// has written since the last and while `allowed` says so, checked under the input lock, until the
/// screen shows the box empty, a press has not shown within `shown`, or the run's budget has run
/// out: the larger of [`CLEAR_FLOOR`] and `allowance` ([`clear_allowance`]) for each press it
/// plans. Returns how it ended, how many presses went in, whether any of them showed as a change
/// in the box, and whether the last one did not (a press nobody saw); `writes` is moved past each.
pub(super) fn clear_input_box(
    live: &LiveSession,
    writes: &mut u64,
    shown: Duration,
    allowance: Duration,
    allowed: impl Fn() -> bool,
) -> (Cleared, usize, bool, bool) {
    let Some(text) = live.input_box(&live.screen_text_undimmed()) else {
        return (Cleared::Left, 0, false, false);
    };
    // A press clears a line's text and the next its line break, so two a row and one to spare.
    let presses = (text.lines().count() * 2 + 1).min(MAX_PRESSES);
    let deadline = Instant::now() + CLEAR_FLOOR.max(allowance * presses as u32);
    let mut pressed = 0;
    let mut changed = false;
    let mut unseen = false;
    for _ in 0..presses {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        let screen = live.screen_text_undimmed();
        match live.input_box(&screen) {
            Some(text) if text.is_empty() => return (Cleared::Empty, pressed, changed, false),
            Some(_) => {}
            None => return (Cleared::Left, pressed, changed, false),
        }
        let before = live.input_box_rows(&screen);
        match live.write_keys_if(*writes, &allowed, CTRL_U) {
            Ok(true) => {
                *writes += 1;
                pressed += 1;
            }
            Ok(false) => return (Cleared::Refused, pressed, changed, false),
            Err(_) => return (Cleared::Left, pressed, changed, false),
        }
        // The box's own reading, not the screen: a spinner turning elsewhere changes the screen
        // before the press has shown. A press not seen ends the run: the next would be decided
        // on a box the agent may not have redrawn yet, a press sent blind, and a run of those
        // spends its presses before the screen catches up and then reads the box as left full.
        // A Ctrl-U that changes nothing in the box, as at its very start, where there is neither
        // text nor a line break before the cursor, leaves nothing for the next to change either.
        // No press is waited for past the run's budget.
        if !wait_for(shown.min(left), || {
            live.input_box_rows(&live.screen_text_undimmed()) != before
        }) {
            unseen = true;
            break;
        }
        changed = true;
    }
    match live.input_box(&live.screen_text_undimmed()) {
        Some(text) if text.is_empty() => (Cleared::Empty, pressed, changed, false),
        _ => (Cleared::Left, pressed, changed, unseen),
    }
}
