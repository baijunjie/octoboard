//! How a trust screen reads off the terminal, each provider's being its screen reader's, and one
//! session's watch for it ([`TrustState`]): when it is sighted, whether it is still waiting, and
//! whether messages for the session have to be held meanwhile.

use super::*;

use crate::platform::term;

/// One agent's folder-trust confirmation as [`normalise`] leaves its text, and how it is pressed.
pub(crate) struct Screen {
    /// The known first drawings: any one sequence of pieces, in order, anything between them.
    /// Only complete first drawings are accepted, including across CLI versions. Empty for a
    /// screen that has not been measured, which is then never looked for.
    pub(crate) first_draw: &'static [&'static [&'static str]],
    /// The option that trusts the folder and the options that decline it: both for reading where a
    /// cursor is, and for telling whether the screen is still drawn after a key the person typed
    /// ([`Self::replaced_in`]); only the one that trusts after Octoboard's own key
    /// ([`Self::gone_in`]), since a later screen can say the same words as the one that declines.
    /// `None` for a screen that has not been measured.
    pub(crate) accept: Option<&'static str>,
    pub(crate) decline: &'static [&'static str],
    /// The glyph in front of the option the cursor is on, for a screen that has a cursor.
    pub(crate) cursor: Option<char>,
    pub(crate) press: Press,
    /// Text that the agent's next screen holds once this one is gone, where the agent draws one
    /// that can be told apart, for judging a key the person typed ([`Self::replaced_in`]): Codex
    /// answers a key it ignores at its trust screen with an empty frame, which shows nothing of
    /// the screen either, so only the screen that follows says it has gone.
    pub(crate) next_screen: Option<&'static str>,
    /// Anything written into the session's input since the screen was sighted means it is no
    /// longer waiting for Octoboard: the person is answering it in the terminal. For an agent that
    /// runs no hook right after its screen is answered, this is the only way to tell.
    pub(crate) input_ends_wait: bool,
    /// What the agent does first once it is past its startup, which ends the watch for the screen:
    /// everything before it, however long, may still be the screen's to come.
    pub(crate) start_signal: StartSignal,
    /// For a [`StartSignal::Hook`] agent whose hook cannot be relied on to come at all: how long
    /// after the launch its messages stay held for a hook before they are let through anyway,
    /// provided no screen has been sighted ([`super::release_when_no_hook_comes`]); a screen that
    /// has been is held until it is seen to go ([`super::release_when_the_sighted_screen_goes`]).
    /// `None` holds them for as long as the session runs.
    pub(crate) hook_fallback: Option<Duration>,
    /// Whether the hook alone is not enough to say the agent reads its input: it must also have
    /// turned bracketed paste on (`ESC [ ? 2004 h`), which it keeps on from the moment its real
    /// input handling starts; the brief toggle of its terminal probe at start, on and straight
    /// off again, does not count. Claude Code's fullscreen renderer runs `SessionStart` before
    /// that, while an early-input capture still reads the terminal itself, drops the paste markers
    /// and turns the Enter into a newline, so a message written in between is never sent.
    pub(crate) waits_for_paste_mode: bool,
}

/// How an agent shows that its startup, the trust screen included, is behind it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartSignal {
    /// It runs a hook as it starts, once it is past the screen. Its messages are held at least until
    /// that hook, whatever else happens: after Claude Code's trust screen another startup modal
    /// can follow (approving a project's `.mcp.json` servers, say), which a message's Enter would
    /// answer, and only the hook says the session is ready.
    Hook,
    /// It runs no hook until its first prompt (Codex) but writes the terminal title as its
    /// session starts, after the screen and after the login screen that can come before it, and
    /// never before either. Its messages are released by Octoboard's press of its screen, or by
    /// the watch ending with none sighted. The launch fixes the title's items
    /// ([`crate::adapter::codex`]), so no configuration switches the title off.
    TerminalTitle,
}

/// The keys that accept a screen, and what is checked around them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Press {
    /// The cursor starts on the decline option: a Down, then an Enter once the cursor is seen on
    /// the accept option.
    DownThenEnter,
    /// The cursor starts on the accept option: an Enter once it is seen still there.
    Enter,
    /// No cursor; one key accepts.
    Yes,
}

