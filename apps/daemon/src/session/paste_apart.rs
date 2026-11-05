//! Submitting a message where the terminal hands the agent a paste as typed keys
//! (`TermFacts::paste_as_keys`) and the agent reads an Enter that arrives in the same burst as a
//! line break in the text (`ScreenReader::joins_enter_to_key_paste`). Written together with the
//! paste, the Enter would stay in the input box as a new line and the message would never be sent.
//!
//! So the paste is written alone, and its submit key only once the agent has caught up with it:
//! its input box shows the whole message ([`Pasted::shown_in`]), and the box's rows have then stayed
//! the same for the settle window, which is past the burst ([`box_settled`]). A box that shows text
//! is not enough: a loaded agent draws part of a paste before it has taken the rest, and an Enter
//! among keys it has not consumed yet is a line break again. It is the box that has to settle, not
//! the whole output: during a turn Codex redraws its title spinner and its status line every frame,
//! so its output never goes quiet. A box no longer recognised at all says nothing either way, and
//! its time is capped apart ([`Timing::unread`]). Every wait is a gate; one that times out leaves
//! the message in the box unsubmitted ([`NotSubmitted`]) rather than pressing a key that would only
//! add a line break.
//!
//! The input lock is not held meanwhile: the host may hold the agent until a query of its is
//! answered (`TermFacts::host_waits_on_queries`), and the answer is written under that lock.
//!
//! Whether the key took is read off the input box: an empty box, or none recognised, is a sent
//! message. The key is pressed once more only on evidence that the first one became a line break
//! in the text: the box's rows changed, end in an empty row and still show the whole message, and
//! they settled so. A box whose rows the key did not change is waited on, never pressed into again,
//! and one still holding the message once the waits are over is reported as a message not
//! submitted, not as one sent.
//!
//! Each step is logged at info level with lengths and row counts only, never the message's text.

use std::io;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use super::input_box::text_of;
use super::{wait, LiveSession};
use crate::platform::term::{self, Input, PartialWrite};

/// How long the waits of a submit last.
#[derive(Debug, Clone, Copy)]
pub(super) struct Timing {
    /// At most, for the agent to show the whole paste in its input box, settled, not counting
    /// the time no input box is recognised.
    pub paste_taken: Duration,
    /// At most, in all, that no input box is recognised while waiting for the paste to show.
    pub unread: Duration,
    /// At most, after a submit key, for the input box to empty, or to show the key taken as a line
    /// break, settled.
    pub submit_taken: Duration,
    /// How long the box's rows have to stay the same to count as settled.
    pub settle: Duration,
}

/// Codex draws a paste it took as keys 100–150 ms after it and empties its box within a frame of
/// the Enter that sends it, on an idle machine. Under load, with its host holding it up for an
/// answer to a cursor position query, a message was still not taken after waits of several
/// seconds; the caps leave room for that, and only bound how long a message that is never taken
/// holds up its queue. A box settles over the span the terminal backend takes to finish drawing.
///
/// Codex on a Windows console host was seen drawing a screen whose composer is not recognised for
/// as long as a turn ran a tool, with the paste in its composer, and drawing that composer again
/// once the turn ended; so the time no box is recognised has a cap of its own, long enough for a
/// turn running tools to end.
fn timing() -> Timing {
    Timing {
        paste_taken: Duration::from_secs(15),
        unread: Duration::from_secs(10 * 60),
        submit_taken: Duration::from_secs(15),
        settle: wait::quiet(),
    }
}

/// The fewest characters, whitespace aside, a box showing only the end of a message has to show
/// for that to count as the whole message drawn: a composer too small for a long message scrolls
/// it to its end, where the cursor is. Fewer could be a prefix drawn so far that happens to repeat
/// the message's end.
const TAIL_SHOWN: usize = 40;

/// The error of a message written in full that the agent left in its input box, its submit key
/// pressed or held back because the agent never caught up with it, or that the session could no
/// longer take the key for. Its text is in the box, as an unsent message's is, but the key has had
/// its chance here: no later one presses it again.
#[derive(Debug)]
pub(crate) struct NotSubmitted;

impl std::fmt::Display for NotSubmitted {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("the agent did not take the message out of its input box")
    }
}

impl std::error::Error for NotSubmitted {}

