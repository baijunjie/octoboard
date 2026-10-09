//! Each agent's trust screen as it reads off the terminal, and one session's watch for it
//! ([`TrustState`]): when it is sighted, whether it is still waiting, and whether messages for the
//! session have to be held meanwhile.

use super::*;

/// One agent's folder-trust confirmation as [`normalise`] leaves its text, and how it is pressed.
pub(super) struct Screen {
    /// The screen as first drawn: these pieces, in this order, anything between them. The first
    /// drawing is the only form the screen has at its start, so nothing looser is accepted.
    pub(super) first_draw: &'static [&'static str],
    /// The option that trusts the folder and the one that declines it: both for reading where a
    /// cursor is, and for telling whether the screen is still drawn after a key the person typed
    /// ([`Self::replaced_in`]); only the one that trusts after Octoboard's own key
    /// ([`Self::gone_in`]), since a later screen can say the same words as the one that declines.
    pub(super) accept: &'static str,
    pub(super) decline: &'static str,
    /// The glyph in front of the option the cursor is on, for a screen that has a cursor.
    pub(super) cursor: Option<char>,
    pub(super) press: Press,
    /// Text that the agent's next screen holds once this one is gone, where the agent draws one
    /// that can be told apart, for judging a key the person typed ([`Self::replaced_in`]): Codex
    /// answers a key it ignores at its trust screen with an empty frame, which shows nothing of
    /// the screen either, so only the screen that follows says it has gone.
    pub(super) next_screen: Option<&'static str>,
    /// Anything written into the session's input since the screen was sighted means it is no
    /// longer waiting for Octoboard: the person is answering it in the terminal. For an agent that
    /// runs no hook right after its screen is answered, this is the only way to tell.
    pub(super) input_ends_wait: bool,
    /// The agent runs a hook as it starts, once it is past this screen. Its messages are then held
    /// until that hook, whatever else happens: after Claude Code's trust screen another startup
    /// modal can follow (approving a project's `.mcp.json` servers, say), which a message's Enter
    /// would answer, and only the hook says the session is ready. An agent that runs none until
    /// its first prompt (Codex) has its messages released by Octoboard's press of its screen, or
    /// by the watch ending with none sighted.
    pub(super) hook_at_start: bool,
}

/// The keys that accept a screen, and what is checked around them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Press {
    /// The cursor starts on the decline option: a Down, then an Enter once the cursor is seen on
    /// the accept option.
    DownThenEnter,
    /// The cursor starts on the accept option: an Enter once it is seen still there.
    Enter,
    /// No cursor; one key accepts.
    Yes,
}

/// Claude Code 2.1.289: the cursor on "No, exit", then "Yes, I trust this folder".
pub(super) const CLAUDE: Screen = Screen {
    first_draw: &["\u{276F}No,exitYes,Itrustthisfolder"],
    accept: "Yes,Itrustthisfolder",
    decline: "No,exit",
    cursor: Some('\u{276F}'),
    press: Press::DownThenEnter,
    next_screen: None,
    // Claude Code's first hook follows an answer in the terminal at once, and its press looks at
    // the cursor again right before each key; a key the person pressed without answering, such as
    // a Down, is caught there.
    input_ends_wait: false,
    hook_at_start: true,
};

/// Codex 0.161.0: the question, then "› 1. Trust and continue" and "2. Quit". Codex redraws only
/// the cells that changed, so its text can lose characters wherever the frame before held the
/// same one; only phrases measured intact in its first drawing are matched. Its hook-review modal
/// ("Trust all and continue") holds neither piece.
pub(super) const CODEX: Screen = Screen {
    first_draw: &["Trustthisfolder?", "\u{203A}1.Trustandcontinue2.Quit"],
    accept: "1.Trustandcontinue",
    decline: "2.Quit",
    cursor: Some('\u{203A}'),
    press: Press::Enter,
    // Its banner, ">_ OpenAI Codex (v…)", drawn whole over the screen it replaces.
    next_screen: Some("OpenAICodex"),
    // Codex runs no hook until its first prompt, so an Enter pressed in the terminal would
    // otherwise leave the screen looking unanswered until then.
    input_ends_wait: true,
    hook_at_start: false,
};

/// Grok Build 1.0.50: the question, then "Yes, proceed y" and "No, quit n", with no cursor.
pub(super) const GROK: Screen = Screen {
    first_draw: &[
        "Doyoutrustthecontentsofthisdirectory?",
        "Yes,proceedyNo,quitn",
    ],
    accept: "Yes,proceedy",
    decline: "No,quitn",
    cursor: None,
    press: Press::Yes,
    next_screen: None,
    input_ends_wait: true,
    hook_at_start: true,
};