/// The screen of a provider whose trust confirmation has not been measured: never looked for, and
/// its messages held for a hook up to [`HOOK_FALLBACK`] after the launch — the whole of it for a
/// provider without hooks, whose launch-time messages go in only then.
pub(crate) const NO_SCREEN: Screen = Screen {
    first_draw: &[],
    accept: None,
    decline: &[],
    cursor: None,
    press: Press::Yes,
    next_screen: None,
    input_ends_wait: true,
    start_signal: StartSignal::Hook,
    hook_fallback: Some(HOOK_FALLBACK),
    waits_for_paste_mode: false,
};

/// The screen `agent`'s provider shows ([`crate::session::screen::ScreenReader::trust_screen`]).
pub(super) fn screen_of(agent: AgentId) -> &'static Screen {
    crate::adapter::provider(agent).screen().trust_screen()
}

/// How much recent output is searched for the screen. Each screen's first drawing is a few KiB at
/// most (Codex's, with the frame it is drawn over, about 4.3 KiB); the rest is room for it to
/// arrive in pieces and for what comes before it.
pub(super) const WINDOW: usize = 16 * 1024;

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
    /// Whether the agent's hook is what ends the watch and releases its messages, and nothing else
    /// does ([`StartSignal::Hook`]).
    pub(super) fn hook_at_start(&self) -> bool {
        self.start_signal == StartSignal::Hook
    }

    /// Whether the screen can be sighted at all: one that has not been measured has no first
    /// drawing to look for, and nothing of the terminal's output is kept or read for it.
    pub(super) fn can_be_sighted(&self) -> bool {
        !self.first_draw.is_empty()
    }

    /// Whether this output holds the screen as it is first drawn.
    pub(super) fn is_shown_in(&self, raw: &[u8]) -> bool {
        first_drawn_in(&normalise(raw), self.first_draw)
    }

    /// Whether this parsed screen shows the screen, wherever its cursor is: its first drawing with
    /// the cursor glyph left out of both. The parsed screen keeps only what is up now, so a screen
    /// whose cursor has moved since it was first drawn no longer holds that drawing, and is still
    /// up; the output it was first drawn in would still hold it.
    pub(super) fn is_up_in(&self, screen: &[u8]) -> bool {
        let Some(glyph) = self.cursor else {
            return self.is_shown_in(screen);
        };
        let without = |text: &str| text.chars().filter(|c| *c != glyph).collect::<String>();
        let text = without(&normalise(screen));
        self.first_draw.iter().any(|drawing| {
            let pieces: Vec<String> = drawing.iter().map(|piece| without(piece)).collect();
            in_order(&text, pieces.iter().map(String::as_str))
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

    /// [`Self::gone_in`] for the parsed screen, which shows what is up now rather than what was
    /// printed after the key: it has changed since `before`, what it showed as the key was sent,
    /// and shows the screen gone. A frame of bare escape sequences changes nothing on it.
    pub(super) fn gone_from(&self, before: &[u8], now: &[u8]) -> bool {
        now != before && self.gone_in(now)
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
        self.accept
            .into_iter()
            .chain(
                self.first_draw
                    .iter()
                    .flat_map(|drawing| drawing.iter().copied()),
            )
            .filter_map(|piece| text.rfind(piece))
            .max()
    }

    /// [`Self::replaced_in`] for the parsed screen, which shows what is up now rather than what
    /// was drawn after what: where the agent's next screen can be told apart, its text is there
    /// and none of this screen's own, either option included; otherwise, as [`Self::gone_in`]. The
    /// screen was up when it was sighted, so a parsed screen without any of its text has changed
    /// since.
    pub(super) fn replaced_on(&self, screen: &[u8]) -> bool {
        let text = normalise(screen);
        let Some(marker) = self.next_screen else {
            return self.other_text_in(&text);
        };
        text.contains(marker)
            && self.last_drawn(&text).is_none()
            && !self.decline.iter().any(|piece| text.contains(piece))
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
            .max(
                self.decline
                    .iter()
                    .filter_map(|piece| text.rfind(piece))
                    .max(),
            )
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
        if self.decline.iter().any(|piece| after.starts_with(piece)) {
            Some(Cursor::Decline)
        } else if self.accept.is_some_and(|accept| after.starts_with(accept)) {
            Some(Cursor::Accept)
        } else {
            None
        }
    }

    /// Which option the cursor is on, read off the cells drawn in inverse video (`inverse`, the
    /// parsed screen with every other cell blank), as Codex draws the option its cursor is on:
    /// the one option whose text is there. `None` for a screen without a cursor, or when neither
    /// option or both are.
    pub(super) fn cursor_in_inverse(&self, inverse: &[u8]) -> Option<Cursor> {
        self.cursor?;
        let text = normalise(inverse);
        let decline = self.decline.iter().any(|piece| text.contains(piece));
        let accept = self.accept.is_some_and(|accept| text.contains(accept));
        match (decline, accept) {
            (true, false) => Some(Cursor::Decline),
            (false, true) => Some(Cursor::Accept),
            _ => None,
        }
    }
}

/// Whether `text` holds any one of the `drawings`, each a sequence of pieces in order with anything
/// between them.
fn first_drawn_in(text: &str, drawings: &[&[&str]]) -> bool {
    drawings
        .iter()
        .any(|drawing| in_order(text, drawing.iter().copied()))
}

/// Whether `text` holds the `pieces` in order, anything between them.
fn in_order<'a>(text: &str, pieces: impl IntoIterator<Item = &'a str>) -> bool {
    let mut rest = text;
    pieces.into_iter().all(|piece| match rest.find(piece) {
        Some(at) => {
            rest = &rest[at + piece.len()..];
            true
        }
        None => false,
    })
}

/// What the first hook on a session left to do ([`TrustState::note_hook`]).
pub(super) struct HookNoted {
    /// Messages were held for the session, and the hook lets them through.
    pub(super) released: bool,
    /// The hook ends a sighted screen whose trust record nothing has carried yet — the person
    /// answered it in the terminal, Octoboard's press is still in flight or has failed, or the
    /// press counted only because the rendered screen was seen to go — so the trust the agent
    /// recorded is the hook's to carry over, where the agent's record has to be carried.
    pub(super) carry: bool,
}

/// The rendezvous of a carry's outcome between the hook that carries and Octoboard's press, which
/// waits for it to know whether the press succeeded.
#[derive(Default)]
pub(super) struct CarryRendezvous {
    /// A press has sent its key and is waiting for the outcome.
    pub(super) awaited: bool,
    pub(super) outcome: Option<std::result::Result<(), CarryFailure>>,
}

/// One session's watch for its trust screen, owned by its [`LiveSession`].
pub struct TrustState {
    /// The session's agent, whose input box says where the parsed screen is read that its
    /// startup is behind it ([`Self::feed_screen`]).
    agent: AgentId,
    /// The screen this session's agent shows, and how it is pressed.
    pub(super) screen: &'static Screen,
    /// Where the trust entry is carried from and to after a press, for a provider that records
    /// the trust in a store of the session's own (`Capabilities::per_session_trust`, Grok Build).
    pub(super) carried: Option<Arc<dyn TrustCarrier>>,
    /// Whether the screen is read off the session's parsed screen rather than its output
    /// ([`Self::reads_screen`]).
    reads_screen: AtomicBool,
    /// The output not yet identified as anything, at most [`WINDOW`] bytes of it, where the output
    /// is read; empty where the parsed screen is. Released when the watch ends.
    pub(super) window: Mutex<Vec<u8>>,
    /// The watch has ended without a sighting: the agent's terminal title came, or, where the
    /// parsed screen is read, its input box. Never set for an agent that runs a hook at start.
    pub(super) expired: AtomicBool,
    /// The agent's hook fallback ([`Screen::hook_fallback`]) has let its messages through with no
    /// hook seen. Outranked by a sighting of the screen after it.
    pub(super) released_without_hook: AtomicBool,
    /// The screen has been sighted, once, and `sightings` told.
    pub(super) sighted: AtomicBool,
    /// The screen needs a person rather than an automatic press.
    pub(super) needs_user: AtomicBool,
    /// How many foreign writes the session's input had had when the screen was sighted, and how
    /// much the session had printed by then.
    pub(super) writes_at_sighting: AtomicU64,
    pub(super) output_at_sighting: AtomicU64,
    /// A hook callback has arrived, so the screen is past and nothing on the terminal is it.
    pub(super) hook_seen: AtomicBool,
    /// The session's terminal has bracketed paste on as of its last output, or, once the hook has
    /// come, has had it on since; see [`Screen::waits_for_paste_mode`].
    pub(super) paste_mode_seen: AtomicBool,
    /// Taken around the hook's arrival and around undoing a sighting of bracketed paste, so that
    /// none is undone once the hook has come.
    paste_mode_order: Mutex<()>,
    /// The agent's `SessionStart` hook has come ([`Self::note_session_start`]).
    session_started: AtomicBool,
    /// Where the parsed screen is read, the agent's input box has been seen on it since its
    /// `SessionStart` hook, which stands for bracketed paste turned on
    /// ([`Self::note_input_box`]).
    input_box_seen: AtomicBool,
    /// An answer has been started. A session is answered at most once.
    pub(super) answered: AtomicBool,
    /// Octoboard's press of the screen succeeded.
    pub(super) pressed: AtomicBool,
    /// The screen has been seen to go with no hook to say so: the person answered it in the
    /// terminal ([`super::release_when_the_screen_goes`], Codex), or the agent's hook cannot be
    /// relied on and the rendered screen no longer shows it
    /// ([`super::release_when_the_sighted_screen_goes`], Grok Build).
    pub(super) dismissed: AtomicBool,
    /// Where a hook leaves the outcome of carrying the agent's trust record for a press waiting on
    /// it.
    pub(super) carry: Mutex<CarryRendezvous>,
    pub(super) carried_over: std::sync::Condvar,
    pub(super) sightings: UnboundedSender<()>,
    pub(super) sighting_receiver: Mutex<Option<UnboundedReceiver<()>>>,
}

impl TrustState {
    /// The watch for `agent`'s screen; `carried` is what its launch says about carrying the trust
    /// entry, kept only where the provider's trust is per session.
    pub fn new(agent: AgentId, carried: Option<Arc<dyn TrustCarrier>>) -> Self {
        let (sightings, receiver) = unbounded_channel();
        Self {
            agent,
            screen: screen_of(agent),
            carried: carried.filter(|_| agent.descriptor().capabilities.per_session_trust),
            reads_screen: AtomicBool::new(term::facts().host_renders),
            window: Mutex::new(Vec::new()),
            expired: AtomicBool::new(false),
            released_without_hook: AtomicBool::new(false),
            sighted: AtomicBool::new(false),
            needs_user: AtomicBool::new(false),
            writes_at_sighting: AtomicU64::new(0),
            output_at_sighting: AtomicU64::new(0),
            hook_seen: AtomicBool::new(false),
            paste_mode_seen: AtomicBool::new(false),
            paste_mode_order: Mutex::new(()),
            session_started: AtomicBool::new(false),
            input_box_seen: AtomicBool::new(false),
            answered: AtomicBool::new(false),
            pressed: AtomicBool::new(false),
            dismissed: AtomicBool::new(false),
            carry: Mutex::new(CarryRendezvous::default()),
            carried_over: std::sync::Condvar::new(),
            sightings,
            sighting_receiver: Mutex::new(Some(receiver)),
        }
    }

    /// Whether the screen is read off the session's parsed screen rather than its output: where
    /// the host renders what the program draws (`TermFacts::host_renders`), the output is the
    /// host's repaint, in an order of its own, and only the parsed screen says what is drawn.
    /// Elsewhere the output is read, since its order also says what was drawn after what, which
    /// the parsed screen, holding only what is up now, cannot: a first drawing and the redraw after
    /// a Down that arrive in one read leave only the redraw on it, and a redraw of the options
    /// followed by the next screen leaves only the next screen.
    pub(crate) fn reads_screen(&self) -> bool {
        self.reads_screen.load(Ordering::Relaxed)
    }

    /// Reads the screen off the parsed screen from now on, as a host that renders what the program
    /// draws has it read, for a test of that reading on a host that does not.
    #[cfg(test)]
    pub(crate) fn read_the_screen(&self) {
        self.reads_screen.store(true, Ordering::Relaxed);
    }

    /// Reads the screen off the output from now on, as a host that passes the program's output
    /// through has it read, for a test of that reading on a host that renders it.
    #[cfg(test)]
    pub(crate) fn read_the_output(&self) {
        self.reads_screen.store(false, Ordering::Relaxed);
    }

    /// Looks at the session's terminal after one more chunk of its output: the chunk, or the parsed
    /// screen it left, which `screen` gives and is asked for only while the screen is looked for
    /// ([`Self::reads_screen`]). Called from the PTY reader thread for every chunk, so it must stay
    /// quick and must not block; `input_writes` and `output_total` are as for [`Self::feed`].
    pub fn on_output(
        &self,
        chunk: &[u8],
        screen: impl FnOnce() -> String,
        input_writes: u64,
        output_total: u64,
    ) {
        if self.reads_screen() {
            self.feed_screen(screen, input_writes, output_total);
        } else {
            self.feed(chunk, input_writes, output_total);
        }
    }

    /// Whether the screen is still looked for.
    fn looking(&self) -> bool {
        self.screen.can_be_sighted()
            && !self.hook_seen.load(Ordering::Acquire)
            && !self.sighted.load(Ordering::Acquire)
            && !self.expired.load(Ordering::Acquire)
    }

    /// Looks at one more chunk of the session's output; called from the PTY reader thread for
    /// every chunk, so it must stay quick and must not block. `input_writes` is the session's count
    /// of foreign writes as it stands, and `output_total` how much it has printed, this chunk
    /// included. Signals a sighting the first time the screen is complete in what has been seen.
    pub fn feed(&self, chunk: &[u8], input_writes: u64, output_total: u64) {
        if !self.looking() {
            return;
        }
        let mut watched = self.window.lock().expect("trust window mutex poisoned");
        watched.extend_from_slice(chunk);
        if watched.len() > WINDOW {
            let excess = watched.len() - WINDOW;
            watched.drain(..excess);
        }
        if self.screen.is_shown_in(&watched) {
            *watched = Vec::new();
            self.note_sighting(input_writes, output_total);
        }
    }

    /// [`Self::feed`] for the parsed screen as the latest chunk left it, which `screen` gives: a
    /// sighting the first time it shows the screen as first drawn. For an agent whose start signal
    /// is its terminal title ([`StartSignal::TerminalTitle`]) the watch ends, with no screen
    /// sighted, once the parsed screen shows the agent's input box instead: its session has
    /// started, and that is read off what is drawn rather than off a title, which the host can set
    /// and send at a moment of its own ([`Self::note_title`]).
    pub(crate) fn feed_screen(
        &self,
        screen: impl FnOnce() -> String,
        input_writes: u64,
        output_total: u64,
    ) {
        if !self.looking() {
            return;
        }
        // Read before the window's lock is taken, as the parsed screen has a lock of its own.
        let text = screen();
        let _watched = self.window.lock().expect("trust window mutex poisoned");
        if self.screen.is_shown_in(text.as_bytes()) {
            self.note_sighting(input_writes, output_total);
        } else if self.screen.start_signal == StartSignal::TerminalTitle
            && crate::session::input_box::input_box_rows(self.agent, &text).is_some()
        {
            self.expired.store(true, Ordering::Release);
        }
    }

    /// The screen has been sighted, under the window's lock; signalled once.
    fn note_sighting(&self, input_writes: u64, output_total: u64) {
        self.writes_at_sighting
            .store(input_writes, Ordering::Release);
        self.output_at_sighting
            .store(output_total, Ordering::Release);
        if !self.sighted.swap(true, Ordering::AcqRel) {
            // The receiver is gone only once the session is, and nothing is left to tell.
            let _ = self.sightings.send(());
        }
    }

    /// The session's output has completed a terminal title, which [`normalise`] strips from what
    /// [`Self::feed`] sees, so the session reads it off the raw output. For an agent whose start
    /// signal is the title ([`StartSignal::TerminalTitle`]) that ends the watch if no screen has
    /// been sighted: its startup is behind it and nothing comes that is the screen. Called after
    /// [`Self::feed`] has seen the same chunk, so a chunk that holds both sights the screen. Where
    /// the parsed screen is read the title ends nothing: the watch ends at the input box drawn
    /// instead ([`Self::feed_screen`]).
    pub fn note_title(&self) {
        if self.screen.start_signal != StartSignal::TerminalTitle || self.reads_screen() {
            return;
        }
        let mut watched = self.window.lock().expect("trust window mutex poisoned");
        if self.hook_seen.load(Ordering::Acquire) || self.sighted.load(Ordering::Acquire) {
            return;
        }
        self.expired.store(true, Ordering::Release);
        *watched = Vec::new();
    }

    /// A hook of this session reached the daemon: the screen has been answered, so it is no longer
    /// looked for. Says what the first such hook leaves to do; a later one leaves nothing.
    pub(super) fn note_hook(&self) -> HookNoted {
        let was_holding = self.holds_writes();
        let first = {
            let _order = self
                .paste_mode_order
                .lock()
                .expect("trust paste mutex poisoned");
            !self.hook_seen.swap(true, Ordering::AcqRel)
        };
        // Still held when the agent has yet to turn bracketed paste on: that moment releases them.
        let released = was_holding && !self.holds_writes();
        *self.window.lock().expect("trust window mutex poisoned") = Vec::new();
        HookNoted {
            released,
            // A press the rendered screen alone confirmed has had nothing carried, so a hook that
            // comes after it still carries the entry.
            carry: first
                && self.carried.is_some()
                && self.sighted.load(Ordering::Acquire)
                && (!self.pressed.load(Ordering::Acquire)
                    || self.dismissed.load(Ordering::Acquire)),
        }
    }

    /// Whether a message written into the session now could land on its trust screen, and so has to
    /// be held. A paste's trailing carriage return accepts Codex's screen and Grok's, and exits
    /// Claude Code's. For an agent that runs a hook at start (`Screen::hook_at_start`) messages are
    /// held until that hook, so a session whose hooks never arrive holds them for as long as it
    /// runs, unless the agent has a [`Screen::hook_fallback`] and it has let them through. For one
    /// that runs none (Codex) they are held while the watch is still looking for the screen — it
    /// ends with the agent's terminal title — and, once the screen is up, until Octoboard's press
    /// of it has succeeded, the person's own answer in the terminal has been seen to take it away,
    /// or a hook arrives.
    ///
    /// For an agent that turns bracketed paste on only once it reads its input
    /// ([`Screen::waits_for_paste_mode`], Claude Code) the hook is not the end of the hold: it
    /// lasts until the terminal has also been seen with bracketed paste on at the hook or after,
    /// or, where the parsed screen is read, the agent's input box on it after its `SessionStart`
    /// hook, with no time limit, like a hook that never comes.
    pub fn holds_writes(&self) -> bool {
        if self.hook_seen.load(Ordering::Acquire) {
            return self.awaits_paste_mode();
        }
        if self.screen.hook_at_start() {
            // For an agent whose hook may never come, a sighted screen seen to go is as good as
            // the hook. Claude Code has no such fallback: its hold cannot end before its hook.
            if self.screen.hook_fallback.is_some() && self.dismissed.load(Ordering::Acquire) {
                return false;
            }
            // Let through without a hook only while no screen has come up since.
            return !self.released_without_hook.load(Ordering::Acquire)
                || self.sighted.load(Ordering::Acquire);
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
        !self.expired.load(Ordering::Acquire)
    }

    /// Whether the agent has yet to turn bracketed paste on, and that is still what its messages
    /// wait for ([`Screen::waits_for_paste_mode`]). Where the parsed screen is read, its input box
    /// seen since its `SessionStart` hook does as well ([`Self::note_input_box`]).
    fn awaits_paste_mode(&self) -> bool {
        self.screen.waits_for_paste_mode && !self.reads_its_input()
    }

    /// Whether the agent has been seen to read its input: bracketed paste on, or, where the parsed
    /// screen is read, its input box shown since its `SessionStart` hook.
    fn reads_its_input(&self) -> bool {
        self.paste_mode_seen.load(Ordering::Acquire) || self.input_box_seen.load(Ordering::Acquire)
    }

    /// Whether the session's bracketed paste is still to be followed ([`Self::note_paste_mode`]):
    /// for an agent that waits for it, until the hook has come and the agent has been seen to read
    /// its input.
    pub fn follows_paste_mode(&self) -> bool {
        self.screen.waits_for_paste_mode
            && !(self.hook_seen.load(Ordering::Acquire) && self.reads_its_input())
    }

    /// The agent's `SessionStart` hook has come. Where the parsed screen is read, the host may
    /// keep the program's bracketed paste to itself (a console host that renders what the program
    /// draws need not pass `ESC [ ? 2004 h` on), so from then on the agent's input box drawn on
    /// the parsed screen also says that it reads its input ([`Self::note_input_box`]). Elsewhere
    /// bracketed paste alone does.
    pub(crate) fn note_session_start(&self) {
        if self.reads_screen() && self.screen.waits_for_paste_mode {
            self.session_started.store(true, Ordering::Release);
        }
    }

    /// Whether the parsed screen is to be looked at for the agent's input box
    /// ([`Self::note_input_box`]): from its `SessionStart` hook on, until it has been seen to read
    /// its input.
    pub(crate) fn watches_input_box(&self) -> bool {
        self.session_started.load(Ordering::Acquire) && !self.reads_its_input()
    }

    /// Records whether the parsed screen shows the agent's input box, as looked at while
    /// [`Self::watches_input_box`] says to; once shown, it stays seen.
    pub(crate) fn note_input_box(&self, shown: bool) {
        if shown && self.session_started.load(Ordering::Acquire) {
            self.input_box_seen.store(true, Ordering::Release);
        }
    }

    /// Records whether the parsed terminal has bracketed paste on after a piece of output.
    ///
    /// Claude Code turns it on and straight off again while probing the terminal as it starts,
    /// long before it reads its input, and a read can end between the two; so until the hook has
    /// come, bracketed paste turned off again undoes the sighting. From the hook on it is latched:
    /// it does not matter whether the agent turns it off again.
    pub fn note_paste_mode(&self, on: bool) {
        if on {
            self.paste_mode_seen.store(true, Ordering::Release);
        } else {
            let _order = self
                .paste_mode_order
                .lock()
                .expect("trust paste mutex poisoned");
            if !self.hook_seen.load(Ordering::Acquire) {
                self.paste_mode_seen.store(false, Ordering::Release);
            }
        }
    }

    /// Lets through what is held for an agent whose hook has not come ([`Screen::hook_fallback`]),
    /// and says whether it did: not once a hook has arrived or the screen has been sighted, which
    /// this checks under the window's lock a sighting is made under.
    pub(super) fn release_without_hook(&self) -> bool {
        let _watched = self.window.lock().expect("trust window mutex poisoned");
        if self.hook_seen.load(Ordering::Acquire) || self.sighted.load(Ordering::Acquire) {
            return false;
        }
        self.released_without_hook.store(true, Ordering::Release);
        true
    }

    /// Ends the watch as if the session had run its first hook, for a test's stand-in that stands
    /// for a session past its screen.
    #[cfg(test)]
    pub(crate) fn end_watch(&self) {
        self.note_paste_mode(true);
        self.note_hook();
    }

    /// For a press that has sent its key: says that it waits for the hook's carry. Taken back by
    /// [`Self::stop_awaiting_carry`].
    pub(super) fn await_carry(&self) {
        let mut slot = self.carry.lock().expect("trust carry mutex poisoned");
        slot.awaited = true;
        slot.outcome = None;
    }

    /// Waits up to `timeout` for the hook's carry outcome; `None` when none came. The press still
    /// awaits it until [`Self::stop_awaiting_carry`].
    pub(super) fn carry_outcome(
        &self,
        timeout: Duration,
    ) -> Option<std::result::Result<(), CarryFailure>> {
        let slot = self.carry.lock().expect("trust carry mutex poisoned");
        let (mut slot, _) = self
            .carried_over
            .wait_timeout_while(slot, timeout, |slot| slot.outcome.is_none())
            .expect("trust carry mutex poisoned");
        slot.outcome.take()
    }

    /// For a press that stops waiting without its key having led anywhere: whatever outcome a
    /// hook left meanwhile is the caller's, and a later one is the hook's own to report.
    pub(super) fn stop_awaiting_carry(&self) -> Option<std::result::Result<(), CarryFailure>> {
        let mut slot = self.carry.lock().expect("trust carry mutex poisoned");
        slot.awaited = false;
        slot.outcome.take()
    }

    /// For the hook that carried: hands `outcome` to a press waiting for it, or back to the hook,
    /// to report itself, when none is.
    pub(super) fn deliver_carry(
        &self,
        outcome: std::result::Result<(), CarryFailure>,
    ) -> Option<std::result::Result<(), CarryFailure>> {
        let mut slot = self.carry.lock().expect("trust carry mutex poisoned");
        if slot.awaited {
            slot.outcome = Some(outcome);
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

    /// A hand stays raised through a failed press or a cursor key until the screen is known gone.
    pub(crate) fn needs_user(&self) -> bool {
        self.needs_user.load(Ordering::Acquire)
            && !self.hook_seen.load(Ordering::Acquire)
            && !self.pressed.load(Ordering::Acquire)
            && !self.dismissed.load(Ordering::Acquire)
    }

    /// Whether the screen is up and nobody has started answering it.
    pub(crate) fn waiting(&self) -> bool {
        self.sighted.load(Ordering::Acquire)
            && !self.hook_seen.load(Ordering::Acquire)
            && !self.answered.load(Ordering::Acquire)
            && !self.dismissed.load(Ordering::Acquire)
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