/// What submitting a pasted message needs of a session.
pub(super) trait Target {
    /// The session, for the logs.
    fn session(&self) -> &str;
    /// The rows of the input box, each trimmed and empty rows kept, `None` where no input box is
    /// recognised.
    fn box_rows(&self) -> Option<Vec<String>>;
    /// Whether the session has ended, is being stopped, or has been archived, so that no key will
    /// be written into it.
    fn gone(&self) -> bool;
    /// Writes `keys` under the input lock, unless the session can no longer take them, which is
    /// `Ok(false)`: among the reasons, a dialog on the screen, which the key would answer.
    fn press(&self, keys: &[Input]) -> Result<bool, PartialWrite>;
}

/// `message` split after its paste where what follows the paste has to wait for it to be taken
/// (`apart`): the inputs to write now, and the rest. Without `apart`, or without a paste with
/// something after it, the whole message is written now.
pub(super) fn split(message: &[Input], apart: bool) -> (&[Input], &[Input]) {
    match message
        .iter()
        .position(|input| matches!(input, Input::Paste(_)))
    {
        Some(at) if apart && at + 1 < message.len() => message.split_at(at + 1),
        _ => (message, &[]),
    }
}

/// How an input box shows a pasted message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shown {
    /// All of its text, and nothing else where the message was written [`Pasted::alone`].
    Whole,
    /// The end of it, at least [`TAIL_SHOWN`] characters, its start scrolled out of the box.
    Tail,
    /// The placeholder Codex shows for a large paste, naming the message's length.
    Placeholder,
    /// Not all of it, or something else.
    No,
}

/// A pasted message as its input box is compared with.
struct Pasted {
    /// Its text with every whitespace character taken out, so that where the box wraps it, the
    /// indent and trailing blanks the box's rows lose, and the trimming the agent does on submit do
    /// not count.
    squeezed: String,
    /// The lengths, in characters, a placeholder for it may name: as written, and trimmed.
    lengths: [usize; 2],
    /// Written into a box found empty, which then has to show the message alone: text the user
    /// typed meanwhile would be sent along with it.
    alone: bool,
}

impl Pasted {
    /// The message pasted by `now`, the inputs written before the submit key, into a box found
    /// empty where `alone`.
    fn new(now: &[Input], alone: bool) -> Self {
        let text = now
            .iter()
            .rev()
            .find_map(|input| match input {
                Input::Paste(text) => Some(text.as_str()),
                _ => None,
            })
            .unwrap_or_default();
        Self {
            squeezed: squeezed(text),
            lengths: [text.chars().count(), text.trim().chars().count()],
            alone,
        }
    }

    /// The message's length for the logs, in characters whitespace aside, as compared.
    fn len(&self) -> usize {
        self.squeezed.chars().count()
    }

    /// How the box `rows` show the message. Codex replaces a large paste with
    /// `[Pasted Content <n> chars]`, as captured with a bracketed paste on macOS
    /// (`testdata/codex_hooked_empty.bin`); that it does so for a paste it took as keys, and
    /// counts the characters as written, is assumed.
    fn shown_in(&self, rows: &[String]) -> Shown {
        let shown = squeezed(&rows.concat());
        if shown == self.squeezed || (!self.alone && shown.contains(&self.squeezed)) {
            Shown::Whole
        } else if shown.chars().count() >= TAIL_SHOWN && self.squeezed.ends_with(&shown) {
            Shown::Tail
        } else if self
            .lengths
            .iter()
            .any(|length| shown == format!("[PastedContent{length}chars]"))
        {
            Shown::Placeholder
        } else {
            Shown::No
        }
    }
}