pub(super) fn screen_of(agent: Agent) -> &'static Screen {
    match agent {
        Agent::Claude => &CLAUDE,
        Agent::Codex => &CODEX,
        Agent::Grok => &GROK,
    }
}

/// How much recent output is searched for the screen. Each screen's first drawing is a few KiB at
/// most (Codex's, with the frame it is drawn over, about 4.3 KiB); the rest is room for it to
/// arrive in pieces and for what comes before it.
pub(super) const WINDOW: usize = 16 * 1024;

/// How long the watch for the screen lasts: it is among the first things each agent prints, so a
/// session that has printed this much or has been running this long and has not shown it, will
/// not.
pub(super) const WATCH_BYTES: usize = 64 * 1024;
pub(super) const WATCH_SECONDS: u64 = 30;

/// Which option holds the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Cursor {
    Decline,
    Accept,
}

/// Reduces terminal output to the text a person would read off the screen, with every space
/// removed: escape sequences, control characters and whitespace are all dropped. Dropping the
/// whitespace is the point — the agents position each word with a cursor-move sequence instead
/// of printing a space, so what is left of "Yes, I trust this folder" is `Yes,Itrustthisfolder`,
/// and that holds however the output was cut into reads or re-wrapped by a narrower window.
pub(super) fn normalise(raw: &[u8]) -> String {
    let mut kept = Vec::with_capacity(raw.len());
    let mut at = 0;
    while at < raw.len() {
        if raw[at] != 0x1b {
            kept.push(raw[at]);
            at += 1;
            continue;
        }
        at += 1;
        match raw.get(at) {
            // A control sequence: parameters and intermediates, then one final byte.
            Some(b'[') => {
                at += 1;
                while at < raw.len() && !(0x40..=0x7e).contains(&raw[at]) {
                    at += 1;
                }
                at += 1;
            }
            // An operating-system command (the hyperlink under "Security guide"): up to a bell or
            // a string terminator, the latter being an escape the next round takes care of.
            Some(b']') => {
                at += 1;
                while at < raw.len() && raw[at] != 0x07 && raw[at] != 0x1b {
                    at += 1;
                }
                if raw.get(at) == Some(&0x07) {
                    at += 1;
                }
            }
            // Anything else is an escape with at most intermediates and one final byte.
            Some(_) => {
                while at < raw.len() && (0x20..=0x2f).contains(&raw[at]) {
                    at += 1;
                }
                at += 1;
            }
            None => {}
        }
    }
    String::from_utf8_lossy(&kept)
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect()
}

impl Screen {
    /// Whether this output holds the screen as it is first drawn.
    pub(super) fn is_shown_in(&self, raw: &[u8]) -> bool {
        let text = normalise(raw);
        let mut rest = text.as_str();
        self.first_draw.iter().all(|piece| match rest.find(piece) {
            Some(at) => {
                rest = &rest[at + piece.len()..];
                true
            }
            None => false,
        })
    }

    /// Whether this output, printed after Octoboard's own accepting key, shows the screen gone: it
    /// holds text a person would read, and none of the screen's own — not the option that trusts,
    /// nor any piece of its first drawing, its question included. That key is known to act on the
    /// screen, so what the agent draws next does not have to be recognised. A frame of bare escape
    /// sequences, which Codex prints for every key it ignores, is not the screen going.
    pub(super) fn gone_in(&self, raw: &[u8]) -> bool {
        self.other_text_in(&normalise(raw))
    }

    /// Whether `text` is something to read with none of the screen's own in it.
    fn other_text_in(&self, text: &str) -> bool {
        !text.is_empty() && self.last_drawn(text).is_none()
    }

    /// Where in `text` the screen's own text was last drawn: the option that trusts the folder, or
    /// any piece of the first drawing. The decline option is not looked for on its own, since a
    /// later screen can hold the same words ("No, exit" in Claude Code's warnings), while every
    /// redraw of the screen that matters — the cursor moved, the whole screen again — holds the
    /// option that trusts.
    fn last_drawn(&self, text: &str) -> Option<usize> {
        std::iter::once(self.accept)
            .chain(self.first_draw.iter().copied())
            .filter_map(|piece| text.rfind(piece))
            .max()
    }

