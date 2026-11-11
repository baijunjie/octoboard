//! Pressing a session's trust screen: the keys each agent's screen takes, and the checks made on
//! the terminal before and after each of them. One of the two places that type keys of their own
//! into a session, with `crate::session::interrupt`.

use super::*;

use crate::platform::term::Input;
use crate::session::wait::{quiet, wait_for, wait_quiet};

/// How long to let the agent finish drawing and start reading its input before the first key.
pub(super) const SETTLE: Duration = Duration::from_millis(300);

/// How long the agent may take to answer one of the press's keys: the cursor to move after the
/// Down, the terminal to go quiet after it, and the screen to go after the Enter. Each wait ends
/// as soon as what it waits for is seen, so the figure only bounds how long a press that really
/// failed takes to say so. It is generous because the agent's own pace sets it: on an ARM64
/// Windows machine running Claude Code 2.1.322, Claude Code drew its next screen 4 to 7 seconds
/// after the Enter (measured), the screen blank for seconds in between, and a busier machine
/// takes longer still. Nothing is pressed late because of it: the Enter still goes only on the
/// cursor seen on the accepting option, the terminal quiet and nothing else written meanwhile.
#[cfg_attr(test, allow(dead_code))]
pub(super) const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

/// [`RESPONSE_TIMEOUT`], except under test. A stand-in answers each key through a PTY and a shell,
/// so tests wait up to `PATIENCE` (each wait returns as soon as its condition holds), and a test
/// proving that a wait runs out lowers it with `RESPONSE_OVERRIDE` (or, for the wait after the
/// Enter alone, `DISMISS_OVERRIDE`). Read on the caller's thread, before the press moves onto a
/// blocking one.
pub(super) fn response_timeout() -> Duration {
    #[cfg(test)]
    {
        RESPONSE_OVERRIDE
            .with(std::cell::Cell::get)
            .unwrap_or(crate::test_support::PATIENCE)
    }
    #[cfg(not(test))]
    RESPONSE_TIMEOUT
}

#[cfg(test)]
thread_local! {
    pub(super) static RESPONSE_OVERRIDE: std::cell::Cell<Option<Duration>> =
        const { std::cell::Cell::new(None) };
    pub(super) static DISMISS_OVERRIDE: std::cell::Cell<Option<Duration>> =
        const { std::cell::Cell::new(None) };
}

/// How long the screen has to go after the Enter, or a hook to come: [`response_timeout`], except
/// where a test lowers just this wait with `DISMISS_OVERRIDE`, which only a press started through
/// [`answer`] reads. A test proving that the screen is not dismissed needs the waits before the
/// Enter to succeed, which a lowered `RESPONSE_OVERRIDE` would put at risk on a slow host.
#[cfg(test)]
fn dismiss_timeout() -> Duration {
    DISMISS_OVERRIDE
        .with(std::cell::Cell::get)
        .unwrap_or_else(response_timeout)
}

/// How long Grok's press waits for its hook and the carry the hook makes.
pub(super) const CARRY_WAIT: Duration = Duration::from_secs(12);

/// The keys: the Down arrow, Enter, and ([`yes`]) `y`, Grok's own key for "Yes, proceed".
pub(super) const DOWN: &[Input] = &[Input::Down];
pub(super) const ENTER: &[Input] = &[Input::Enter];

pub(super) fn yes() -> [Input; 1] {
    [Input::Text("y".to_string())]
}

/// The point a key was sent at, which what the press reads after the key is measured from: how
/// much the session had printed by then, and, where the parsed screen is read
/// ([`TrustState::reads_screen`]), what it showed.
struct Mark {
    output: u64,
    screen: Option<String>,
}

impl Mark {
    fn now(live: &LiveSession) -> Self {
        Self {
            output: live.output_total(),
            screen: live.trust.reads_screen().then(|| live.screen_text()),
        }
    }
}

/// The terminal as a press looks at it before a key: the recent output, at most [`WINDOW`] of it,
/// or the parsed screen.
fn recent(live: &LiveSession) -> Vec<u8> {
    if live.trust.reads_screen() {
        live.screen_text().into_bytes()
    } else {
        live.recent_output(WINDOW)
    }
}

/// The terminal as a press looks at it after the key `mark` was taken at: what was printed since,
/// or the parsed screen.
fn since(live: &LiveSession, mark: &Mark) -> Vec<u8> {
    if mark.screen.is_some() {
        live.screen_text().into_bytes()
    } else {
        live.output_since(mark.output)
    }
}

