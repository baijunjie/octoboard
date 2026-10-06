//! Answering Claude Code's workspace-trust screen for the user, once they have agreed to that.
//!
//! The first time Claude Code runs in a directory it stops on a screen asking whether the folder
//! is trusted, and waits for a person. A session the hub starts in a fresh project would sit there
//! unattended. Octoboard answers it by typing at the terminal — a Down and an Enter, because the
//! cursor starts on "No, exit" — and never by editing Claude Code's global config file, which
//! Claude Code rewrites constantly and which is the user's. The user's agreement comes first, in a
//! dialog of Octoboard's own, and is remembered per project (`Project::claude_trust_consent`) or
//! for a whole directory (`trusted_directories`): every project whose path lies under it, existing
//! or added later, is answered without asking.
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

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::hostfs::lexically_normalise;
use crate::paths;
use crate::protocol::{error_code, Agent, CodedError, Event, Project, Role, Session};
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
/// file Octoboard wrote there; a project session only with the user's consent, either for that
/// project or for a trusted directory its path lies under.
fn consented(session: &Session, project: Option<&Project>, trusted: &[String]) -> bool {
    session.role == Role::Hub
        || project.is_some_and(|project| {
            project.claude_trust_consent || under_a_trusted_directory(&project.path, trusted)
        })
}

/// A path as the trusted directories are kept and compared: lexically normalised — `.` dropped,
/// `..` folded into the component before it, trailing slashes gone — and absolute to be of any
/// use. Symlinks are deliberately not resolved: a directory is trusted by the path the user chose,
/// and both sides of every comparison are normalised the same way. The consequence is that a
/// symlink inside a trusted directory extends the trust to whatever it points at only if the
/// project's own path goes through it.
fn normalise_path(text: &str) -> PathBuf {
    lexically_normalise(Path::new(text))
}

/// Whether `path` is one of the trusted directories or lies below one. Compared component by
/// component, never as text: `/a/Project` does not cover `/a/Project2`.
fn under_a_trusted_directory(path: &str, trusted: &[String]) -> bool {
    let path = normalise_path(path);
    path.is_absolute()
        && trusted
            .iter()
            .any(|directory| path.starts_with(normalise_path(directory)))
}

/// The directory to trust for a project: its parent. Refused when that is the filesystem root, the
/// user's home directory, or a directory that contains the home directory — trusting one of those
/// would trust nearly everything the user has, which is not what a click in a dialog should grant.
///
/// The home directory and its ancestors, as written and as resolved, are compared with the parent
/// both as text and, where both exist, as the same directory on disk (device and inode), so that a
/// link to home, `/users/me` on a case-insensitive volume, or a home directory reached through a
/// link whose real location sits under the parent does not slip past the text comparison. A home
/// directory that is not absolute, or is the root, cannot be told apart from anything else, so
/// nothing is offered. A project path that is not absolute is refused as such. Every refusal
/// carries the same code: the dialog stays open, since nothing was answered.
fn trustable_parent(project_path: &str, home: &Path) -> Result<PathBuf> {
    let path = normalise_path(project_path);
    if !path.is_absolute() {
        return Err(refused(format!(
            "`{project_path}` is not an absolute path, so there is no folder above it to trust"
        )));
    }
    let parent = match path.parent() {
        Some(parent) if parent.parent().is_some() => parent.to_path_buf(),
        _ => return Err(too_broad(&path)),
    };
    let home = lexically_normalise(home);
    if !home.is_absolute() || home.parent().is_none() {
        return Err(refused(
            "the home directory could not be determined, so no folder can be checked against it"
                .to_string(),
        ));
    }
    if home.starts_with(&parent) {
        return Err(too_broad(&parent));
    }
    let resolved = std::fs::canonicalize(&home).ok();
    if [Some(&home), resolved.as_ref()]
        .into_iter()
        .flatten()
        .flat_map(|home| home.ancestors())
        .any(|ancestor| same_directory(ancestor, &parent))
    {
        return Err(too_broad(&parent));
    }
    Ok(parent)
}