    /// Whether this output, printed since the screen was sighted and then typed at by the person,
    /// shows the screen replaced: where the agent's next screen can be told apart, its text drawn
    /// after the last drawing of the screen's own — either option included, so that any redraw
    /// of the options after it outranks it; otherwise, as [`Self::gone_in`]. Judged by order
    /// over everything printed since the sighting rather than after the latest key, so that a
    /// person typing on into the next screen does not hide that it came; and nothing says what
    /// their key was, so a frame that merely lacks the screen's text is not enough. A later screen
    /// that repeats the decline option's words only keeps the hold, which is the safe side.
    pub(super) fn replaced_in(&self, raw: &[u8]) -> bool {
        let text = normalise(raw);
        let Some(marker) = self.next_screen else {
            return self.other_text_in(&text);
        };
        let Some(next) = text.rfind(marker) else {
            return false;
        };
        self.last_drawn(&text)
            .max(text.rfind(self.decline))
            .is_none_or(|last| last < next)
    }

    /// Which option the cursor is on, read off the last cursor glyph in the output: after the
    /// first draw the agents redraw only what changed, so the latest glyph is the current one.
    /// `None` for a screen without a cursor, when there is no glyph, or what follows it is neither
    /// option — which includes a half-drawn line.
    pub(super) fn cursor_on(&self, raw: &[u8]) -> Option<Cursor> {
        let glyph = self.cursor?;
        let text = normalise(raw);
        let after = &text[text.rfind(glyph)? + glyph.len_utf8()..];
        if after.starts_with(self.decline) {
            Some(Cursor::Decline)
        } else if after.starts_with(self.accept) {
            Some(Cursor::Accept)
        } else {
            None
        }
    }
}

/// What the first hook on a session left to do ([`TrustState::note_hook`]).
pub(super) struct HookNoted {
    /// Messages were held for the session, and the hook lets them through.
    pub(super) released: bool,
    /// The hook ends a sighted screen Octoboard has not pressed successfully — the person answered
    /// it in the terminal, or Octoboard's press is still in flight or has failed — so the trust
    /// the agent recorded is the hook's to carry over, where the agent's record has to be carried.
    pub(super) carry: bool,
}

/// The hand-over of a carry's outcome between the hook that carries and Octoboard's press, which
/// waits for it to know whether the press succeeded.
#[derive(Default)]
pub(super) struct CarryHandover {
    /// A press has sent its key and is waiting for the outcome.
    pub(super) awaited: bool,
    pub(super) outcome: Option<std::result::Result<(), CarryFailure>>,
}

#[derive(Default)]
pub(super) struct Watched {
    pub(super) window: Vec<u8>,
    pub(super) total: usize,
}

/// One session's watch for its trust screen, owned by its [`LiveSession`].
pub struct TrustState {
    /// The screen this session's agent shows, and how it is pressed.
    pub(super) screen: &'static Screen,
    /// Grok's: where its trust entry is carried from and to after a press.
    pub(super) carried: Option<CarriedTrust>,
    /// The output not yet identified as anything, at most [`WINDOW`] bytes of it, and what the
    /// watch has seen in all. Released when the watch ends.
    pub(super) window: Mutex<Watched>,
    /// When the watch began, and how much output and how long it lasts.
    pub(super) started: Instant,
    pub(super) watch_bytes: usize,
    pub(super) watch_age: Mutex<Duration>,
    /// The watch has ended without a sighting: too much output or too long a time.
    pub(super) expired: AtomicBool,
    /// The screen has been sighted, once, and `sightings` told.
    pub(super) sighted: AtomicBool,
    /// How many foreign writes the session's input had had when the screen was sighted, and how
    /// much the session had printed by then.
    pub(super) writes_at_sighting: AtomicU64,
    pub(super) output_at_sighting: AtomicU64,
    /// A hook callback has arrived, so the screen is past and nothing on the terminal is it.
    pub(super) hook_seen: AtomicBool,
    /// An answer has been started. A session is answered at most once.
    pub(super) answered: AtomicBool,
    /// Octoboard's press of the screen succeeded.
    pub(super) pressed: AtomicBool,
    /// The person answered the screen in the terminal and it has been seen to go
    /// ([`super::release_when_the_screen_goes`]); for an agent that runs no hook after it.
    pub(super) dismissed: AtomicBool,
    /// Where a hook leaves the outcome of carrying the agent's trust record for a press waiting on
    /// it.
    pub(super) carry: Mutex<CarryHandover>,
    pub(super) carried_over: std::sync::Condvar,
    pub(super) sightings: UnboundedSender<()>,
    pub(super) sighting_receiver: Mutex<Option<UnboundedReceiver<()>>>,
}

