//! Pressing a session's trust screen: the keys each agent's screen takes, and the checks made on
//! the terminal before and after each of them. The only code that types into a session on its own.

use super::*;

/// How long the session's output has to stay silent before an Enter, and after it.
pub(super) const QUIET: Duration = Duration::from_millis(100);

/// How long to let the agent finish drawing and start reading its input before the first key.
pub(super) const SETTLE: Duration = Duration::from_millis(300);

/// How long the cursor has to take to move after the Down, and the screen to go after the last
/// key.
pub(super) const MOVE_TIMEOUT: Duration = Duration::from_secs(2);
pub(super) const DISMISS_TIMEOUT: Duration = Duration::from_secs(3);

/// How long Grok's press waits for its hook and the carry the hook makes.
pub(super) const CARRY_WAIT: Duration = Duration::from_secs(12);
pub(super) const POLL: Duration = Duration::from_millis(25);

/// The keys. Down is the sequence a terminal sends for the arrow key; Enter is `\r`, as the
/// terminal sends it; `y` is Grok's own key for "Yes, proceed".
pub(super) const DOWN: &[u8] = b"\x1b[B";
pub(super) const ENTER: &[u8] = b"\r";
pub(super) const YES: &[u8] = b"y";

/// Sends the keys that accept the session's trust confirmation, checking before each one that the
/// screen is as it must be. Blocks, for the settle time and while it watches the screen respond.
///
/// This is the only place that writes keystrokes of its own into a session, and it takes no text:
/// the bytes are the constants above, so nothing a model or a message wrote can reach it.
pub(super) fn answer(live: &LiveSession) -> Result<()> {
    if !live.trust.claim_answer() || !still_waiting_after_claim(live) {
        return Err(not_waiting());
    }
    let screen = live.trust.screen;
    let pressed = match screen.press {
        Press::DownThenEnter => press_down_then_enter(live, screen),
        Press::Enter => press_enter(live, screen),
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
pub(super) fn press_down_then_enter(live: &LiveSession, screen: &Screen) -> Result<()> {
    let agent = live.agent;
    // Read before anything is looked at: a write by anyone else after this point means what was
    // looked at may no longer be true.
    let writes = live.input_writes();
    std::thread::sleep(SETTLE);

    let output = live.recent_output(WINDOW);
    if !screen.is_shown_in(&output) {
        return Err(screen_gone(agent));
    }
    // A Down from the second option wraps round to the first, where the Enter would then exit.
    if screen.cursor_on(&output) != Some(Cursor::Decline) {
        return Err(cursor_not_at_start(agent));
    }

    let mark = live.output_total();
    write(live, writes, DOWN)?;
    if !wait_for(MOVE_TIMEOUT, || {
        screen.cursor_on(&live.output_since(mark)) == Some(Cursor::Accept)
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
    if !wait_quiet(live, MOVE_TIMEOUT) {
        return Err(not_settled(agent));
    }
    if screen.cursor_on(&live.output_since(mark)) != Some(Cursor::Accept) {
        return Err(cursor_moved_away(agent));
    }
    enter_until_gone(live, screen, writes)
}

/// Codex's: an Enter on "1. Trust and continue", where the cursor starts, while nothing has been
/// written into its input since the screen was sighted (one step with the key).
pub(super) fn press_enter(live: &LiveSession, screen: &Screen) -> Result<()> {
    let agent = live.agent;
    let writes = live.trust.writes_at_sighting.load(Ordering::Acquire);
    std::thread::sleep(SETTLE);

    let output = live.recent_output(WINDOW);
    if !screen.is_shown_in(&output) {
        return Err(screen_gone(agent));
    }
    // An Enter on "2. Quit" ends the session.
    if screen.cursor_on(&output) != Some(Cursor::Accept) {
        return Err(cursor_not_at_start(agent));
    }
    if !wait_quiet(live, MOVE_TIMEOUT) {
        return Err(not_settled(agent));
    }
    if screen.cursor_on(&live.recent_output(WINDOW)) != Some(Cursor::Accept) {
        return Err(cursor_moved_away(agent));
    }
    enter_until_gone(live, screen, writes)
}

/// The Enter, and then the screen seen gone (`Screen::gone_in`), and still gone a moment later.
pub(super) fn enter_until_gone(live: &LiveSession, screen: &Screen, writes: u64) -> Result<()> {
    let mark = live.output_total();
    write(live, writes, ENTER)?;
    if !wait_for(DISMISS_TIMEOUT, || screen.gone_in(&live.output_since(mark))) {
        return Err(not_dismissed(live.agent));
    }
    std::thread::sleep(QUIET);
    if !screen.gone_in(&live.output_since(mark)) {
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
/// run its hook and the entry it recorded has reached the user's own store.
pub(super) fn press_yes(live: &LiveSession) -> Result<()> {
    let agent = live.agent;
    let trust = &live.trust;
    // Every Grok launch says where its trust record is carried (`crate::adapter::grok`).
    let Some(carried) = trust.carried.as_ref() else {
        return Err(anyhow!(
            "this Grok Build session has nowhere to carry its trust record to"
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
    if let Err(err) = write(live, writes, YES) {
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
        outcome = trust.carry_outcome(CARRY_LONGEST);
    }
    // Anything handed over in the meantime is still this press's; from here on, a hook reports
    // its own outcome.
    let outcome = outcome.or_else(|| trust.stop_awaiting_carry());
    match outcome {
        Some(Ok(())) => Ok(()),
        Some(Err(failure)) => Err(not_carried_over(agent, carried, &failure)),
        // A carry that outlasted even that reports itself; nothing more to say here.
        None if trust.hook_seen.load(Ordering::Acquire) => Err(not_waiting()),
        // No hook: the confirmation did not go. A hook that comes later reports its own outcome.
        None => Err(not_dismissed(agent)),
    }
}

pub(super) fn screen_gone(agent: Agent) -> anyhow::Error {
    answer_failed(
        agent,
        trust_reason::SCREEN_GONE,
        "the trust confirmation is no longer on the terminal",
        &[],
    )
}

pub(super) fn cursor_not_at_start(agent: Agent) -> anyhow::Error {
    answer_failed(
        agent,
        trust_reason::CURSOR_NOT_AT_START,
        "the cursor is not on the option it starts on, or could not be found",
        &[],
    )
}

pub(super) fn not_settled(agent: Agent) -> anyhow::Error {
    answer_failed(
        agent,
        trust_reason::TERMINAL_NOT_SETTLED,
        "the terminal did not settle, so Enter was not sent",
        &[],
    )
}

pub(super) fn cursor_moved_away(agent: Agent) -> anyhow::Error {
    answer_failed(
        agent,
        trust_reason::CURSOR_MOVED_AWAY,
        "the cursor is no longer on the option that trusts the folder, so Enter was not sent",
        &[],
    )
}

pub(super) fn not_dismissed(agent: Agent) -> anyhow::Error {
    answer_failed(
        agent,
        trust_reason::SCREEN_NOT_DISMISSED,
        "the confirmation did not go away after the key was sent",
        &[],
    )
}

/// Waits until the session has printed nothing for [`QUIET`], or `timeout` passes.
pub(super) fn wait_quiet(live: &LiveSession, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        let before = live.output_total();
        std::thread::sleep(QUIET);
        if live.output_total() == before {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
    }
}

/// Writes the keys unless anyone else has written into the session's input since `writes` was read;
/// the check and the write are one step.
pub(super) fn write(live: &LiveSession, writes: u64, keys: &[u8]) -> Result<()> {
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

/// Polls `done` until it holds or `timeout` passes.
pub(crate) fn wait_for(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(POLL);
    }
}