/// A directory to trust that cannot be offered, with the reason. One code for all of them.
fn refused(message: String) -> anyhow::Error {
    CodedError::raised(error_code::TRUST_DIRECTORY_TOO_BROAD, message)
}

/// Whether two paths are the same directory on disk, however they are spelled: through a symlink,
/// or in another letter case on a volume that does not tell them apart. False when either is
/// unreadable.
fn same_directory(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

fn too_broad(directory: &Path) -> anyhow::Error {
    CodedError::raised(
        error_code::TRUST_DIRECTORY_TOO_BROAD,
        format!(
            "`{}` is too broad to trust as a whole: it is, or contains, the filesystem root or \
             your home directory. Trust this project on its own, or move it under a narrower folder \
             and trust that.",
            directory.display()
        ),
    )
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
    let trusted = state.store.trusted_directories()?;
    if consented(&session, project.as_ref(), &trusted) {
        answer_and_report(state, live).await;
        return Ok(());
    }
    let project = project.ok_or_else(|| anyhow!("the session's project is gone"))?;
    state.broadcast(prompt(&session.id, &project));
    Ok(())
}

/// The prompt for one session at its screen, with the directory its parent-directory button would
/// trust, if there is one to offer.
fn prompt(session_id: &str, project: &Project) -> Event {
    Event::ClaudeTrustPrompt {
        session: session_id.to_string(),
        project: project.id.clone(),
        path: project.path.clone(),
        trust_dir: trustable_parent(&project.path, &paths::home_dir())
            .ok()
            .map(|directory| directory.to_string_lossy().into_owned()),
    }
}

/// The live Claude Code project sessions that are at their trust screen with nobody answering it,
/// each with its record and its project.
fn waiting_project_sessions(state: &AppState) -> Vec<(Arc<LiveSession>, Session, Project)> {
    state
        .live_sessions()
        .into_iter()
        .filter(|live| live.trust.waiting())
        .filter_map(|live| {
            let session = state.store.get_session(&live.id).ok().flatten()?;
            let project = state
                .store
                .get_project(session.project_id.as_deref()?)
                .ok()
                .flatten()?;
            Some((live, session, project))
        })
        .collect()
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
/// concurrent go-ahead, or the user in the terminal — has answered it already. Nothing went wrong,
/// so nobody is told, and a client that asked is told with a code it shows nothing for.
fn not_waiting() -> anyhow::Error {
    CodedError::raised(
        error_code::CLAUDE_TRUST_NOT_WAITING,
        "this session is not waiting at Claude Code's trust screen any more",
    )
}

fn is_not_waiting(err: &anyhow::Error) -> bool {
    err.downcast_ref::<CodedError>()
        .is_some_and(|coded| coded.code == error_code::CLAUDE_TRUST_NOT_WAITING)
}

/// Logs a failed answer and tells the user, whoever asked for it: the screen is then still up and
/// theirs to answer. A lost claim is not a failure.
fn report_failure(state: &Arc<AppState>, session_id: &str, err: &anyhow::Error) {
    if is_not_waiting(err) {
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
/// consented, by its own consent or through a trusted directory. A prompt is broadcast once, so a
/// client that missed it — it was not connected, or a snapshot replaced what it had — is told here,
/// and one that already has it ignores the repeat.
pub fn pending_prompts(state: &AppState) -> Vec<Event> {
    let trusted = state.store.trusted_directories().unwrap_or_default();
    waiting_project_sessions(state)
        .into_iter()
        .filter(|(_, session, project)| !consented(session, Some(project), &trusted))
        .map(|(_, session, project)| prompt(&session.id, &project))
        .collect()
}

/// The user's go-ahead for one session's screen, from the dialog a `claude_trust_prompt` opened.
/// Refused unless that session is a running Claude Code project session still waiting at its
/// screen — a stale dialog, a session that has since gone, or a request naming something else
/// entirely changes nothing. Consent is recorded only once the screen has been answered — the
/// project's when `remember` asks for it, its parent directory's when `trust_parent_dir` does; a
/// failure to answer is reported to the user as a notice as well as to the caller, because the
/// dialog it came from may be closed by then. A parent directory that is too broad is refused
/// before anything is answered.
pub async fn confirm(
    state: &Arc<AppState>,
    session_id: &str,
    remember: bool,
    trust_parent_dir: bool,
) -> Result<()> {
    let session = state.session_record(session_id)?;
    if session.agent != Agent::Claude {
        bail!("only Claude Code sessions have a trust screen");
    }
    let Some(live) = state.live_session(session_id) else {
        bail!("this session is not running");
    };
    if !live.trust.waiting() {
        return Err(not_waiting());
    }
    let Some(project_id) = &session.project_id else {
        bail!("a hub session's trust screen is answered by Octoboard without asking");
    };
    let mut project = state
        .store
        .get_project(project_id)?
        .ok_or_else(|| anyhow!("unknown project {project_id}"))?;

    let directory = if trust_parent_dir {
        Some(trustable_parent(&project.path, &paths::home_dir())?)
    } else {
        None
    };

    if let Err(err) = answer_off_the_runtime(live).await {
        report_failure(state, session_id, &err);
        return Err(err);
    }

    if let Some(directory) = directory {
        add_trusted_directory(state, &directory)?;
    } else if remember && !project.claude_trust_consent {
        state
            .store
            .set_project_claude_trust_consent(&project.id, true)?;
        project.claude_trust_consent = true;
        state.broadcast(Event::ProjectUpserted { project });
    }
    Ok(())
}

/// Trusts a directory and everything below it, tells every client, and answers the screens already
/// waiting in projects under it. Each project's own consent is untouched. Needs the runtime: the
/// answers go off on tasks of their own.
fn add_trusted_directory(state: &Arc<AppState>, directory: &Path) -> Result<()> {
    let directory = directory.to_string_lossy().into_owned();
    if !state.store.add_trusted_directory(&directory)? {
        return Ok(());
    }
    state.broadcast(Event::TrustedDirectoriesUpdated {
        trusted_directories: state.store.trusted_directories()?,
    });
    let covered = [directory];
    for (live, _, project) in waiting_project_sessions(state) {
        if under_a_trusted_directory(&project.path, &covered) {
            let state = state.clone();
            tokio::spawn(async move { answer_and_report(&state, &live).await });
        }
    }
    Ok(())
}

/// Stops trusting a directory, and tells every client. Projects' own consents stay, and so do the
/// sessions already running.
pub fn remove_trusted_directory(state: &Arc<AppState>, path: &str) -> Result<()> {
    let path = normalise_path(path).to_string_lossy().into_owned();
    if state.store.remove_trusted_directory(&path)? {
        state.broadcast(Event::TrustedDirectoriesUpdated {
            trusted_directories: state.store.trusted_directories()?,
        });
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
        return Err(not_waiting());
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
            path: "/x/y".into(),
            trust_dir: Some("/x".into()),
        })
        .unwrap();
        assert_eq!(json["type"], "claude_trust_prompt");
        assert_eq!(json["path"], "/x/y");
        assert_eq!(json["trust_dir"], "/x");
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
        let none: &[String] = &[];
        let work = ["/work".to_string()];
        assert!(consented(&hub, None, none));
        assert!(consented(&worker, Some(&project(true)), none));
        assert!(!consented(&worker, Some(&project(false)), none));
        assert!(!consented(&worker, None, none));
        // A trusted directory covers the project under it, one with no consent of its own included.
        assert!(consented(&worker, Some(&project(false)), &work));
        assert!(!consented(
            &worker,
            Some(&project(false)),
            &["/elsewhere".to_string()]
        ));
        assert!(!consented(&worker, None, &work));
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
                    ..
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

        confirm(&state, "worker-1", true, false)
            .await
            .expect("confirmed");

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
        assert!(confirm(&state, "worker-1", true, false).await.is_err());
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
        assert!(confirm(&state, "nobody", true, false).await.is_err());

        // Another agent's, even running and even holding the words.
        let codex = session("codex-1", Agent::Codex, Role::Worker, Some("project-1"));
        let (codex_live, codex_received) = start(&state, &dir, codex, "sleep 30");
        codex_live.trust.feed(SCREEN);
        let err = confirm(&state, "codex-1", true, false)
            .await
            .expect_err("refused");
        assert!(err.to_string().contains("Claude Code"), "{err}");

        // Claude Code's, but not running.
        let gone = session("gone-1", Agent::Claude, Role::Worker, Some("project-1"));
        state.store.insert_session(&gone).expect("session");
        let err = confirm(&state, "gone-1", true, false)
            .await
            .expect_err("refused");
        assert!(err.to_string().contains("not running"), "{err}");

        // Running, but its screen is not up.
        let quiet = session("quiet-1", Agent::Claude, Role::Worker, Some("project-1"));
        let (quiet_live, quiet_received) = start(&state, &dir, quiet, "sleep 30");
        let err = confirm(&state, "quiet-1", true, false)
            .await
            .expect_err("refused");
        assert!(err.to_string().contains("any more"), "{err}");

        // A hub's screen is not the user's to confirm.
        let hub = session("hub-1", Agent::Claude, Role::Hub, None);
        let (hub_live, _) = start(&state, &dir, hub, "sleep 30");
        hub_live.trust.feed(SCREEN);
        let err = confirm(&state, "hub-1", true, false)
            .await
            .expect_err("refused");
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

        confirm(&state, "worker-1", false, false)
            .await
            .expect("confirmed");
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

        let err = confirm(&state, "worker-1", true, false)
            .await
            .expect_err("failed");
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
            matches!(&prompts[0], Event::ClaudeTrustPrompt { session, project, path, trust_dir }
            if session == "waiting-1" && project == "project-1" && path == "/work/project"
                && trust_dir.as_deref() == Some("/work"))
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

    fn trusted_in_store(state: &AppState) -> Vec<String> {
        state.store.trusted_directories().unwrap()
    }

    /// Paths are compared component by component, after a lexical clean-up, and a trusted directory
    /// covers itself and everything below it.
    #[test]
    fn a_trusted_directory_covers_itself_and_what_is_below_it_and_nothing_else() {
        let work = ["/work".to_string()];
        assert!(under_a_trusted_directory("/work", &work), "itself");
        assert!(under_a_trusted_directory("/work/project", &work), "a child");
        assert!(
            under_a_trusted_directory("/work/a/b/c", &work),
            "a descendant"
        );
        assert!(
            under_a_trusted_directory("/work/project/", &work),
            "a trailing slash"
        );
        assert!(
            under_a_trusted_directory("/work/./a/../project", &work),
            "lexical clean-up"
        );
        assert!(under_a_trusted_directory(
            "/work/project",
            &["/work/".to_string()]
        ));
        // Not a string prefix.
        assert!(!under_a_trusted_directory("/workspace/project", &work));
        assert!(!under_a_trusted_directory("/work2", &work));
        assert!(
            !under_a_trusted_directory("/Work/project", &work),
            "case differs"
        );
        // Not above it, not beside it, not escaping it.
        assert!(!under_a_trusted_directory("/", &work));
        assert!(!under_a_trusted_directory("/work/../other", &work));
        assert!(
            !under_a_trusted_directory("work/project", &work),
            "relative"
        );
        assert!(!under_a_trusted_directory("/work/project", &[]));
    }

    #[test]
    fn the_parent_to_trust_is_never_the_root_the_home_directory_or_above_it() {
        let home = Path::new("/Users/me");
        assert_eq!(
            trustable_parent("/Users/me/Coding/app", home).unwrap(),
            Path::new("/Users/me/Coding")
        );
        assert_eq!(
            trustable_parent("/Users/me/Coding/app/", home).unwrap(),
            Path::new("/Users/me/Coding"),
            "a trailing slash"
        );
        // Directly in the home directory, in the root, and the root itself.
        for project in ["/Users/me/app", "/app", "/", "/Users/app"] {
            let err = trustable_parent(project, home).expect_err(project);
            let coded = err.downcast_ref::<CodedError>().expect("a coded error");
            assert_eq!(
                coded.code,
                error_code::TRUST_DIRECTORY_TOO_BROAD,
                "{project}"
            );
            assert!(err.to_string().contains("too broad"), "{err}");
        }
        assert!(trustable_parent("relative/app", home).is_err());
    }

    /// "Trust parent folder" answers this screen and, once that worked, records the
    /// parent directory — and leaves the project's own consent unset.
    #[tokio::test]
    async fn confirming_for_the_parent_directory_records_it_after_the_answer() {
        let (state, dir) = app_state("confirm-parent");
        let mut events = state.subscribe();
        let record = session("worker-1", Agent::Claude, Role::Worker, Some("project-1"));
        let (live, received) = start(&state, &dir, record, &full_script(Path::new("%RECEIVED%")));
        live.trust.feed(SCREEN);

        confirm(&state, "worker-1", false, true)
            .await
            .expect("confirmed");
        assert_eq!(sent(&received), b"\x1b[B\r");
        assert_eq!(trusted_in_store(&state), ["/work"]);
        assert!(
            !consented_in_store(&state),
            "the project's own consent is separate"
        );
        let mut told = false;
        while let Ok(event) = events.try_recv() {
            told |= matches!(event, Event::TrustedDirectoriesUpdated { trusted_directories }
                if trusted_directories == ["/work"]);
        }
        assert!(told, "clients are told");

        tokio::task::spawn_blocking(move || live.terminate())
            .await
            .unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn nothing_is_trusted_when_the_answer_fails_or_is_refused() {
        let (state, dir) = app_state("confirm-parent-refused");
        // The stand-in never moves its cursor.
        let script = format!(
            "cat {}; dd bs=1 count=3 2>/dev/null >> '%RECEIVED%'; sleep 30",
            fixture("claude_trust_screen.bin")
        );
        let record = session("worker-1", Agent::Claude, Role::Worker, Some("project-1"));
        let (live, _) = start(&state, &dir, record, &script);
        live.trust.feed(SCREEN);
        let seen = live.clone();
        tokio::task::spawn_blocking(move || {
            wait_for(Duration::from_secs(5), || {
                is_trust_screen(&seen.recent_output(WINDOW))
            })
        })
        .await
        .unwrap();
        assert!(confirm(&state, "worker-1", false, true).await.is_err());
        assert!(trusted_in_store(&state).is_empty());

        // The same refusals as for one project: unknown, another agent's, not running, a hub's.
        let codex = session("codex-1", Agent::Codex, Role::Worker, Some("project-1"));
        let (codex_live, _) = start(&state, &dir, codex, "sleep 30");
        let hub = session("hub-1", Agent::Claude, Role::Hub, None);
        let (hub_live, _) = start(&state, &dir, hub, "sleep 30");
        hub_live.trust.feed(SCREEN);
        let gone = session("gone-1", Agent::Claude, Role::Worker, Some("project-1"));
        state.store.insert_session(&gone).expect("session");
        for id in ["nobody", "codex-1", "hub-1", "gone-1"] {
            assert!(confirm(&state, id, true, true).await.is_err(), "{id}");
        }
        assert!(trusted_in_store(&state).is_empty());

        tokio::task::spawn_blocking(move || {
            live.terminate();
            codex_live.terminate();
            hub_live.terminate();
        })
        .await
        .unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A project directly under the home directory has nothing narrower to trust, so the request is
    /// refused before anything is answered or sent.
    #[tokio::test]
    async fn a_parent_that_is_the_home_directory_is_refused_before_the_screen_is_answered() {
        let (state, dir) = app_state("confirm-home");
        let home = paths::home_dir();
        let mut broad = project(false);
        broad.id = "broad".to_string();
        broad.path = home.join("app").to_string_lossy().into_owned();
        state.store.insert_project(&broad).expect("project");
        let record = session("worker-1", Agent::Claude, Role::Worker, Some("broad"));
        let (live, received) = start(&state, &dir, record, &full_script(Path::new("%RECEIVED%")));
        live.trust.feed(SCREEN);

        let err = confirm(&state, "worker-1", false, true)
            .await
            .expect_err("refused");
        assert_eq!(
            err.downcast_ref::<CodedError>().map(|coded| coded.code),
            Some(error_code::TRUST_DIRECTORY_TOO_BROAD)
        );
        assert!(trusted_in_store(&state).is_empty());
        assert!(live.trust.waiting(), "the screen is still waiting");
        assert!(sent(&received).is_empty());

        tokio::task::spawn_blocking(move || live.terminate())
            .await
            .unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn a_directory_can_be_removed_again_and_each_projects_own_consent_stays() {
        let (state, dir) = app_state("remove");
        state
            .store
            .set_project_claude_trust_consent("project-1", true)
            .expect("consent");
        state.store.add_trusted_directory("/work").expect("trusted");
        let mut events = state.subscribe();

        remove_trusted_directory(&state, "/work/").expect("removed");
        remove_trusted_directory(&state, "/work").expect("a repeat");
        assert!(trusted_in_store(&state).is_empty());
        assert!(consented_in_store(&state));

        let mut updates = Vec::new();
        while let Ok(event) = events.try_recv() {
            if let Event::TrustedDirectoriesUpdated {
                trusted_directories,
            } = event
            {
                updates.push(trusted_directories);
            }
        }
        assert_eq!(updates, [Vec::<String>::new()], "a repeat is not announced");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A screen of a project under a directory is answered without a prompt, and trusting the
    /// directory answers the screens already waiting under it — and only those.
    #[tokio::test]
    async fn trusting_a_directory_answers_the_screens_waiting_under_it_and_no_others() {
        let (state, dir) = app_state("fan-out");
        let mut elsewhere = project(false);
        elsewhere.id = "project-2".to_string();
        elsewhere.path = "/other/project".to_string();
        state.store.insert_project(&elsewhere).expect("project");
        let mut events = state.subscribe();

        let under = session("under-1", Agent::Claude, Role::Worker, Some("project-1"));
        let (under_live, under_received) =
            start(&state, &dir, under, &full_script(Path::new("%RECEIVED%")));
        supervise(&state, &under_live);
        let beside = session("beside-1", Agent::Claude, Role::Worker, Some("project-2"));
        let (beside_live, beside_received) =
            start(&state, &dir, beside, &full_script(Path::new("%RECEIVED%")));
        supervise(&state, &beside_live);
        let mut asked = 0;
        tokio::time::timeout(Duration::from_secs(10), async {
            while asked < 2 {
                if let Event::ClaudeTrustPrompt { .. } = events.recv().await.expect("event") {
                    asked += 1;
                }
            }
        })
        .await
        .expect("both are asked about");

        add_trusted_directory(&state, Path::new("/work")).expect("trusted");
        assert!(
            tokio::task::spawn_blocking({
                let received = under_received.clone();
                move || wait_for(Duration::from_secs(10), || sent(&received) == b"\x1b[B\r")
            })
            .await
            .unwrap(),
            "the screen under the directory is answered"
        );
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert!(
            sent(&beside_received).is_empty(),
            "the one beside it is not"
        );
        assert_eq!(pending_prompts(&state).len(), 1, "and is still asked about");

        // A screen that comes up later under the directory is answered with no prompt.
        let later = session("later-1", Agent::Claude, Role::Worker, Some("project-1"));
        let (later_live, later_received) =
            start(&state, &dir, later, &full_script(Path::new("%RECEIVED%")));
        supervise(&state, &later_live);
        assert!(tokio::task::spawn_blocking({
            let received = later_received.clone();
            move || wait_for(Duration::from_secs(10), || sent(&received) == b"\x1b[B\r")
        })
        .await
        .unwrap());

        tokio::task::spawn_blocking(move || {
            under_live.terminate();
            beside_live.terminate();
            later_live.terminate();
        })
        .await
        .unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }

    fn is_too_broad(result: &Result<PathBuf>) -> bool {
        result.as_ref().is_err_and(|err| {
            err.downcast_ref::<CodedError>()
                .is_some_and(|coded| coded.code == error_code::TRUST_DIRECTORY_TOO_BROAD)
        })
    }

    /// The guard is not fooled by a spelling of the home directory the text comparison cannot see:
    /// a symlink to it, a symlink to something containing it, or — on a volume that ignores letter
    /// case, which a temporary directory here may or may not be on — the same name in another case.
    #[test]
    fn the_guard_sees_through_symlinks_and_letter_case() {
        let dir = scratch("guard");
        let home = dir.join("Home");
        std::fs::create_dir_all(home.join("app")).expect("home");
        std::os::unix::fs::symlink(&home, dir.join("link-to-home")).expect("link");
        std::os::unix::fs::symlink(&dir, dir.join("link-to-above")).expect("link");
        let under = |parent: PathBuf| parent.join("app").to_string_lossy().into_owned();

        // As written, for the baseline.
        assert!(is_too_broad(&trustable_parent(&under(home.clone()), &home)));
        // A link to the home directory, and a link to a directory above it.
        assert!(is_too_broad(&trustable_parent(
            &under(dir.join("link-to-home")),
            &home
        )));
        assert!(is_too_broad(&trustable_parent(
            &under(dir.join("link-to-above")),
            &home
        )));
        // A sibling that is neither is still offered.
        std::fs::create_dir_all(dir.join("Elsewhere")).expect("sibling");
        assert!(trustable_parent(&under(dir.join("Elsewhere")), &home).is_ok());

        // Another letter case. On a volume that ignores case it is the home directory and is
        // refused; on one that does not it is a different folder, which is offered.
        let flipped = dir.join("hOME");
        if flipped.exists() {
            assert!(is_too_broad(&trustable_parent(&under(flipped), &home)));
        } else {
            std::fs::create_dir_all(flipped.join("app")).expect("a different folder");
            assert!(trustable_parent(&under(flipped), &home).is_ok());
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The home directory may itself be given through a link whose real location lies under the
    /// parent being offered: `/home -> /data/home`, `HOME=/home/me`, a project at `/data/app`.
    #[test]
    fn the_guard_sees_a_home_directory_reached_through_a_link() {
        let dir = scratch("guard-home-link");
        std::fs::create_dir_all(dir.join("data/home/me")).expect("real home");
        std::fs::create_dir_all(dir.join("data/app")).expect("project");
        std::os::unix::fs::symlink(dir.join("data/home"), dir.join("home")).expect("link");
        let home = dir.join("home/me");

        let project = dir.join("data/app").to_string_lossy().into_owned();
        assert!(
            is_too_broad(&trustable_parent(&project, &home)),
            "`data` contains the real home"
        );
        // A folder that does not contain it is still offered.
        std::fs::create_dir_all(dir.join("other/app")).expect("other");
        let other = dir.join("other/app").to_string_lossy().into_owned();
        assert!(trustable_parent(&other, &home).is_ok());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// With no usable home directory nothing can be checked against it, so nothing is offered —
    /// and a project path that is not absolute says so rather than blaming the root or home.
    #[test]
    fn an_odd_home_directory_or_a_relative_project_path_offers_nothing() {
        for home in ["", "relative/home", "/"] {
            let err = trustable_parent("/work/project", Path::new(home)).expect_err(home);
            assert!(err.to_string().contains("home directory"), "{err}");
            assert!(is_too_broad(&Err::<PathBuf, _>(err)), "{home:?}");
        }
        let err = trustable_parent("work/project", Path::new("/Users/me")).expect_err("relative");
        assert!(err.to_string().contains("not an absolute path"), "{err}");
        assert!(!err.to_string().contains("too broad"), "{err}");
        // Still a refusal the dialog stays open for.
        assert!(is_too_broad(&Err::<PathBuf, _>(err)));
    }

    #[test]
    fn dot_dot_in_a_project_path_is_folded_before_anything_is_compared() {
        let home = Path::new("/Users/me");
        assert_eq!(
            trustable_parent("/work/other/../project", home).unwrap(),
            Path::new("/work")
        );
        let work = ["/work".to_string()];
        assert!(under_a_trusted_directory("/work/other/../project", &work));
        // Climbing out of the directory is not under it.
        assert!(!under_a_trusted_directory("/work/../etc/project", &work));
        assert!(!under_a_trusted_directory("/work/../../project", &work));
    }

    /// The prompt carries the directory its button would trust, and none when that would be too
    /// broad.
    #[test]
    fn a_prompt_names_the_directory_it_would_trust_unless_that_is_too_broad() {
        let mut inside = project(false);
        inside.path = "/work/project".to_string();
        assert!(matches!(prompt("s", &inside),
            Event::ClaudeTrustPrompt { trust_dir, .. } if trust_dir.as_deref() == Some("/work")));
        let mut at_home = project(false);
        at_home.path = paths::home_dir().join("app").to_string_lossy().into_owned();
        assert!(matches!(
            prompt("s", &at_home),
            Event::ClaudeTrustPrompt {
                trust_dir: None,
                ..
            }
        ));
    }

    /// The replay after a snapshot leaves out a screen whose project sits under a trusted
    /// directory, as it does one whose project has consented.
    #[tokio::test]
    async fn the_replay_leaves_out_a_screen_under_a_trusted_directory() {
        let (state, dir) = app_state("replay-trusted");
        let record = session("waiting-1", Agent::Claude, Role::Worker, Some("project-1"));
        let (live, _) = start(&state, &dir, record, "sleep 30");
        live.trust.feed(SCREEN);
        assert_eq!(pending_prompts(&state).len(), 1);

        state.store.add_trusted_directory("/work").expect("trusted");
        assert!(
            live.trust.waiting(),
            "still waiting; only the question is moot"
        );
        assert!(pending_prompts(&state).is_empty());

        tokio::task::spawn_blocking(move || live.terminate())
            .await
            .unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Trusting the directory from one session's dialog answers another session's screen waiting
    /// under it; a repeat neither announces nor answers anything again; a hub and another agent's
    /// session are left to their own rules.
    #[tokio::test]
    async fn one_sessions_go_ahead_answers_the_others_waiting_under_the_directory() {
        let (state, dir) = app_state("fan-out-end-to-end");
        let mut events = state.subscribe();
        let a = session("a-1", Agent::Claude, Role::Worker, Some("project-1"));
        let (a_live, a_received) = start(&state, &dir, a, &full_script(Path::new("%RECEIVED%")));
        a_live.trust.feed(SCREEN);
        let b = session("b-1", Agent::Claude, Role::Worker, Some("project-1"));
        let (b_live, b_received) = start(&state, &dir, b, &full_script(Path::new("%RECEIVED%")));
        b_live.trust.feed(SCREEN);
        let hub = session("hub-1", Agent::Claude, Role::Hub, None);
        let (hub_live, hub_received) =
            start(&state, &dir, hub, &full_script(Path::new("%RECEIVED%")));
        hub_live.trust.feed(SCREEN);
        let codex = session("codex-1", Agent::Codex, Role::Worker, Some("project-1"));
        let (codex_live, codex_received) =
            start(&state, &dir, codex, &full_script(Path::new("%RECEIVED%")));
        codex_live.trust.feed(SCREEN);

        confirm(&state, "a-1", false, true)
            .await
            .expect("confirmed");
        assert_eq!(sent(&a_received), b"\x1b[B\r");
        assert!(
            tokio::task::spawn_blocking({
                let received = b_received.clone();
                move || wait_for(Duration::from_secs(10), || sent(&received) == b"\x1b[B\r")
            })
            .await
            .unwrap(),
            "the other screen under the directory is answered"
        );
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert!(sent(&hub_received).is_empty(), "a hub is not the fan-out's");
        assert!(sent(&codex_received).is_empty(), "nor is another agent's");

        // A repeat: nothing announced, and nothing more is answered.
        let c = session("c-1", Agent::Claude, Role::Worker, Some("project-1"));
        let (c_live, c_received) = start(&state, &dir, c, &full_script(Path::new("%RECEIVED%")));
        c_live.trust.feed(SCREEN);
        add_trusted_directory(&state, Path::new("/work")).expect("a repeat");
        tokio::time::sleep(Duration::from_millis(800)).await;
        assert!(sent(&c_received).is_empty());
        let mut announced = 0;
        while let Ok(event) = events.try_recv() {
            if matches!(event, Event::TrustedDirectoriesUpdated { .. }) {
                announced += 1;
            }
        }
        assert_eq!(announced, 1);

        tokio::task::spawn_blocking(move || {
            for live in [a_live, b_live, hub_live, codex_live, c_live] {
                live.terminate();
            }
        })
        .await
        .unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }
}