impl TrustState {
    pub fn new(agent: Agent, carried: Option<CarriedTrust>) -> Self {
        Self::with_limits(
            agent,
            carried,
            WATCH_BYTES,
            Duration::from_secs(WATCH_SECONDS),
        )
    }

    pub(super) fn with_limits(
        agent: Agent,
        carried: Option<CarriedTrust>,
        bytes: usize,
        age: Duration,
    ) -> Self {
        let (sightings, receiver) = unbounded_channel();
        Self {
            screen: screen_of(agent),
            carried,
            window: Mutex::new(Watched::default()),
            started: Instant::now(),
            watch_bytes: bytes,
            watch_age: Mutex::new(age),
            expired: AtomicBool::new(false),
            sighted: AtomicBool::new(false),
            writes_at_sighting: AtomicU64::new(0),
            output_at_sighting: AtomicU64::new(0),
            hook_seen: AtomicBool::new(false),
            answered: AtomicBool::new(false),
            pressed: AtomicBool::new(false),
            dismissed: AtomicBool::new(false),
            carry: Mutex::new(CarryHandover::default()),
            carried_over: std::sync::Condvar::new(),
            sightings,
            sighting_receiver: Mutex::new(Some(receiver)),
        }
    }

    /// Looks at one more chunk of the session's output; called from the PTY reader thread for
    /// every chunk, so it must stay quick and must not block. `input_writes` is the session's count
    /// of foreign writes as it stands, and `output_total` how much it has printed, this chunk
    /// included. Signals a sighting the first time the screen is complete in what has been seen.
    pub fn feed(&self, chunk: &[u8], input_writes: u64, output_total: u64) {
        if self.hook_seen.load(Ordering::Acquire)
            || self.sighted.load(Ordering::Acquire)
            || self.expired.load(Ordering::Acquire)
        {
            return;
        }
        let mut watched = self.window.lock().expect("trust window mutex poisoned");
        watched.total += chunk.len();
        if watched.total > self.watch_bytes || self.started.elapsed() > self.watch_age() {
            self.expired.store(true, Ordering::Release);
            *watched = Watched::default();
            return;
        }
        watched.window.extend_from_slice(chunk);
        if watched.window.len() > WINDOW {
            let excess = watched.window.len() - WINDOW;
            watched.window.drain(..excess);
        }
        if self.screen.is_shown_in(&watched.window) {
            *watched = Watched::default();
            self.writes_at_sighting
                .store(input_writes, Ordering::Release);
            self.output_at_sighting
                .store(output_total, Ordering::Release);
            if !self.sighted.swap(true, Ordering::AcqRel) {
                // The receiver is gone only once the session is, and nothing is left to tell.
                let _ = self.sightings.send(());
            }
        }
    }

    fn watch_age(&self) -> Duration {
        *self
            .watch_age
            .lock()
            .expect("trust watch age mutex poisoned")
    }

    /// A hook of this session reached the daemon: the screen has been answered, so it is no longer
    /// looked for. Says what the first such hook leaves to do; a later one leaves nothing.
    pub(super) fn note_hook(&self) -> HookNoted {
        let released = self.holds_writes();
        let first = !self.hook_seen.swap(true, Ordering::AcqRel);
        *self.window.lock().expect("trust window mutex poisoned") = Watched::default();
        HookNoted {
            released,
            carry: first
                && self.carried.is_some()
                && self.sighted.load(Ordering::Acquire)
                && !self.pressed.load(Ordering::Acquire),
        }
    }

    /// Whether a message written into the session now could land on its trust screen, and so has
    /// to be held. A paste's trailing carriage return accepts Codex's screen and Grok's, and exits
    /// Claude Code's. For an agent that runs a hook at start (`Screen::hook_at_start`) messages are
    /// held until that hook, so a session whose hooks never arrive holds them for as long as it
    /// runs. For one that runs none (Codex) they are held while the watch is still looking for the
    /// screen — it ends on its own after [`WATCH_SECONDS`], with no output arriving to notice it —
    /// and, once the screen is up, until Octoboard's press of it has succeeded, the person's own
    /// answer in the terminal has been seen to take it away, or a hook arrives.
    pub fn holds_writes(&self) -> bool {
        if self.hook_seen.load(Ordering::Acquire) {
            return false;
        }
        if self.screen.hook_at_start {
            return true;
        }
        if self.pressed.load(Ordering::Acquire) || self.dismissed.load(Ordering::Acquire) {
            return false;
        }
        // Under the window's lock, which a sighting is made under too, so that the watch cannot
        // be found over in the same instant a screen is sighted at its very end.
        let _watched = self.window.lock().expect("trust window mutex poisoned");
        if self.sighted.load(Ordering::Acquire) {
            return true;
        }
        !self.expired.load(Ordering::Acquire) && self.started.elapsed() <= self.watch_age()
    }

