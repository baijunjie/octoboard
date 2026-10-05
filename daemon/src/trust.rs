//! Answering Claude Code's workspace-trust screen for the user, once they have agreed to that.
//!
//! The first time Claude Code runs in a directory it stops on a screen asking whether the folder
//! is trusted, and waits for a person. A session the hub starts in a fresh project would sit there
//! unattended. Octoboard answers it by typing at the terminal — a Down and an Enter, because the
//! cursor starts on "No, exit" — and never by editing Claude Code's global config file, which
//! Claude Code rewrites constantly and which is the user's. The user's agreement comes first, in a
//! dialog of Octoboard's own, and is remembered per project (`Project::claude_trust_consent`).
//!
//! **Detection.** Every Claude Code session's output is watched by a [`TrustState`], fed from the
//! PTY reader thread whether or not a client is attached. The screen is recognised by how it is
//! first drawn — the cursor glyph, then its two options one after the other — after the output has
//! been reduced to bare text ([`normalise`]): Claude Code does not print a space between words, it
//! moves the cursor, so the raw bytes never contain "Yes, I trust this folder". A sighting is
//! signalled once per session. That is enough for "once per occurrence" because the screen cannot
//! come back within one process: no hook runs before it is answered (measured against Claude Code
//! 2.1.289 with a `SessionStart` hook given through `--settings`), so the first hook callback
//! retires the watch. A session that never runs hooks is covered by a bound instead: the watch
//! also ends after [`WATCH_BYTES`] of output or [`WATCH_SECONDS`] seconds, the screen being the
//! first thing Claude Code prints. A transcript that quotes the screen is not taken for it unless
//! it reproduces the first-draw pattern inside that window before any hook has run; and whatever is
//! sighted, keys are sent only if the cursor check below passes.
//!
//! **What happens next** is decided by [`supervise`]: a hub session's working directory is
//! Octoboard's own console directory, so it is answered at once; a project session is answered at
//! once when the project has the user's consent, and otherwise a `claude_trust_prompt` is
//! broadcast and the screen is left alone until `confirm_claude_trust` comes back. With no client
//! connected, or after "Not now", nothing is sent and the screen waits for the person to answer it
//! in the terminal.
//!
//! **Sending keys is the dangerous part**, so [`answer`] is the only place that does it, takes no
//! text, and checks before each key:
//!
//! - the session is a Claude Code session and its screen was sighted and has not been answered;
//! - the terminal's latest output still shows the screen with the cursor on "No, exit" — a Down
//!   from the other option would wrap to it, and an Enter there exits;
//! - nobody else has written into the session's input since then ([`LiveSession::input_writes`]):
//!   the user pressing Down in the terminal, or a queued message, would move the cursor under the
//!   Enter;
//! - after the Down, the output shows the cursor on "Yes, I trust this folder" and has been quiet
//!   for a moment. Only then is Enter sent, after a last look at both of those.
//!
//! Whatever cannot be confirmed ends the attempt with the keys not sent, and the screen waits for
//! the person to answer it. Each session is answered at most once, so a failed attempt is not
//! retried behind the person's back.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::protocol::{Agent, Event, Project, Role, Session};
use crate::session::LiveSession;
use crate::state::AppState;

/// The screen as first drawn, as [`normalise`] leaves it: the cursor on the first option, then the
/// two options one after the other. The captured first draw is the only form the screen has at its
/// start, so nothing looser is accepted — the cursor is never on the second option when it appears.
const SCREEN_PATTERN: &str = "\u{276F}No,exitYes,Itrustthisfolder";

/// The options on their own, for reading where the cursor is and whether either is still shown.
const ACCEPT_ITEM: &str = "Yes,Itrustthisfolder";
const DECLINE_ITEM: &str = "No,exit";

/// The glyph in front of the option the cursor is on.
const CURSOR_GLYPH: char = '\u{276F}';

/// How much recent output is searched for the screen. The screen itself is about 1.5 KiB; the
/// rest is room for it to arrive in pieces and for what comes before it.
const WINDOW: usize = 16 * 1024;

/// How long the watch for the screen lasts: it is the first thing Claude Code prints, so a session
/// that has printed this much or has been running this long and has not shown it, will not.
const WATCH_BYTES: usize = 64 * 1024;
const WATCH_SECONDS: u64 = 30;

/// How long the session's output has to stay silent before the Enter, and after it.
const QUIET: Duration = Duration::from_millis(100);

/// How long to let Claude Code finish drawing and start reading its input before the first key.
const SETTLE: Duration = Duration::from_millis(300);

/// How long the cursor has to take to move after the Down, and the screen to go after the Enter.
const MOVE_TIMEOUT: Duration = Duration::from_secs(2);
const DISMISS_TIMEOUT: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(25);

/// The keys. Down is the sequence a terminal sends for the arrow key; Enter is `\r`, as the
/// terminal sends it.
const DOWN: &[u8] = b"\x1b[B";
const ENTER: &[u8] = b"\r";

/// Which option holds the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cursor {
    Decline,
    Accept,
}