/// Whether the screen is on `terminal`, as [`recent`] read it: first drawn in the recent output, or
/// up on the parsed screen wherever its cursor is ([`Screen::is_up_in`]), which no longer holds
/// the first drawing once the cursor has moved.
fn shown(live: &LiveSession, screen: &Screen, terminal: &[u8]) -> bool {
    if live.trust.reads_screen() {
        screen.is_up_in(terminal)
    } else {
        screen.is_shown_in(terminal)
    }
}

/// Which option the cursor is on in `terminal`, as [`recent`] or [`since`] read it: by its glyph,
/// and, on the parsed screen only, by the option drawn in inverse video where no glyph says
/// ([`Screen::cursor_in_inverse`]).
fn cursor(live: &LiveSession, screen: &Screen, terminal: &[u8]) -> Option<Cursor> {
    screen.cursor_on(terminal).or_else(|| {
        live.trust
            .reads_screen()
            .then(|| screen.cursor_in_inverse(live.screen_text_inverse().as_bytes()))
            .flatten()
    })
}

/// Whether the screen is gone since Octoboard's own key, `mark`: the output printed since shows it
/// gone ([`Screen::gone_in`]), or the parsed screen has changed since and shows it gone
/// ([`Screen::gone_from`]).
fn gone(live: &LiveSession, screen: &Screen, mark: &Mark) -> bool {
    match &mark.screen {
        None => screen.gone_in(&live.output_since(mark.output)),
        Some(before) => screen.gone_from(before.as_bytes(), live.screen_text().as_bytes()),
    }
}

/// [`answer_within`], with the agent given [`response_timeout`] to answer each key, and
/// [`dismiss_timeout`] to dismiss the screen after the Enter.
#[cfg(test)]
pub(super) fn answer(live: &LiveSession) -> Result<()> {
    answer_with(live, response_timeout(), dismiss_timeout())
}

/// Sends the keys that accept the session's trust confirmation, checking before each one that the
/// screen is as it must be, and giving the agent `patience` to answer each key. Blocks, for the
/// settle time and while it watches the screen respond.
///
/// Like `crate::session::interrupt`, the other place that writes keystrokes of its own into a
/// session, it takes no text: the bytes are the constants above, so nothing a model or a message
/// wrote can reach it.
pub(super) fn answer_within(live: &LiveSession, patience: Duration) -> Result<()> {
    answer_with(live, patience, patience)
}

/// [`answer_within`], with the screen given `dismissal` rather than `patience` to go after the
/// Enter.
fn answer_with(live: &LiveSession, patience: Duration, dismissal: Duration) -> Result<()> {
    if !live.trust.claim_answer() || !still_waiting_after_claim(live) {
        return Err(not_waiting());
    }
    let screen = live.trust.screen;
    let pressed = match screen.press {
        Press::DownThenEnter => press_down_then_enter(live, screen, patience, dismissal),
        Press::Enter => press_enter(live, screen, patience, dismissal),
        Press::Yes => press_yes(live),
    };
    match pressed {
        Ok(()) => {
            live.trust.pressed.store(true, Ordering::Release);
            Ok(())
        }
        // Whatever stopped the press, a person who has typed at a screen that input ends — a
        // Down, an `n` — has it in hand; there is nothing to tell them. A carry of their own
        // answer that failed is still told.
        Err(err)
            if screen.input_ends_wait
                && live.trust.input_since_sighting(live.input_writes())
                && err
                    .downcast_ref::<CodedError>()
                    .is_none_or(|coded| coded.code != error_code::TRUST_NOT_CARRIED_OVER) =>
        {
            Err(not_waiting())
        }
        Err(err) => Err(err),
    }
}