    /// Ends the watch as if the session had run its first hook, for a test's stand-in that stands
    /// for a session past its screen.
    #[cfg(test)]
    pub(crate) fn end_watch(&self) {
        self.note_hook();
    }

    /// Shortens the watch, for a test that cannot wait out [`WATCH_SECONDS`].
    #[cfg(test)]
    pub(crate) fn shorten_watch(&self, age: Duration) {
        *self
            .watch_age
            .lock()
            .expect("trust watch age mutex poisoned") = age;
    }

    /// For a press that has sent its key: says that it waits for the hook's carry. Taken back by
    /// [`Self::stop_awaiting_carry`].
    pub(super) fn await_carry(&self) {
        let mut handover = self.carry.lock().expect("trust carry mutex poisoned");
        handover.awaited = true;
        handover.outcome = None;
    }

    /// Waits up to `timeout` for the hook's carry outcome; `None` when none came. The press still
    /// awaits it until [`Self::stop_awaiting_carry`].
    pub(super) fn carry_outcome(
        &self,
        timeout: Duration,
    ) -> Option<std::result::Result<(), CarryFailure>> {
        let handover = self.carry.lock().expect("trust carry mutex poisoned");
        let (mut handover, _) = self
            .carried_over
            .wait_timeout_while(handover, timeout, |handover| handover.outcome.is_none())
            .expect("trust carry mutex poisoned");
        handover.outcome.take()
    }

    /// For a press that stops waiting without its key having led anywhere: whatever outcome a
    /// hook left meanwhile is the caller's, and a later one is the hook's own to report.
    pub(super) fn stop_awaiting_carry(&self) -> Option<std::result::Result<(), CarryFailure>> {
        let mut handover = self.carry.lock().expect("trust carry mutex poisoned");
        handover.awaited = false;
        handover.outcome.take()
    }

    /// For the hook that carried: hands `outcome` to a press waiting for it, or back to the hook,
    /// to report itself, when none is.
    pub(super) fn hand_over_carry(
        &self,
        outcome: std::result::Result<(), CarryFailure>,
    ) -> Option<std::result::Result<(), CarryFailure>> {
        let mut handover = self.carry.lock().expect("trust carry mutex poisoned");
        if handover.awaited {
            handover.outcome = Some(outcome);
            self.carried_over.notify_all();
            None
        } else {
            Some(outcome)
        }
    }

    /// Whether the session's input has been written to since the screen was sighted;
    /// `input_writes` is the session's count as it stands now.
    pub(super) fn input_since_sighting(&self, input_writes: u64) -> bool {
        input_writes != self.writes_at_sighting.load(Ordering::Acquire)
    }

    /// Whether the screen is up and nobody has started answering it.
    pub(super) fn waiting(&self) -> bool {
        self.sighted.load(Ordering::Acquire)
            && !self.hook_seen.load(Ordering::Acquire)
            && !self.answered.load(Ordering::Acquire)
    }

    /// [`Self::waiting`], and for a screen that input ends, nothing written into the session's
    /// input since it was sighted; `input_writes` is the session's count as it stands now.
    pub(super) fn waiting_with_input(&self, input_writes: u64) -> bool {
        self.waiting() && !(self.screen.input_ends_wait && self.input_since_sighting(input_writes))
    }

    /// Takes the right to answer, which only one caller ever gets.
    pub(super) fn claim_answer(&self) -> bool {
        self.sighted.load(Ordering::Acquire)
            && !self.hook_seen.load(Ordering::Acquire)
            && !self.answered.swap(true, Ordering::AcqRel)
    }

    /// The sightings of this session, for whoever supervises it. There is only one such receiver.
    pub(super) fn take_sightings(&self) -> Option<UnboundedReceiver<()>> {
        self.sighting_receiver
            .lock()
            .expect("trust receiver mutex poisoned")
            .take()
    }
}