/// Reduces terminal output to the text a person would read off the screen, with every space
/// removed: escape sequences, control characters and whitespace are all dropped. Dropping the
/// whitespace is the point — Claude Code positions each word with a cursor-move sequence instead
/// of printing a space, so what is left of "Yes, I trust this folder" is `Yes,Itrustthisfolder`,
/// and that holds however the output was cut into reads or re-wrapped by a narrower window.
fn normalise(raw: &[u8]) -> String {
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

/// Whether this output holds the trust screen as it is first drawn.
fn is_trust_screen(raw: &[u8]) -> bool {
    normalise(raw).contains(SCREEN_PATTERN)
}

/// Whether this output mentions either of the screen's options at all.
fn mentions_an_option(raw: &[u8]) -> bool {
    let text = normalise(raw);
    text.contains(ACCEPT_ITEM) || text.contains(DECLINE_ITEM)
}

/// Which option the cursor is on, read off the last cursor glyph in the output: after the first
/// draw Claude Code redraws only what changed, so the latest glyph is the current one. `None` when
/// there is none, or what follows it is neither option — which includes a half-drawn line.
fn cursor_on(raw: &[u8]) -> Option<Cursor> {
    let text = normalise(raw);
    let after = &text[text.rfind(CURSOR_GLYPH)? + CURSOR_GLYPH.len_utf8()..];
    if after.starts_with(DECLINE_ITEM) {
        Some(Cursor::Decline)
    } else if after.starts_with(ACCEPT_ITEM) {
        Some(Cursor::Accept)
    } else {
        None
    }
}

#[derive(Default)]
struct Watched {
    window: Vec<u8>,
    total: usize,
}

/// One session's watch for its trust screen, owned by its [`LiveSession`].
pub struct TrustState {
    /// Only Claude Code sessions have the screen, and only theirs are ever answered.
    claude: bool,
    /// The output not yet identified as anything, at most [`WINDOW`] bytes of it, and what the
    /// watch has seen in all. Released when the watch ends.
    window: Mutex<Watched>,
    /// When the watch began, and how much output and how long it lasts.
    started: Instant,
    limits: (usize, Duration),
    /// The watch has ended without a sighting: too much output or too long a time.
    expired: AtomicBool,
    /// The screen has been sighted, once, and `sightings` told.
    sighted: AtomicBool,
    /// A hook callback has arrived, so the screen is past and nothing on the terminal is it.
    hook_seen: AtomicBool,
    /// An answer has been started. A session is answered at most once.
    answered: AtomicBool,
    sightings: UnboundedSender<()>,
    sighting_receiver: Mutex<Option<UnboundedReceiver<()>>>,
}

impl TrustState {
    pub fn new(agent: Agent) -> Self {
        Self::with_limits(agent, WATCH_BYTES, Duration::from_secs(WATCH_SECONDS))
    }

    fn with_limits(agent: Agent, bytes: usize, age: Duration) -> Self {
        let (sightings, receiver) = unbounded_channel();
        Self {
            claude: agent == Agent::Claude,
            window: Mutex::new(Watched::default()),
            started: Instant::now(),
            limits: (bytes, age),
            expired: AtomicBool::new(false),
            sighted: AtomicBool::new(false),
            hook_seen: AtomicBool::new(false),
            answered: AtomicBool::new(false),
            sightings,
            sighting_receiver: Mutex::new(Some(receiver)),
        }
    }

    /// Looks at one more chunk of the session's output; called from the PTY reader thread for
    /// every chunk, so it must stay quick and must not block. Signals a sighting the first time
    /// the screen is complete in what has been seen.
    pub fn feed(&self, chunk: &[u8]) {
        if !self.claude
            || self.hook_seen.load(Ordering::Acquire)
            || self.sighted.load(Ordering::Acquire)
            || self.expired.load(Ordering::Acquire)
        {
            return;
        }
        let mut watched = self.window.lock().expect("trust window mutex poisoned");
        watched.total += chunk.len();
        if watched.total > self.limits.0 || self.started.elapsed() > self.limits.1 {
            self.expired.store(true, Ordering::Release);
            *watched = Watched::default();
            return;
        }
        watched.window.extend_from_slice(chunk);
        if watched.window.len() > WINDOW {
            let excess = watched.window.len() - WINDOW;
            watched.window.drain(..excess);
        }
        if is_trust_screen(&watched.window) {
            *watched = Watched::default();
            if !self.sighted.swap(true, Ordering::AcqRel) {
                // The receiver is gone only once the session is, and nothing is left to tell.
                let _ = self.sightings.send(());
            }
        }
    }

    /// A hook of this session reached the daemon: the screen has been answered, so it is no
    /// longer looked for.
    pub fn mark_hook_seen(&self) {
        self.hook_seen.store(true, Ordering::Release);
        *self.window.lock().expect("trust window mutex poisoned") = Watched::default();
    }

    /// Whether the screen is up and nobody has started answering it.
    fn waiting(&self) -> bool {
        self.sighted.load(Ordering::Acquire)
            && !self.hook_seen.load(Ordering::Acquire)
            && !self.answered.load(Ordering::Acquire)
    }

    /// Takes the right to answer, which only one caller ever gets.
    fn claim_answer(&self) -> bool {
        self.sighted.load(Ordering::Acquire)
            && !self.hook_seen.load(Ordering::Acquire)
            && !self.answered.swap(true, Ordering::AcqRel)
    }

    /// The sightings of this session, for whoever supervises it. There is only one such receiver.
    fn take_sightings(&self) -> Option<UnboundedReceiver<()>> {
        self.sighting_receiver
            .lock()
            .expect("trust receiver mutex poisoned")
            .take()
    }
}

/// Whether Octoboard may answer this session's screen without asking: a hub session always may,
/// because its working directory is the console's own, which holds nothing but the instruction
/// file Octoboard wrote there; a project session only with the user's consent for that project.
fn consented(session: &Session, project: Option<&Project>) -> bool {
    session.role == Role::Hub || project.is_some_and(|project| project.claude_trust_consent)
}

/// Starts acting on this session's trust screen whenever it appears. Gone when the session is.
pub fn supervise(state: &Arc<AppState>, live: &Arc<LiveSession>) {
    let Some(mut sightings) = live.trust.take_sightings() else {
        return;
    };
    let state = state.clone();
    // Weak, so that the session being dropped closes its sightings and ends this task.
    let live = Arc::downgrade(live);
    tokio::spawn(async move {
        while sightings.recv().await.is_some() {
            let Some(live) = live.upgrade() else { break };
            if let Err(err) = on_sighting(&state, &live).await {
                tracing::warn!(session = %live.id, %err, "handling the Claude Code trust screen failed");
            }
        }
    });
}

/// What to do about a screen that has just come up.
async fn on_sighting(state: &Arc<AppState>, live: &Arc<LiveSession>) -> Result<()> {
    let session = state.session_record(&live.id)?;
    let project = match &session.project_id {
        Some(id) => state.store.get_project(id)?,
        None => None,
    };
    if consented(&session, project.as_ref()) {
        answer_and_report(state, live).await;
        return Ok(());
    }
    let project = project.ok_or_else(|| anyhow!("the session's project is gone"))?;
    state.broadcast(Event::ClaudeTrustPrompt {
        session: session.id,
        project: project.id,
        path: project.path,
    });
    Ok(())
}

/// Answers a screen nobody is being asked about, and tells the user when that did not work: the
/// screen is then still up and theirs to answer, which a notice says.
async fn answer_and_report(state: &Arc<AppState>, live: &Arc<LiveSession>) {
    if let Err(err) = answer_off_the_runtime(live.clone()).await {
        report_failure(state, &live.id, &err);
    }
}

/// [`answer`], which blocks, on a blocking thread.
async fn answer_off_the_runtime(live: Arc<LiveSession>) -> Result<()> {
    tokio::task::spawn_blocking(move || answer(&live))
        .await
        .map_err(anyhow::Error::from)
        .and_then(|outcome| outcome)
}

/// The right to answer was not there to take: the screen is not up, or somebody else — a
/// concurrent go-ahead — is answering it already. Nothing went wrong, so nobody is told.
#[derive(Debug)]
struct NotWaiting;

impl std::fmt::Display for NotWaiting {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("the trust screen is not up, or is already being answered")
    }
}