/// Claude Code's: a Down from "No, exit", then an Enter on "Yes, I trust this folder".
pub(super) fn press_down_then_enter(
    live: &LiveSession,
    screen: &Screen,
    patience: Duration,
    dismissal: Duration,
) -> Result<()> {
    let agent = live.agent;
    // Read before anything is looked at: a write by anyone else after this point means what was
    // looked at may no longer be true.
    let writes = live.input_writes();
    std::thread::sleep(SETTLE);

    let output = recent(live);
    if !shown(live, screen, &output) {
        return Err(screen_gone(agent));
    }
    // A Down from the second option wraps round to the first, where the Enter would then exit.
    if cursor(live, screen, &output) != Some(Cursor::Decline) {
        return Err(cursor_not_at_start(agent));
    }

    let mark = Mark::now(live);
    write(live, writes, DOWN)?;
    if !wait_for(patience, || {
        cursor(live, screen, &since(live, &mark)) == Some(Cursor::Accept)
    }) {
        return Err(answer_failed(
            agent,
            trust_reason::CURSOR_DID_NOT_MOVE,
            "the cursor did not move to the option that trusts the folder, so Enter was not sent",
            &[],
        ));
    }
    // Quiet first, so that what is read next is the screen as it now stands, then a last look at
    // the cursor and at the input, right before the key that cannot be taken back.
    if !wait_quiet(live, patience) {
        return Err(not_settled(agent));
    }
    if cursor(live, screen, &since(live, &mark)) != Some(Cursor::Accept) {
        return Err(cursor_moved_away(agent));
    }
    enter_until_gone(live, screen, writes, dismissal)
}

/// Codex's: an Enter on "1. Trust and continue", where the cursor starts, while nothing has been
/// written into its input since the screen was sighted (one step with the key).
pub(super) fn press_enter(
    live: &LiveSession,
    screen: &Screen,
    patience: Duration,
    dismissal: Duration,
) -> Result<()> {
    let agent = live.agent;
    let writes = live.trust.writes_at_sighting.load(Ordering::Acquire);
    std::thread::sleep(SETTLE);

    let output = recent(live);
    if !shown(live, screen, &output) {
        return Err(screen_gone(agent));
    }
    // An Enter on the decline option quits or returns to the command center.
    if cursor(live, screen, &output) != Some(Cursor::Accept) {
        return Err(cursor_not_at_start(agent));
    }
    if !wait_quiet(live, patience) {
        return Err(not_settled(agent));
    }
    if cursor(live, screen, &recent(live)) != Some(Cursor::Accept) {
        return Err(cursor_moved_away(agent));
    }
    enter_until_gone(live, screen, writes, dismissal)
}

/// The Enter, and then the agent seen past the screen: its first hook has come, which it runs
/// only once it is past the screen, or the screen is seen gone ([`gone`]) and still gone a moment
/// later.
pub(super) fn enter_until_gone(
    live: &LiveSession,
    screen: &Screen,
    writes: u64,
    dismissal: Duration,
) -> Result<()> {
    let hooked = || live.trust.hook_seen.load(Ordering::Acquire);
    // A hook seen already shows the agent past the screen before this Enter: it is not an answer
    // to it, and the Enter would land in whatever the agent shows now.
    if hooked() {
        return Err(not_waiting());
    }
    let mark = Mark::now(live);
    write(live, writes, ENTER)?;
    if !wait_for(dismissal, || hooked() || gone(live, screen, &mark)) {
        return Err(not_dismissed(live.agent));
    }
    if hooked() {
        return Ok(());
    }
    std::thread::sleep(quiet());
    if !hooked() && !gone(live, screen, &mark) {
        return Err(answer_failed(
            live.agent,
            trust_reason::SCREEN_REDRAWN,
            "the confirmation was drawn again after the key was sent",
            &[],
        ));
    }
    Ok(())
}

/// [`still_waiting`] for the caller that has just claimed the answer: whether the person has
/// written into the terminal since the screen was sighted, for a screen that input ends.
pub(super) fn still_waiting_after_claim(live: &LiveSession) -> bool {
    !live.trust.screen.input_ends_wait || !live.trust.input_since_sighting(live.input_writes())
}