fn squeezed(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// The length, in characters whitespace aside, of what `rows` hold, for the logs.
fn box_len(rows: Option<&[String]>) -> Option<usize> {
    rows.map(|rows| squeezed(&rows.concat()).chars().count())
}

fn millis(since: Instant) -> u128 {
    since.elapsed().as_millis()
}

/// Writes `keys`, what follows the paste `now` already written, once the paste has been taken,
/// and checks that they submitted it; see the module header. A failure's `written` counts the
/// bytes of the whole message as [`PartialWrite`] does, so a message whose keys were held back
/// counts the paste alone and one written in full but left in the box counts all of it, both
/// carrying [`NotSubmitted`]. `alone` says the paste went into a box found empty
/// ([`Pasted::alone`]).
pub(super) fn submit(
    target: &impl Target,
    timing: Timing,
    now: &[Input],
    keys: &[Input],
    alone: bool,
) -> Result<(), PartialWrite> {
    let written = term::encode(now).len();
    let full = written + term::encode(keys).len();
    let pasted = Pasted::new(now, alone);
    let Some(rows) = caught_up(target, &pasted, timing) else {
        return Err(not_submitted(written));
    };
    press(target, &pasted, keys, 1, &rows, written, full)?;
    let rows = match after_press(target, &pasted, &rows, timing, 1) {
        AfterPress::Sent => return Ok(()),
        AfterPress::LineBreak(rows) => rows,
        AfterPress::Undecided => return Err(not_submitted(full)),
    };
    press(target, &pasted, keys, 2, &rows, full, full)?;
    match after_press(target, &pasted, &rows, timing, 2) {
        AfterPress::Sent => Ok(()),
        AfterPress::LineBreak(_) | AfterPress::Undecided => Err(not_submitted(full)),
    }
}

fn not_submitted(written: usize) -> PartialWrite {
    PartialWrite {
        written,
        error: io::Error::other(NotSubmitted),
    }
}

/// Polls the box's rows until they have stayed the same for `settle`, and returns whether they did
/// before `deadline`, with the rows last read.
fn box_settled(
    target: &impl Target,
    settle: Duration,
    deadline: Instant,
) -> (bool, Option<Vec<String>>) {
    let mut rows = target.box_rows();
    let mut since = Instant::now();
    loop {
        if since.elapsed() >= settle {
            return (true, rows);
        }
        if Instant::now() >= deadline {
            return (false, rows);
        }
        std::thread::sleep(wait::POLL.min(settle));
        let next = target.box_rows();
        if next != rows {
            rows = next;
            since = Instant::now();
        }
    }
}

/// Waits, up to its caps in `timing`, for the box to show the whole message and settle so, and
/// returns the box's rows then; `None` when that never came. For a message pasted into a box found
/// empty ([`Pasted::alone`]), the time no box is recognised does not count against
/// [`Timing::paste_taken`], up to [`Timing::unread`]: the box was read before the paste and may be
/// drawn again with it, but nothing says yet whether it was taken. A message written without its
/// box found empty gets no more than the paste cap.
fn caught_up(target: &impl Target, pasted: &Pasted, timing: Timing) -> Option<Vec<String>> {
    let started = Instant::now();
    let mut unread = Duration::ZERO;
    let mut looked = started;
    let shown_in = |rows: Option<&[String]>| rows.map_or(Shown::No, |rows| pasted.shown_in(rows));
    let log = |result: &str, rows: Option<&[String]>, drawn_ms: Option<u128>, unread: Duration| {
        tracing::info!(
            session = target.session(),
            result,
            elapsed_ms = millis(started),
            unread_ms = unread.as_millis(),
            drawn_ms = ?drawn_ms,
            box_len = ?box_len(rows),
            message_len = pasted.len(),
            rows = ?rows.map(<[String]>::len),
            shown = ?shown_in(rows),
            "pasted message: waited for the input box to show it all"
        );
    };
    loop {
        let rows = target.box_rows();
        let now = Instant::now();
        if rows.is_none() && pasted.alone {
            unread += now - looked;
        }
        looked = now;
        let deadline = started + timing.paste_taken + unread;
        if shown_in(rows.as_deref()) != Shown::No {
            let drawn_ms = millis(started);
            let (settled, rows) = box_settled(target, timing.settle, deadline);
            looked = Instant::now();
            if !settled {
                log(
                    "input box never settled",
                    rows.as_deref(),
                    Some(drawn_ms),
                    unread,
                );
                return None;
            }
            match rows {
                Some(rows) if pasted.shown_in(&rows) != Shown::No => {
                    log("caught up", Some(&rows), Some(drawn_ms), unread);
                    return Some(rows);
                }
                // Drawn again without the message, or no longer recognised: looked at afresh.
                _ => continue,
            }
        }
        let result = if target.gone() {
            "session gone"
        } else if unread >= timing.unread {
            "input box not recognised for too long"
        } else if now >= deadline {
            "never shown whole"
        } else {
            std::thread::sleep(wait::POLL);
            continue;
        };
        log(result, rows.as_deref(), None, unread);
        return None;
    }
}

/// Presses the submit key, the `nth` time, into a box showing `rows`; `before` is what of the
/// message is in so far, out of `full`.
fn press(
    target: &impl Target,
    pasted: &Pasted,
    keys: &[Input],
    nth: u8,
    rows: &[String],
    before: usize,
    full: usize,
) -> Result<(), PartialWrite> {
    let pressed = target.press(keys);
    let result = match &pressed {
        Ok(true) => "written",
        Ok(false) => "refused",
        Err(_) => "failed",
    };
    tracing::info!(
        session = target.session(),
        press = nth,
        result,
        box_len = ?box_len(Some(rows)),
        message_len = pasted.len(),
        rows_before = rows.len(),
        "pasted message: submit key"
    );
    match pressed {
        Ok(true) => Ok(()),
        Ok(false) => Err(not_submitted(before)),
        Err(partial) if partial.written == 0 => {
            // The message stays unsent either way; the write's own failure is kept here, as the
            // caller only learns that the key did not take.
            tracing::warn!(error = %partial.error, "the submit key of a pasted message could not be written");
            Err(not_submitted(before))
        }
        Err(partial) => Err(PartialWrite {
            written: (before + partial.written).min(full),
            error: partial.error,
        }),
    }
}

/// What a submit key did to the input box.
#[derive(Debug, PartialEq, Eq)]
enum AfterPress {
    /// The box emptied, or is no longer recognised.
    Sent,
    /// The key became a line break after the message, as these rows show, settled.
    LineBreak(Vec<String>),
    /// Neither, within the cap.
    Undecided,
}

/// What the box `rows` say of a submit key pressed into a box showing `before`, or `None` while
/// they say nothing yet.
fn read_press(rows: Option<Vec<String>>, before: &[String], pasted: &Pasted) -> Option<AfterPress> {
    let Some(rows) = rows else {
        // A box no longer recognised is no sign of the text left in it: pressing again into what
        // replaced it, a dialog perhaps, would answer it.
        return Some(AfterPress::Sent);
    };
    if text_of(rows.clone()).is_empty() {
        return Some(AfterPress::Sent);
    }
    // Read off the rows rather than their text, which drops the empty row a line break leaves at
    // the end and so looks the same whether the key became one or did nothing yet.
    let line_break = rows != before
        && rows.last().is_some_and(String::is_empty)
        && pasted.shown_in(&rows) != Shown::No;
    line_break.then_some(AfterPress::LineBreak(rows))
}

/// Waits, up to its cap in `timing`, for what the `nth` submit key, pressed into a box showing
/// `before`, did.
fn after_press(
    target: &impl Target,
    pasted: &Pasted,
    before: &[String],
    timing: Timing,
    nth: u8,
) -> AfterPress {
    let started = Instant::now();
    let deadline = started + timing.submit_taken;
    let log = |result: &str, rows: Option<&[String]>| {
        tracing::info!(
            session = target.session(),
            press = nth,
            result,
            elapsed_ms = millis(started),
            box_len = ?box_len(rows),
            message_len = pasted.len(),
            rows_before = before.len(),
            rows_after = ?rows.map(<[String]>::len),
            "pasted message: after its submit key"
        );
    };
    loop {
        let mut rows = None;
        let mut read = None;
        wait::wait_for(deadline.saturating_duration_since(Instant::now()), || {
            rows = target.box_rows();
            read = read_press(rows.clone(), before, pasted);
            read.is_some()
        });
        match read {
            Some(AfterPress::Sent) => {
                log("sent", rows.as_deref());
                return AfterPress::Sent;
            }
            Some(AfterPress::LineBreak(_)) => {}
            _ => {
                let unchanged = rows.as_deref() == Some(before);
                log(
                    if unchanged {
                        "rows unchanged"
                    } else {
                        "neither sent nor a line break"
                    },
                    rows.as_deref(),
                );
                return AfterPress::Undecided;
            }
        }
        let (settled, rows) = box_settled(target, timing.settle, deadline);
        if !settled {
            log("line break, input box never settled", rows.as_deref());
            return AfterPress::Undecided;
        }
        match read_press(rows.clone(), before, pasted) {
            Some(AfterPress::Sent) => {
                log("sent", rows.as_deref());
                return AfterPress::Sent;
            }
            Some(AfterPress::LineBreak(rows)) => {
                log("line break", Some(&rows));
                return AfterPress::LineBreak(rows);
            }
            _ if Instant::now() >= deadline => {
                log("neither sent nor a line break", rows.as_deref());
                return AfterPress::Undecided;
            }
            _ => {}
        }
    }
}

impl LiveSession {
    /// Whether a message's keys after its paste wait for the paste to be taken; see the module
    /// header.
    pub(super) fn submits_paste_apart(&self) -> bool {
        term::facts().paste_as_keys
            && crate::adapter::provider(self.agent)
                .screen()
                .joins_enter_to_key_paste()
    }

    /// Writes `rest`, what [`split`] left of a message after `now` was written into a box found
    /// empty where `alone`, as [`submit`] does; nothing when it left nothing.
    pub(super) fn submit_rest(
        &self,
        now: &[Input],
        rest: &[Input],
        alone: bool,
    ) -> Result<(), PartialWrite> {
        if rest.is_empty() {
            return Ok(());
        }
        submit(self, timing(), now, rest, alone)
    }
}

impl Target for LiveSession {
    fn session(&self) -> &str {
        &self.id
    }

    fn box_rows(&self) -> Option<Vec<String>> {
        LiveSession::input_box_rows(self, &self.screen_text_undimmed())
    }

    fn gone(&self) -> bool {
        self.is_frozen() || self.poll_exit()
    }

    /// Under the input lock, with the checks every write of a message makes, but not the one on
    /// an input replacement: a message written while the draft is stashed is the replacement's
    /// own.
    fn press(&self, keys: &[Input]) -> Result<bool, PartialWrite> {
        let input = self.pty.lock_input();
        if self.is_frozen()
            || self.poll_exit()
            || !self.allows_automatic_input()
            || self.trust.holds_writes()
            || self.look_for_dialog().current.is_some()
        {
            return Ok(false);
        }
        self.input_writes.fetch_add(1, Ordering::AcqRel);
        self.pty.write_input_locked(&input, keys).map(|()| true)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;

    const QUICK: Timing = Timing {
        paste_taken: Duration::from_millis(300),
        unread: Duration::from_millis(900),
        submit_taken: Duration::from_millis(300),
        settle: Duration::from_millis(60),
    };

    /// How long the burst a paste opens lasts, well inside [`QUICK`]'s settle window.
    const BURST: Duration = Duration::from_millis(20);

    /// An agent with a paste burst, as Codex's on a Windows console: a paste arrives as keys and
    /// opens a burst for [`BURST`], and an Enter inside the burst is a line break in the box rather
    /// than a submit. How it draws its box is set by the fields below.
    #[derive(Default)]
    struct BurstAgent {
        text: RefCell<String>,
        burst_until: Cell<Option<Instant>>,
        /// The Enter is taken into the burst even after quiet output this many times.
        stuck_enters: Cell<u32>,
        /// Enter does nothing at all.
        ignores_enter: bool,
        refuses_keys: bool,
        /// A row under the text changes at every look, as a box that keeps redrawing.
        churns: bool,
        looks: Cell<u32>,
        /// Only this many characters of the text are ever drawn.
        draws_only: Option<usize>,
        /// Rows wrap at this many characters, losing the blanks at their ends.
        width: Option<usize>,
        /// The box shows at most this many rows, the last ones.
        height: Option<usize>,
        /// A paste longer than this many characters shows as Codex's placeholder.
        placeholder_over: Option<usize>,
        /// No box is recognised for this long after a paste, as Codex's on a Windows console
        /// while its turn runs a tool.
        unrecognised_for: Option<Duration>,
        pasted_at: Cell<Option<Instant>>,
        /// The session has ended.
        ended: bool,
        sent: RefCell<Vec<String>>,
        writes: RefCell<Vec<Vec<Input>>>,
    }

    impl BurstAgent {
        fn take(&self, inputs: &[Input]) {
            self.writes.borrow_mut().push(inputs.to_vec());
            for input in inputs {
                match input {
                    Input::Paste(text) => {
                        match self.placeholder_over {
                            Some(over) if text.chars().count() > over => {
                                let placeholder =
                                    format!("[Pasted Content {} chars]", text.chars().count());
                                self.text.borrow_mut().push_str(&placeholder);
                            }
                            _ => self.text.borrow_mut().push_str(text),
                        }
                        self.burst_until.set(Some(Instant::now() + BURST));
                        self.pasted_at.set(Some(Instant::now()));
                    }
                    Input::Enter if self.ignores_enter => {}
                    Input::Enter if self.in_burst() || self.stuck_enters.get() > 0 => {
                        self.stuck_enters
                            .set(self.stuck_enters.get().saturating_sub(1));
                        self.text.borrow_mut().push('\n');
                    }
                    Input::Enter => {
                        let text = self.text.take();
                        self.sent.borrow_mut().push(text.trim_end().to_string());
                    }
                    other => panic!("unexpected input {other:?}"),
                }
            }
        }

        fn in_burst(&self) -> bool {
            self.burst_until
                .get()
                .is_some_and(|until| Instant::now() < until)
        }

        fn enters(&self) -> usize {
            self.writes
                .borrow()
                .iter()
                .filter(|write| write.contains(&Input::Enter))
                .count()
        }
    }

    impl Target for BurstAgent {
        fn session(&self) -> &str {
            "test"
        }

        fn box_rows(&self) -> Option<Vec<String>> {
            if let (Some(hidden), Some(at)) = (self.unrecognised_for, self.pasted_at.get()) {
                if at.elapsed() < hidden {
                    return None;
                }
            }
            let text = self.text.borrow();
            let drawn: String = match self.draws_only {
                Some(only) => text.chars().take(only).collect(),
                None => text.clone(),
            };
            let mut rows: Vec<String> = Vec::new();
            for line in drawn.split('\n') {
                let chars: Vec<char> = line.chars().collect();
                match self.width {
                    Some(width) if chars.len() > width => rows.extend(
                        chars
                            .chunks(width)
                            .map(|row| row.iter().collect::<String>().trim().to_string()),
                    ),
                    _ => rows.push(line.trim().to_string()),
                }
            }
            if let Some(height) = self.height {
                rows.drain(..rows.len().saturating_sub(height));
            }
            if self.churns {
                self.looks.set(self.looks.get() + 1);
                rows.push(self.looks.get().to_string());
            }
            Some(rows)
        }

        fn gone(&self) -> bool {
            self.ended
        }

        fn press(&self, keys: &[Input]) -> Result<bool, PartialWrite> {
            if self.refuses_keys {
                return Ok(false);
            }
            self.take(keys);
            Ok(true)
        }
    }

    fn message_of(text: &str) -> [Input; 2] {
        [Input::Paste(text.into()), Input::Enter]
    }

    fn message() -> [Input; 2] {
        message_of("hello")
    }

    fn paste_len() -> usize {
        term::encode(&message()[..1]).len()
    }

    fn not_submitted(error: &PartialWrite) -> bool {
        error
            .error
            .get_ref()
            .is_some_and(|error| error.is::<NotSubmitted>())
    }

    /// Pastes `text` into `agent` and submits it as a live session does.
    fn send(agent: &BurstAgent, text: &str) -> Result<(), PartialWrite> {
        let message = message_of(text);
        let (now, rest) = split(&message, true);
        agent.take(now);
        submit(agent, QUICK, now, rest, true)
    }

    fn rows(rows: &[&str]) -> Vec<String> {
        rows.iter().map(|row| row.to_string()).collect()
    }

    const LONG: &str = "Message from Octoboard: the report on the branch is ready, \
                        and the tests that cover the change all pass on both hosts.";

    #[test]
    fn a_message_is_split_after_its_paste_only_where_its_keys_wait() {
        let message = message();
        assert_eq!(split(&message, false), (&message[..], &[][..]));
        assert_eq!(split(&message, true), (&message[..1], &message[1..]));
        let keys = [Input::Text("peer".into()), Input::Enter];
        assert_eq!(split(&keys, true), (&keys[..], &[][..]));
        let paste = [Input::Paste("alone".into())];
        assert_eq!(split(&paste, true), (&paste[..], &[][..]));
    }

    #[test]
    fn the_box_counts_as_showing_the_message_only_when_it_shows_all_of_it() {
        let pasted = Pasted::new(&[Input::Paste(format!("  {LONG}\n"))], true);
        assert_eq!(pasted.shown_in(&rows(&[LONG])), Shown::Whole);
        assert_eq!(
            pasted.shown_in(&rows(&["Message from Octoboard: the report"])),
            Shown::No,
            "a paste drawn in part"
        );
        let wrapped = rows(&[
            "Message from Octoboard: the report on the",
            "branch is ready, and the tests that cover",
            "the change all pass on both hosts.",
            "",
        ]);
        assert_eq!(pasted.shown_in(&wrapped), Shown::Whole);
        assert_eq!(
            pasted.shown_in(&rows(&[
                "and the tests that cover",
                "the change all pass on both hosts."
            ])),
            Shown::Tail,
            "the end of a message too long for the box"
        );
        assert_eq!(
            pasted.shown_in(&rows(&["both hosts."])),
            Shown::No,
            "too short an end to tell from a prefix"
        );
        assert_eq!(pasted.shown_in(&rows(&[""])), Shown::No);
        let length = format!("  {LONG}\n").chars().count();
        assert_eq!(
            pasted.shown_in(&rows(&[&format!("[Pasted Content {length} chars]")])),
            Shown::Placeholder
        );
        assert_eq!(
            pasted.shown_in(&rows(&["[Pasted Content 12 chars]"])),
            Shown::No,
            "a placeholder for another paste"
        );
    }

    #[test]
    fn an_enter_written_with_the_paste_stays_in_the_box_as_a_line_break() {
        // What one write of the whole message does to such an agent.
        let agent = BurstAgent::default();
        agent.take(&message());
        assert_eq!(agent.box_rows(), Some(rows(&["hello", ""])));
        assert!(agent.sent.borrow().is_empty());
    }

    #[test]
    fn the_enter_written_once_the_paste_is_taken_sends_it() {
        let agent = BurstAgent::default();
        let started = Instant::now();
        send(&agent, "hello").expect("sent");
        assert!(started.elapsed() >= QUICK.settle, "the box settled first");
        assert_eq!(*agent.sent.borrow(), ["hello"]);
        assert_eq!(
            *agent.writes.borrow(),
            [vec![Input::Paste("hello".into())], vec![Input::Enter]],
            "the paste and its Enter are separate writes"
        );
    }

    #[test]
    fn a_message_the_box_wraps_or_scrolls_is_sent() {
        for agent in [
            BurstAgent {
                width: Some(20),
                ..BurstAgent::default()
            },
            BurstAgent {
                width: Some(20),
                height: Some(3),
                ..BurstAgent::default()
            },
            BurstAgent {
                placeholder_over: Some(50),
                ..BurstAgent::default()
            },
        ] {
            send(&agent, LONG).expect("sent");
            assert_eq!(agent.sent.borrow().len(), 1);
        }
    }

    #[test]
    fn a_box_that_never_shows_the_whole_message_gets_no_key() {
        let agent = BurstAgent {
            draws_only: Some(3),
            ..BurstAgent::default()
        };
        let error = send(&agent, "hello").unwrap_err();
        assert!(not_submitted(&error));
        assert_eq!(error.written, paste_len(), "only the paste went in");
        assert_eq!(agent.enters(), 0);
        assert_eq!(
            agent.box_rows(),
            Some(rows(&["hel"])),
            "no line break added"
        );
    }

    #[test]
    fn a_box_not_recognised_for_longer_than_the_paste_cap_is_waited_out() {
        let agent = BurstAgent {
            unrecognised_for: Some(QUICK.paste_taken * 2),
            ..BurstAgent::default()
        };
        send(&agent, "hello").expect("sent once the box is drawn again");
        assert_eq!(*agent.sent.borrow(), ["hello"]);
        assert_eq!(agent.enters(), 1);
    }

    #[test]
    fn only_a_message_pasted_into_a_box_found_empty_waits_out_an_unrecognised_box() {
        let agent = BurstAgent {
            unrecognised_for: Some(QUICK.paste_taken * 2),
            ..BurstAgent::default()
        };
        let message = message();
        let (now, rest) = split(&message, true);
        agent.take(now);
        let error = submit(&agent, QUICK, now, rest, false).unwrap_err();
        assert!(not_submitted(&error));
        assert_eq!(agent.enters(), 0);
    }

    #[test]
    fn a_box_never_recognised_again_gets_no_key() {
        for ended in [false, true] {
            let agent = BurstAgent {
                unrecognised_for: Some(Duration::from_secs(60)),
                ended,
                ..BurstAgent::default()
            };
            let started = Instant::now();
            let error = send(&agent, "hello").unwrap_err();
            assert!(not_submitted(&error));
            assert_eq!(error.written, paste_len());
            assert_eq!(agent.enters(), 0);
            if ended {
                assert!(
                    started.elapsed() < QUICK.paste_taken,
                    "an ended session is not waited on"
                );
            } else {
                assert!(started.elapsed() >= QUICK.unread);
            }
        }
    }

    #[test]
    fn a_box_that_keeps_changing_gets_no_key() {
        let agent = BurstAgent {
            churns: true,
            ..BurstAgent::default()
        };
        let error = send(&agent, "hello").unwrap_err();
        assert!(not_submitted(&error));
        assert_eq!(agent.enters(), 0);
    }

    #[test]
    fn a_box_holding_other_text_besides_the_message_gets_no_key_unless_mixing_is_allowed() {
        for (alone, sent) in [(true, false), (false, true)] {
            let agent = BurstAgent {
                text: RefCell::new("typed ".into()),
                ..BurstAgent::default()
            };
            let message = message();
            let (now, rest) = split(&message, true);
            agent.take(now);
            let result = submit(&agent, QUICK, now, rest, alone);
            assert_eq!(result.is_ok(), sent, "alone: {alone}");
            assert_eq!(agent.enters(), usize::from(sent), "alone: {alone}");
        }
        let pasted = Pasted::new(&[Input::Paste("hello".into())], true);
        assert_eq!(pasted.shown_in(&rows(&["typed hello"])), Shown::No);
    }

    #[test]
    fn a_box_showing_the_key_as_a_line_break_gets_it_once_more() {
        let agent = BurstAgent {
            stuck_enters: Cell::new(1),
            ..BurstAgent::default()
        };
        send(&agent, "hello").expect("sent by the second key");
        assert_eq!(*agent.sent.borrow(), ["hello"]);
        assert_eq!(agent.enters(), 2);
    }

    #[test]
    fn a_box_the_key_did_not_change_gets_no_second_key() {
        for text in ["hello", "hello\n"] {
            let agent = BurstAgent {
                ignores_enter: true,
                ..BurstAgent::default()
            };
            let error = send(&agent, text).unwrap_err();
            assert!(not_submitted(&error));
            assert_eq!(error.written, term::encode(&message_of(text)).len());
            assert_eq!(agent.enters(), 1, "{text:?}: no second key");
        }
    }

    #[test]
    fn a_line_break_needs_an_empty_row_after_the_message() {
        let pasted = Pasted::new(&[Input::Paste("hello".into())], true);
        let before = rows(&["hello"]);
        assert_eq!(read_press(Some(before.clone()), &before, &pasted), None);
        assert_eq!(
            read_press(Some(rows(&["hello", "x"])), &before, &pasted),
            None,
            "a row that is not empty"
        );
        assert_eq!(
            read_press(Some(rows(&["hel", ""])), &before, &pasted),
            None,
            "the message no longer shown whole"
        );
        assert_eq!(
            read_press(Some(rows(&["hello", ""])), &before, &pasted),
            Some(AfterPress::LineBreak(rows(&["hello", ""])))
        );
        assert_eq!(
            read_press(Some(rows(&[""])), &before, &pasted),
            Some(AfterPress::Sent)
        );
        assert_eq!(
            read_press(None, &before, &pasted),
            Some(AfterPress::Sent),
            "a box no longer recognised"
        );
    }

    #[test]
    fn a_box_holding_the_text_after_two_keys_is_reported_not_submitted() {
        let agent = BurstAgent {
            stuck_enters: Cell::new(5),
            ..BurstAgent::default()
        };
        let error = send(&agent, "hello").unwrap_err();
        assert!(not_submitted(&error));
        assert_eq!(error.written, term::encode(&message()).len());
        assert!(agent.sent.borrow().is_empty());
        assert_eq!(agent.enters(), 2, "no third key");
    }

    #[test]
    fn a_session_that_can_no_longer_take_the_key_leaves_the_message_unsubmitted() {
        let agent = BurstAgent {
            refuses_keys: true,
            ..BurstAgent::default()
        };
        let error = send(&agent, "hello").unwrap_err();
        assert!(not_submitted(&error));
        assert_eq!(error.written, paste_len());
    }

    #[test]
    fn only_a_host_that_hands_a_paste_over_as_keys_splits_codex_s_messages() {
        assert!(crate::adapter::provider(crate::protocol::AgentId::CODEX)
            .screen()
            .joins_enter_to_key_paste());
        assert!(!crate::adapter::provider(crate::protocol::AgentId::CLAUDE)
            .screen()
            .joins_enter_to_key_paste());
        assert_eq!(term::facts().paste_as_keys, cfg!(windows));
    }
}