impl std::error::Error for NotWaiting {}

/// Logs a failed answer and tells the user, whoever asked for it: the screen is then still up and
/// theirs to answer. A lost claim is not a failure.
fn report_failure(state: &Arc<AppState>, session_id: &str, err: &anyhow::Error) {
    if err.downcast_ref::<NotWaiting>().is_some() {
        tracing::debug!(session = %session_id, "the trust screen is already being answered");
        return;
    }
    tracing::warn!(session = %session_id, %err, "answering the Claude Code trust screen failed");
    state.broadcast(Event::SessionNotice {
        session: session_id.to_string(),
        message: format!(
            "Octoboard could not answer Claude Code's trust screen ({err}). Answer it in the \
             terminal."
        ),
    });
}

/// The prompts to put to a client that has just been sent a snapshot: one for each live Claude
/// Code project session that is at its screen with nobody answering it and whose project has not
/// consented. A prompt is broadcast once, so a client that missed it — it was not connected, or a
/// snapshot replaced what it had — is told here, and one that already has it ignores the repeat.
pub fn pending_prompts(state: &AppState) -> Vec<Event> {
    let mut prompts = Vec::new();
    for live in state.live_sessions() {
        if !live.trust.waiting() {
            continue;
        }
        let Ok(Some(session)) = state.store.get_session(&live.id) else {
            continue;
        };
        let Some(project_id) = &session.project_id else {
            continue;
        };
        let Ok(Some(project)) = state.store.get_project(project_id) else {
            continue;
        };
        if consented(&session, Some(&project)) {
            continue;
        }
        prompts.push(Event::ClaudeTrustPrompt {
            session: session.id,
            project: project.id,
            path: project.path,
        });
    }
    prompts
}

/// The user's go-ahead for one session's screen, from the dialog a `claude_trust_prompt` opened.
/// Refused unless that session is a running Claude Code project session still waiting at its
/// screen — a stale dialog, a session that has since gone, or a request naming something else
/// entirely changes nothing. Consent is recorded only once the screen has been answered, and only
/// when `remember` asks for it; a failure to answer is reported to the user as a notice as well as
/// to the caller, because the dialog it came from may be closed by then.
pub async fn confirm(state: &Arc<AppState>, session_id: &str, remember: bool) -> Result<()> {
    let session = state.session_record(session_id)?;
    if session.agent != Agent::Claude {
        bail!("only Claude Code sessions have a trust screen");
    }
    let Some(live) = state.live_session(session_id) else {
        bail!("this session is not running");
    };
    if !live.trust.waiting() {
        bail!("this session is not waiting at Claude Code's trust screen any more");
    }
    let Some(project_id) = &session.project_id else {
        bail!("a hub session's trust screen is answered by Octoboard without asking");
    };
    let mut project = state
        .store
        .get_project(project_id)?
        .ok_or_else(|| anyhow!("unknown project {project_id}"))?;

    if let Err(err) = answer_off_the_runtime(live).await {
        report_failure(state, session_id, &err);
        return Err(err);
    }

    if remember && !project.claude_trust_consent {
        state
            .store
            .set_project_claude_trust_consent(&project.id, true)?;
        project.claude_trust_consent = true;
        state.broadcast(Event::ProjectUpserted { project });
    }
    Ok(())
}

/// Sends the keys that choose "Yes, I trust this folder", checking before each one that the screen
/// is as it must be. Blocks, for the settle time and while it watches the screen respond.
///
/// This is the only place that writes keystrokes of its own into a session, and it takes no text:
/// the bytes are the two constants above, so nothing a model or a message wrote can reach it.
fn answer(live: &LiveSession) -> Result<()> {
    if live.agent != Agent::Claude {
        bail!("only Claude Code sessions have a trust screen");
    }
    if !live.trust.claim_answer() {
        return Err(NotWaiting.into());
    }
    // Read before anything is looked at: a write by anyone else after this point means what was
    // looked at may no longer be true.
    let writes = live.input_writes();
    std::thread::sleep(SETTLE);

    let screen = live.recent_output(WINDOW);
    if !is_trust_screen(&screen) {
        bail!("the trust screen is no longer on the terminal");
    }
    // A Down from the second option wraps round to the first, where the Enter would then exit.
    if cursor_on(&screen) != Some(Cursor::Decline) {
        bail!("the cursor is not on the screen's first option, or could not be found");
    }

    let mark = live.output_total();
    write(live, writes, DOWN)?;
    if !wait_for(MOVE_TIMEOUT, || {
        cursor_on(&live.output_since(mark)) == Some(Cursor::Accept)
    }) {
        bail!("the cursor did not move to \"Yes, I trust this folder\", so Enter was not sent");
    }
    // Quiet first, so that what is read next is the screen as it now stands, then a last look at
    // the cursor and at the input, right before the key that cannot be taken back.
    if !wait_quiet(live, MOVE_TIMEOUT) {
        bail!("the terminal did not settle after the Down, so Enter was not sent");
    }
    if cursor_on(&live.output_since(mark)) != Some(Cursor::Accept) {
        bail!("the cursor is no longer on \"Yes, I trust this folder\", so Enter was not sent");
    }

    let mark = live.output_total();
    write(live, writes, ENTER)?;
    let gone = |after: &[u8]| !after.is_empty() && !mentions_an_option(after);
    // Some output without either option, and still without one a moment later.
    if !wait_for(DISMISS_TIMEOUT, || gone(&live.output_since(mark))) {
        bail!("the screen did not go away after Enter");
    }
    std::thread::sleep(QUIET);
    if !gone(&live.output_since(mark)) {
        bail!("the screen was drawn again after Enter");
    }
    Ok(())
}