/// Grok's: a `y`, while nothing has shown the screen gone. Grok draws its screen once and then
/// animates its logo for as long as it waits, so the first drawing soon leaves the recent output
/// and the terminal is never quiet; what is checked instead is that no hook has run (the claim),
/// that the process is still there (a decline exits it), and that nothing has been written into
/// its input since the screen was sighted (one step with the key). The press counts once Grok has
/// run its hook and the entry it recorded has reached the user's own store, or, with no hook to
/// carry it (a policy that runs none), once the rendered screen is seen to have gone; a hook that
/// comes after that still carries the entry.
pub(super) fn press_yes(live: &LiveSession) -> Result<()> {
    let agent = live.agent;
    let trust = &live.trust;
    // Every launch of a provider whose trust is per session says where its trust record is
    // carried (`crate::adapter::LaunchPlan::carried_trust`).
    let Some(carried) = trust.carried.as_deref() else {
        return Err(anyhow!(
            "this {} session has nowhere to carry its trust record to",
            agent.label()
        ));
    };
    let writes = trust.writes_at_sighting.load(Ordering::Acquire);
    std::thread::sleep(SETTLE);
    if live.poll_exit() {
        return Err(screen_gone(agent));
    }
    // Grok runs `SessionStart` right after the press, and the hook carries the entry Grok then
    // writes (`super::on_hook`); the press waits for that to know whether it succeeded. Said
    // before the key, which is what the hook can follow.
    trust.await_carry();
    if let Err(err) = write(live, writes, &yes()) {
        // The person answered first. Should their answer have been carried and failed, that is
        // what there is to tell; otherwise the screen is simply theirs.
        return match trust.stop_awaiting_carry() {
            Some(Err(failure)) => Err(not_carried_over(agent, carried, &failure)),
            _ => Err(err),
        };
    }
    // Room for a hook that comes late. Once it has come, the carry it makes is bounded, and its
    // outcome is waited for in full, so that the press reports what really happened.
    let mut outcome = trust.carry_outcome(CARRY_WAIT);
    if outcome.is_none() && trust.hook_seen.load(Ordering::Acquire) {
        outcome = trust.carry_outcome(carried.longest());
    }
    // Anything handed over in the meantime is still this press's; from here on, a hook reports
    // its own outcome.
    let outcome = outcome.or_else(|| trust.stop_awaiting_carry());
    match outcome {
        Some(Ok(())) => Ok(()),
        Some(Err(failure)) => Err(not_carried_over(agent, carried, &failure)),
        // A carry that outlasted even that reports itself; nothing more to say here.
        None if trust.hook_seen.load(Ordering::Acquire) => Err(not_waiting()),
        // The watch for the rendered screen has seen it go with no hook to carry anything: the key
        // worked, so it counts as pressed, and the messages held are let through.
        None if trust.dismissed.load(Ordering::Acquire) => Ok(()),
        // No hook: the confirmation did not go. A hook that comes later reports its own outcome.
        None => Err(not_dismissed(agent)),
    }
}

pub(super) fn screen_gone(agent: AgentId) -> anyhow::Error {
    answer_failed(
        agent,
        trust_reason::SCREEN_GONE,
        "the trust confirmation is no longer on the terminal",
        &[],
    )
}

pub(super) fn cursor_not_at_start(agent: AgentId) -> anyhow::Error {
    answer_failed(
        agent,
        trust_reason::CURSOR_NOT_AT_START,
        "the cursor is not on the option it starts on, or could not be found",
        &[],
    )
}

pub(super) fn not_settled(agent: AgentId) -> anyhow::Error {
    answer_failed(
        agent,
        trust_reason::TERMINAL_NOT_SETTLED,
        "the terminal did not settle, so Enter was not sent",
        &[],
    )
}

pub(super) fn cursor_moved_away(agent: AgentId) -> anyhow::Error {
    answer_failed(
        agent,
        trust_reason::CURSOR_MOVED_AWAY,
        "the cursor is no longer on the option that trusts the folder, so Enter was not sent",
        &[],
    )
}

pub(super) fn not_dismissed(agent: AgentId) -> anyhow::Error {
    answer_failed(
        agent,
        trust_reason::SCREEN_NOT_DISMISSED,
        "the confirmation did not go away after the key was sent",
        &[],
    )
}

/// Writes the keys unless anyone else has written into the session's input since `writes` was read;
/// the check and the write are one step.
pub(super) fn write(live: &LiveSession, writes: u64, keys: &[Input]) -> Result<()> {
    match live.write_input_if_untouched(writes, keys) {
        Ok(true) => Ok(()),
        // For a screen that input ends, that was the person answering it in the terminal: it is
        // theirs, and there is nothing to tell them.
        Ok(false) if live.trust.screen.input_ends_wait => Err(not_waiting()),
        Ok(false) => Err(answer_failed(
            live.agent,
            trust_reason::INPUT_TOUCHED,
            "something else wrote into the terminal meanwhile, so the keys were not sent",
            &[],
        )),
        Err(err) => {
            let detail = err.to_string();
            Err(answer_failed(
                live.agent,
                trust_reason::TERMINAL_WRITE_FAILED,
                format!("writing to the terminal failed: {detail}"),
                &[("detail", &detail)],
            ))
        }
    }
}