/// Waits until the session has printed nothing for [`QUIET`], or `timeout` passes.
fn wait_quiet(live: &LiveSession, timeout: Duration) -> bool {
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
fn write(live: &LiveSession, writes: u64, keys: &[u8]) -> Result<()> {
    match live.write_input_if_untouched(writes, keys) {
        Ok(true) => Ok(()),
        Ok(false) => {
            bail!("something else wrote into the terminal meanwhile, so the keys were not sent")
        }
        Err(err) => Err(anyhow!("writing to the terminal failed: {err}")),
    }
}

/// Polls `done` until it holds or `timeout` passes.
fn wait_for(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
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

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use portable_pty::{native_pty_system, CommandBuilder, PtySize};

    use super::*;
    use crate::protocol::{Console, Origin, ProjectSource, SessionStatus};
    use crate::session::{spawn_reader_thread, NewSession};
    use crate::store::{Store, LOCAL_HOST_ID};

    // What Claude Code 2.1.289 actually printed, captured from a real PTY in a directory it had
    // never seen; only the directory's name and the account's plan were replaced. The screen as
    // first drawn, the redraw after one Down, and what replaced the screen after Enter.
    const SCREEN: &[u8] = include_bytes!("../testdata/claude_trust_screen.bin");
    const AFTER_DOWN: &[u8] = include_bytes!("../testdata/claude_trust_after_down.bin");
    const DISMISSED: &[u8] = include_bytes!("../testdata/claude_trust_dismissed.bin");

    /// A fixture's path as a shell script names it, quoted.
    fn fixture(name: &str) -> String {
        format!("'{}/testdata/{name}'", env!("CARGO_MANIFEST_DIR"))
    }

    /// A path as a shell script names it, quoted.
    fn quoted(path: &Path) -> String {
        format!("'{}'", path.display())
    }

    #[test]
    fn the_screen_is_recognised_although_the_raw_bytes_never_spell_its_words() {
        let raw = String::from_utf8_lossy(SCREEN);
        assert!(
            !raw.contains("trust this folder"),
            "words are cursor-positioned"
        );
        assert!(is_trust_screen(SCREEN));
        assert_eq!(cursor_on(SCREEN), Some(Cursor::Decline));
    }

    #[test]
    fn a_redraw_after_the_cursor_moves_reads_as_the_cursor_on_the_other_option() {
        assert_eq!(cursor_on(AFTER_DOWN), Some(Cursor::Accept));
        // And the cursor of the whole picture is the latest one drawn, not the first.
        let both = [SCREEN, AFTER_DOWN].concat();
        assert_eq!(cursor_on(&both), Some(Cursor::Accept));
    }

    #[test]
    fn what_replaces_the_screen_is_not_the_screen() {
        assert!(!is_trust_screen(DISMISSED));
        assert!(!mentions_an_option(DISMISSED));
        assert_eq!(cursor_on(DISMISSED), None);
    }

    #[test]
    fn quoted_prose_is_not_the_screen() {
        assert!(!is_trust_screen(
            b"the option is called \"No, exit\" and nothing else"
        ));
        assert!(!is_trust_screen(b"Yes, I trust this folder"));
        assert!(!is_trust_screen(
            b"Claude Code asks: No, exit, or Yes, I trust this folder?"
        ));
        // Both options in a row, but no cursor in front of the first.
        assert!(!is_trust_screen(b"No, exit\r\nYes, I trust this folder"));
        // The redraw after a Down is not a first draw either.
        assert!(!is_trust_screen(AFTER_DOWN));
    }

    /// A transcript can quote both options next to a cursor glyph, as a model printing a prompt
    /// would; neither a numbered list nor words between them is the screen's first draw.
    #[test]
    fn a_transcript_quoting_both_options_beside_a_cursor_glyph_is_not_the_screen() {
        let numbered = "\u{276F} 1. No, exit\r\n  2. Yes, I trust this folder\r\n";
        assert!(!is_trust_screen(numbered.as_bytes()));
        let sentence = "\u{276F} No, exit or maybe Yes, I trust this folder";
        assert!(!is_trust_screen(sentence.as_bytes()));
        let reversed = "\u{276F} Yes, I trust this folder\r\n  No, exit\r\n";
        assert!(!is_trust_screen(reversed.as_bytes()));
    }

    #[test]
    fn the_watch_ends_after_too_much_output_or_too_long() {
        let state = TrustState::new(Agent::Claude);
        let mut sightings = state.take_sightings().expect("receiver");
        state.feed(&vec![b'.'; WATCH_BYTES + 1]);
        state.feed(SCREEN);
        assert!(sightings.try_recv().is_err(), "too late by output");
        assert!(!state.waiting());
        assert_eq!(
            state.window.lock().unwrap().window.capacity(),
            0,
            "the window is released"
        );

        let state = TrustState::with_limits(Agent::Claude, WATCH_BYTES, Duration::ZERO);
        let mut sightings = state.take_sightings().expect("receiver");
        std::thread::sleep(Duration::from_millis(5));
        state.feed(SCREEN);
        assert!(sightings.try_recv().is_err(), "too late by time");
    }

    #[test]
    fn a_hook_releases_the_window() {
        let state = TrustState::new(Agent::Claude);
        state.feed(b"some output");
        state.mark_hook_seen();
        assert_eq!(state.window.lock().unwrap().window.capacity(), 0);
    }

    #[test]
    fn the_event_is_named_claude_trust_prompt_on_the_wire() {
        let json = serde_json::to_value(Event::ClaudeTrustPrompt {
            session: "s".into(),
            project: "p".into(),
            path: "/x".into(),
        })
        .unwrap();
        assert_eq!(json["type"], "claude_trust_prompt");
        assert_eq!(json["path"], "/x");
    }

    /// The screen arrives in however many reads the kernel makes of it, cut anywhere — inside an
    /// escape sequence, inside a multi-byte character, between the two options.
    #[test]
    fn the_screen_is_sighted_once_however_it_is_cut_into_reads() {
        for size in [1, 2, 3, 7, 64, 200, SCREEN.len()] {
            let state = TrustState::new(Agent::Claude);
            let mut sightings = state.take_sightings().expect("receiver");
            for chunk in SCREEN.chunks(size) {
                state.feed(chunk);
            }
            // The redraws that follow, as a resize or a keypress would produce.
            state.feed(AFTER_DOWN);
            state.feed(SCREEN);

            assert!(sightings.try_recv().is_ok(), "sighted with reads of {size}");
            assert!(
                sightings.try_recv().is_err(),
                "sighted only once with reads of {size}"
            );
        }
    }

    #[test]
    fn nothing_is_sighted_before_the_screen_is_complete() {
        let state = TrustState::new(Agent::Claude);
        let mut sightings = state.take_sightings().expect("receiver");
        // Everything up to, and including, the first option.
        let cut = SCREEN
            .windows(4)
            .position(|window| window == b"exit")
            .expect("the first option");
        state.feed(&SCREEN[..cut]);
        assert!(sightings.try_recv().is_err());
        state.feed(&SCREEN[cut..]);
        assert!(sightings.try_recv().is_ok());
    }

    #[test]
    fn only_claude_code_is_watched_and_a_hook_ends_the_watch() {
        for agent in [Agent::Codex, Agent::Grok] {
            let state = TrustState::new(agent);
            let mut sightings = state.take_sightings().expect("receiver");
            state.feed(SCREEN);
            assert!(
                sightings.try_recv().is_err(),
                "{agent:?} has no such screen"
            );
            assert!(!state.claim_answer(), "{agent:?} is never answered");
        }

        let state = TrustState::new(Agent::Claude);
        let mut sightings = state.take_sightings().expect("receiver");
        state.feed(DISMISSED);
        state.mark_hook_seen();
        // Quoted by a session that is already running normally.
        state.feed(SCREEN);
        assert!(sightings.try_recv().is_err());
        assert!(!state.waiting());
    }

    #[test]
    fn a_session_is_answered_at_most_once_and_only_after_the_screen_was_sighted() {
        let state = TrustState::new(Agent::Claude);
        assert!(!state.claim_answer(), "nothing sighted yet");
        state.feed(SCREEN);
        assert!(state.waiting());
        assert!(state.claim_answer());
        assert!(!state.waiting());
        assert!(!state.claim_answer());
    }

    fn console() -> Console {
        Console {
            id: "console-1".to_string(),
            name: "Console".to_string(),
            workdir: "/tmp/console-1".to_string(),
            hub_agent: Agent::Claude,
            default_agent: Agent::Claude,
            claude_config_dir: None,
            codex_config_dir: None,
            grok_config_dir: None,
            created_at: 0,
        }
    }

    fn project(consent: bool) -> Project {
        Project {
            id: "project-1".to_string(),
            console_id: "console-1".to_string(),
            host_id: LOCAL_HOST_ID.to_string(),
            name: "Project".to_string(),
            path: "/work/project".to_string(),
            default_agent: None,
            source: ProjectSource::Local,
            remote_url: None,
            claude_trust_consent: consent,
        }
    }

    fn session(id: &str, agent: Agent, role: Role, project_id: Option<&str>) -> Session {
        Session {
            id: id.to_string(),
            agent,
            agent_session_id: None,
            console_id: "console-1".to_string(),
            project_id: project_id.map(str::to_string),
            host_id: LOCAL_HOST_ID.to_string(),
            role,
            origin: Origin::User,
            title: "Session".to_string(),
            status: SessionStatus::Idle,
            has_conversation: false,
            include_in_hub: false,
            config_dir: None,
            started_at: 0,
            ended_at: None,
        }
    }

    #[test]
    fn a_hub_is_always_answered_and_a_project_only_with_the_users_consent() {
        let hub = session("s", Agent::Claude, Role::Hub, None);
        let worker = session("s", Agent::Claude, Role::Worker, Some("project-1"));
        assert!(consented(&hub, None));
        assert!(consented(&worker, Some(&project(true))));
        assert!(!consented(&worker, Some(&project(false))));
        assert!(!consented(&worker, None));
    }

    /// A fresh directory for one test. Named without the thread id's parentheses, which the shell
    /// scripts below would have to quote.
    fn scratch(name: &str) -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "octoboardd-trust-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("temporary directory");
        dir
    }

    /// A stand-in for Claude Code on a real PTY: prints what the script says to and records every
    /// byte it is sent, so a test sees exactly what reached the terminal.
    fn fake_claude(id: &str, agent: Agent, script: &str) -> Arc<LiveSession> {
        let pty = native_pty_system()
            .openpty(PtySize {
                rows: 32,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("a PTY");
        let mut cmd = CommandBuilder::new("/bin/sh");
        // Raw first, so the fixtures reach the terminal as captured and the keys are read as sent.
        cmd.args(["-c", &format!("stty raw -echo; {script}")]);
        let child = pty.slave.spawn_command(cmd).expect("the stand-in");
        let pid = child.process_id().expect("a pid");
        let fd = pty.master.as_raw_fd().expect("a descriptor");
        crate::ptyio::set_nonblocking(fd).expect("non-blocking");
        let live = Arc::new(LiveSession::new(NewSession {
            id: id.to_string(),
            agent,
            pid,
            fd,
            master: pty.master,
            child,
            scratch_dir: None,
            resolves_approvals_itself: false,
        }));
        spawn_reader_thread(live.clone(), 8 * 1024);
        live
    }

    /// Every byte the stand-in has been sent so far. The file exists, empty, as soon as the stand-in
    /// starts waiting for input.
    fn sent(received: &Path) -> Vec<u8> {
        std::fs::read(received).unwrap_or_default()
    }

    /// The stand-in's whole conversation: the screen, a read of the Down, the redraw, a read of the
    /// Enter, the main screen, and then one more read, so that anything sent after the Enter shows
    /// up in what was received.
    fn full_script(received: &Path) -> String {
        format!(
            "cat {}; dd bs=1 count=3 2>/dev/null >> {r}; cat {}; dd bs=1 count=1 2>/dev/null >> {r}; \
             cat {}; dd bs=1 count=1 2>/dev/null >> {r}; sleep 30",
            fixture("claude_trust_screen.bin"),
            fixture("claude_trust_after_down.bin"),
            fixture("claude_trust_dismissed.bin"),
            r = quoted(received)
        )
    }

    #[test]
    fn answering_sends_a_down_and_then_an_enter_and_nothing_else() {
        let dir = scratch("answer");
        let received = dir.join("received");
        let live = fake_claude("s", Agent::Claude, &full_script(&received));
        let mut sightings = live.trust.take_sightings().expect("receiver");
        assert!(wait_for(Duration::from_secs(5), || sightings
            .try_recv()
            .is_ok()));

        answer(&live).expect("answered");

        // The stand-in is still reading, so a stray byte after the Enter would land in the file.
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(sent(&received), b"\x1b[B\r");
        live.terminate();
        std::fs::remove_dir_all(&dir).ok();
    }

    /// With the cursor already on "Yes", a Down would wrap to "No, exit" and the Enter after it
    /// would quit the session. Nothing is written at all.
    #[test]
    fn nothing_is_sent_when_the_cursor_is_not_on_the_first_option() {
        let dir = scratch("cursor-on-yes");
        let received = dir.join("received");
        let script = format!(
            "cat {}; cat {}; dd bs=1 count=1 2>/dev/null >> {}; sleep 30",
            fixture("claude_trust_screen.bin"),
            fixture("claude_trust_after_down.bin"),
            quoted(&received)
        );
        let live = fake_claude("s", Agent::Claude, &script);
        let mut sightings = live.trust.take_sightings().expect("receiver");
        assert!(wait_for(Duration::from_secs(5), || sightings
            .try_recv()
            .is_ok()));

        let err = answer(&live).expect_err("refused");
        assert!(err.to_string().contains("first option"), "{err}");

        assert!(sent(&received).is_empty(), "no key may have been sent");
        live.terminate();
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The Down goes in and the cursor does not move, so the Enter that would follow is withheld.
    #[test]
    fn enter_is_withheld_when_the_cursor_does_not_move() {
        let dir = scratch("no-move");
        let received = dir.join("received");
        let script = format!(
            "cat {}; dd bs=1 count=3 2>/dev/null >> {r}; dd bs=1 count=1 2>/dev/null >> {r}; \
             sleep 30",
            fixture("claude_trust_screen.bin"),
            r = quoted(&received)
        );
        let live = fake_claude("s", Agent::Claude, &script);
        let mut sightings = live.trust.take_sightings().expect("receiver");
        assert!(wait_for(Duration::from_secs(5), || sightings
            .try_recv()
            .is_ok()));

        let err = answer(&live).expect_err("not confirmed");
        assert!(err.to_string().contains("did not move"), "{err}");

        assert_eq!(sent(&received), b"\x1b[B");
        live.terminate();
        std::fs::remove_dir_all(&dir).ok();
    }

    /// An attached terminal reports focus and answers the agent's queries on its own, which says
    /// nothing about the cursor: the answer still completes.
    #[test]
    fn a_terminals_own_traffic_does_not_stop_the_answer() {
        let dir = scratch("terminal-traffic");
        let received = dir.join("received");
        let chatter: &[u8] = b"\x1b[I\x1b[O\x1b[?1;2c";
        // The stand-in's read of the Enter also takes the chatter that arrives before it.
        let script =
            full_script(&received).replacen("count=1", &format!("count={}", chatter.len() + 1), 1);
        let live = fake_claude("s", Agent::Claude, &script);
        let mut sightings = live.trust.take_sightings().expect("receiver");
        assert!(wait_for(Duration::from_secs(5), || sightings
            .try_recv()
            .is_ok()));

        let answering = {
            let live = live.clone();
            std::thread::spawn(move || answer(&live))
        };
        // After the Down, before the Enter: where a counted write would stop the answer.
        assert!(wait_for(Duration::from_secs(5), || sent(&received) == b"\x1b[B"));
        live.write_input(chatter).expect("the terminal's traffic");
        answering.join().unwrap().expect("answered");

        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(sent(&received), [&b"\x1b[B"[..], chatter, b"\r"].concat());
        live.terminate();
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The count of foreign writes and the refusal that rests on it: a terminal's own traffic is not
    /// counted, a typed byte is, and a key offered against a stale count is refused and never
    /// reaches the terminal.
    #[test]
    fn a_key_is_written_only_while_nothing_else_has_written() {
        let dir = scratch("if-untouched");
        let received = dir.join("received");
        let script = format!(
            "printf ready; dd bs=1 count=5 2>/dev/null >> {}; sleep 30",
            quoted(&received)
        );
        let live = fake_claude("s", Agent::Claude, &script);
        // Input written before the stand-in has set its terminal up would be thrown away.
        assert!(wait_for(Duration::from_secs(5), || live
            .recent_output(64)
            .starts_with(b"ready")));

        let before = live.input_writes();
        live.write_input(b"\x1b[I").expect("focus report");
        assert_eq!(
            live.input_writes(),
            before,
            "protocol traffic is not counted"
        );
        live.write_input(b"x").expect("typed");
        assert_eq!(live.input_writes(), before + 1);
        assert!(!live.write_input_if_untouched(before, b"Z").unwrap());
        assert!(live
            .write_input_if_untouched(live.input_writes(), b"\r")
            .unwrap());

        // The focus report, the typed byte and the key — not the key that was refused.
        assert!(wait_for(Duration::from_secs(5), || sent(&received).len() >= 5));
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(sent(&received), b"\x1b[Ix\r");
        live.terminate();
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Someone else types into the session after the Down went in — the user pressing a key in the
    /// terminal, a message leaving the queue — so the cursor may no longer be where the Enter would
    /// assume. The Enter is not sent.
    #[test]
    fn enter_is_withheld_when_something_else_writes_into_the_session_meanwhile() {
        let dir = scratch("foreign-write");
        let received = dir.join("received");
        // The stand-in stops after the redraw, so the stray byte is all it is sent next.
        let script = format!(
            "cat {}; dd bs=1 count=3 2>/dev/null >> {r}; cat {}; dd bs=1 count=1 2>/dev/null >> {r}; \
             sleep 30",
            fixture("claude_trust_screen.bin"),
            fixture("claude_trust_after_down.bin"),
            r = quoted(&received)
        );
        let live = fake_claude("s", Agent::Claude, &script);
        let mut sightings = live.trust.take_sightings().expect("receiver");
        assert!(wait_for(Duration::from_secs(5), || sightings
            .try_recv()
            .is_ok()));

        let answering = {
            let live = live.clone();
            std::thread::spawn(move || answer(&live))
        };
        assert!(wait_for(Duration::from_secs(5), || sent(&received) == b"\x1b[B"));
        live.write_input(b"x").expect("the foreign write");

        let err = answering.join().unwrap().expect_err("withheld");
        assert!(err.to_string().contains("something else wrote"), "{err}");
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(sent(&received), b"\x1b[Bx", "no Enter was sent");
        live.terminate();
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A session that never showed the screen, or another agent's, is never typed at.
    #[test]
    fn nothing_is_sent_to_a_session_that_never_showed_the_screen_or_to_another_agent() {
        let dir = scratch("never");
        let received = dir.join("received");
        let script = format!(
            "echo hello; dd bs=1 count=1 2>/dev/null >> {}; sleep 30",
            quoted(&received)
        );
        let claude = fake_claude("a", Agent::Claude, &script);
        assert!(answer(&claude).is_err());

        let codex = fake_claude("b", Agent::Codex, &script);
        codex.trust.feed(SCREEN);
        assert!(answer(&codex).is_err());

        std::thread::sleep(Duration::from_millis(300));
        assert!(sent(&received).is_empty());
        claude.terminate();
        codex.terminate();
        std::fs::remove_dir_all(&dir).ok();
    }

    fn app_state(name: &str) -> (Arc<AppState>, PathBuf) {
        let dir = scratch(name);
        let store = Store::open(&dir.join("octoboard.db")).expect("store");
        store.insert_console(&console()).expect("console");
        store.insert_project(&project(false)).expect("project");
        (
            Arc::new(AppState::new(store, 1234, "/opt/octoboardd".to_string())),
            dir,
        )
    }

    /// Starts a stand-in session of the given kind under a stored record and registers it, as a
    /// launch would; the caller decides whether it is supervised.
    fn start(
        state: &Arc<AppState>,
        dir: &Path,
        record: Session,
        script: &str,
    ) -> (Arc<LiveSession>, PathBuf) {
        let received = dir.join(format!("received-{}", record.id));
        state.store.insert_session(&record).expect("session");
        let live = fake_claude(
            &record.id,
            record.agent,
            &script.replace("%RECEIVED%", &received.to_string_lossy()),
        );
        state.register_live(live.clone());
        (live, received)
    }

    #[tokio::test]
    async fn a_hub_sessions_screen_is_answered_without_asking() {
        let (state, dir) = app_state("hub");
        let mut events = state.subscribe();
        let record = session("hub-1", Agent::Claude, Role::Hub, None);
        let (live, received) = start(&state, &dir, record, &full_script(Path::new("%RECEIVED%")));
        supervise(&state, &live);

        assert!(
            tokio::task::spawn_blocking({
                let received = received.clone();
                move || wait_for(Duration::from_secs(10), || sent(&received) == b"\x1b[B\r")
            })
            .await
            .unwrap(),
            "the keys must have been sent"
        );
        while let Ok(event) = events.try_recv() {
            assert!(
                !matches!(event, Event::ClaudeTrustPrompt { .. }),
                "a hub is not asked about"
            );
        }
        tokio::task::spawn_blocking(move || live.terminate())
            .await
            .unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn a_project_without_consent_is_asked_about_and_nothing_is_sent_until_confirmed() {
        let (state, dir) = app_state("ask");
        let mut events = state.subscribe();
        let record = session("worker-1", Agent::Claude, Role::Worker, Some("project-1"));
        let (live, received) = start(&state, &dir, record, &full_script(Path::new("%RECEIVED%")));
        supervise(&state, &live);

        let prompt = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Event::ClaudeTrustPrompt {
                    session,
                    project,
                    path,
                } = events.recv().await.expect("event")
                {
                    return (session, project, path);
                }
            }
        })
        .await
        .expect("the prompt");
        assert_eq!(
            prompt,
            (
                "worker-1".to_string(),
                "project-1".to_string(),
                "/work/project".to_string()
            )
        );
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert!(
            sent(&received).is_empty(),
            "nothing is sent before the user agrees"
        );

        confirm(&state, "worker-1", true).await.expect("confirmed");

        assert_eq!(sent(&received), b"\x1b[B\r");
        assert!(
            state
                .store
                .get_project("project-1")
                .unwrap()
                .unwrap()
                .claude_trust_consent
        );
        let mut upserted = false;
        while let Ok(event) = events.try_recv() {
            upserted |=
                matches!(event, Event::ProjectUpserted { project } if project.claude_trust_consent);
        }
        assert!(upserted, "clients are told the project is now consented");

        // Answered once: a second confirmation of the same screen is stale.
        assert!(confirm(&state, "worker-1", true).await.is_err());
        tokio::task::spawn_blocking(move || live.terminate())
            .await
            .unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn a_consented_project_is_answered_without_a_prompt() {
        let (state, dir) = app_state("consented");
        state
            .store
            .set_project_claude_trust_consent("project-1", true)
            .expect("consent");
        let mut events = state.subscribe();
        let record = session("worker-1", Agent::Claude, Role::Worker, Some("project-1"));
        let (live, received) = start(&state, &dir, record, &full_script(Path::new("%RECEIVED%")));
        supervise(&state, &live);

        assert!(tokio::task::spawn_blocking({
            let received = received.clone();
            move || wait_for(Duration::from_secs(10), || sent(&received) == b"\x1b[B\r")
        })
        .await
        .unwrap());
        while let Ok(event) = events.try_recv() {
            assert!(!matches!(event, Event::ClaudeTrustPrompt { .. }));
        }
        tokio::task::spawn_blocking(move || live.terminate())
            .await
            .unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A go-ahead that names a session which is not a running Claude Code project session at its
    /// screen records no consent and sends nothing.
    #[tokio::test]
    async fn a_confirmation_for_a_stale_or_foreign_session_changes_nothing() {
        let (state, dir) = app_state("refused");

        // Unknown.
        assert!(confirm(&state, "nobody", true).await.is_err());

        // Another agent's, even running and even holding the words.
        let codex = session("codex-1", Agent::Codex, Role::Worker, Some("project-1"));
        let (codex_live, codex_received) = start(&state, &dir, codex, "sleep 30");
        codex_live.trust.feed(SCREEN);
        let err = confirm(&state, "codex-1", true).await.expect_err("refused");
        assert!(err.to_string().contains("Claude Code"), "{err}");

        // Claude Code's, but not running.
        let gone = session("gone-1", Agent::Claude, Role::Worker, Some("project-1"));
        state.store.insert_session(&gone).expect("session");
        let err = confirm(&state, "gone-1", true).await.expect_err("refused");
        assert!(err.to_string().contains("not running"), "{err}");

        // Running, but its screen is not up.
        let quiet = session("quiet-1", Agent::Claude, Role::Worker, Some("project-1"));
        let (quiet_live, quiet_received) = start(&state, &dir, quiet, "sleep 30");
        let err = confirm(&state, "quiet-1", true).await.expect_err("refused");
        assert!(err.to_string().contains("any more"), "{err}");

        // A hub's screen is not the user's to confirm.
        let hub = session("hub-1", Agent::Claude, Role::Hub, None);
        let (hub_live, _) = start(&state, &dir, hub, "sleep 30");
        hub_live.trust.feed(SCREEN);
        let err = confirm(&state, "hub-1", true).await.expect_err("refused");
        assert!(err.to_string().contains("hub session"), "{err}");

        assert!(
            !state
                .store
                .get_project("project-1")
                .unwrap()
                .unwrap()
                .claude_trust_consent
        );
        assert!(sent(&codex_received).is_empty() && sent(&quiet_received).is_empty());
        tokio::task::spawn_blocking(move || {
            codex_live.terminate();
            quiet_live.terminate();
            hub_live.terminate();
        })
        .await
        .unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }

    fn consented_in_store(state: &AppState) -> bool {
        state
            .store
            .get_project("project-1")
            .unwrap()
            .unwrap()
            .claude_trust_consent
    }

    /// A go-ahead without `remember` answers the screen and records nothing.
    #[tokio::test]
    async fn without_remember_the_screen_is_answered_and_no_consent_is_recorded() {
        let (state, dir) = app_state("no-remember");
        let record = session("worker-1", Agent::Claude, Role::Worker, Some("project-1"));
        let (live, received) = start(&state, &dir, record, &full_script(Path::new("%RECEIVED%")));
        live.trust.feed(SCREEN);

        confirm(&state, "worker-1", false).await.expect("confirmed");
        assert_eq!(sent(&received), b"\x1b[B\r");
        assert!(!consented_in_store(&state));

        tokio::task::spawn_blocking(move || live.terminate())
            .await
            .unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Consent follows the answer: a screen that could not be answered leaves nothing recorded, and
    /// the user is told, whether or not the dialog they confirmed in is still open.
    #[tokio::test]
    async fn a_failed_answer_records_no_consent_and_tells_the_user() {
        let (state, dir) = app_state("failed-confirm");
        let mut events = state.subscribe();
        // The stand-in never moves its cursor.
        let script = format!(
            "cat {}; dd bs=1 count=3 2>/dev/null >> '%RECEIVED%'; sleep 30",
            fixture("claude_trust_screen.bin")
        );
        let record = session("worker-1", Agent::Claude, Role::Worker, Some("project-1"));
        let (live, _) = start(&state, &dir, record, &script);
        live.trust.feed(SCREEN);
        // The stand-in's own output is what the answer reads, so wait for it to be printed.
        let seen = live.clone();
        tokio::task::spawn_blocking(move || {
            wait_for(Duration::from_secs(5), || {
                is_trust_screen(&seen.recent_output(WINDOW))
            })
        })
        .await
        .unwrap();

        let err = confirm(&state, "worker-1", true).await.expect_err("failed");
        assert!(err.to_string().contains("did not move"), "{err}");
        assert!(!consented_in_store(&state));
        let mut told = false;
        while let Ok(event) = events.try_recv() {
            told |= matches!(event, Event::SessionNotice { session, message }
                if session == "worker-1" && message.contains("Answer it in the terminal"));
        }
        assert!(told, "a notice, for a dialog that may be closed");

        tokio::task::spawn_blocking(move || live.terminate())
            .await
            .unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }

    /// An automatic answer that fails is not silent either.
    #[tokio::test]
    async fn an_automatic_answer_that_fails_leaves_a_notice() {
        let (state, dir) = app_state("failed-auto");
        let mut events = state.subscribe();
        let script = format!(
            "cat {}; dd bs=1 count=3 2>/dev/null >> '%RECEIVED%'; sleep 30",
            fixture("claude_trust_screen.bin")
        );
        let record = session("hub-1", Agent::Claude, Role::Hub, None);
        let (live, _) = start(&state, &dir, record, &script);
        supervise(&state, &live);

        let notice = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                if let Event::SessionNotice { session, message } =
                    events.recv().await.expect("event")
                {
                    return (session, message);
                }
            }
        })
        .await
        .expect("the notice");
        assert_eq!(notice.0, "hub-1");
        assert!(
            notice.1.contains("Answer it in the terminal"),
            "{}",
            notice.1
        );

        tokio::task::spawn_blocking(move || live.terminate())
            .await
            .unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A client that was not there when the prompt was broadcast, or whose snapshot replaced it, is
    /// told again about exactly the screens still waiting for a go-ahead.
    #[tokio::test]
    async fn a_snapshot_is_followed_by_the_prompts_still_waiting() {
        let (state, dir) = app_state("replay");
        let waiting = session("waiting-1", Agent::Claude, Role::Worker, Some("project-1"));
        let (waiting_live, _) = start(&state, &dir, waiting, "sleep 30");
        let hub = session("hub-1", Agent::Claude, Role::Hub, None);
        let (hub_live, _) = start(&state, &dir, hub, "sleep 30");
        let quiet = session("quiet-1", Agent::Claude, Role::Worker, Some("project-1"));
        let (quiet_live, _) = start(&state, &dir, quiet, "sleep 30");
        let answered = session("answered-1", Agent::Claude, Role::Worker, Some("project-1"));
        let (answered_live, _) = start(&state, &dir, answered, "sleep 30");
        waiting_live.trust.feed(SCREEN);
        hub_live.trust.feed(SCREEN);
        answered_live.trust.feed(SCREEN);
        assert!(answered_live.trust.claim_answer());

        let prompts = pending_prompts(&state);
        assert_eq!(prompts.len(), 1, "{prompts:?}");
        assert!(
            matches!(&prompts[0], Event::ClaudeTrustPrompt { session, project, path }
            if session == "waiting-1" && project == "project-1" && path == "/work/project")
        );

        // Once the project has consented the screen is answered, not asked about.
        state
            .store
            .set_project_claude_trust_consent("project-1", true)
            .expect("consent");
        assert!(pending_prompts(&state).is_empty());

        tokio::task::spawn_blocking(move || {
            for live in [waiting_live, hub_live, quiet_live, answered_live] {
                live.terminate();
            }
        })
        .await
        .unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }
}
