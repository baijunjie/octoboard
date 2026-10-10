//! Pressing an agent's own folder-trust confirmation for the user, once they have agreed to that.
//!
//! Each of the three agents stops on a confirmation of its own the first time it runs in a folder
//! it has not been told to trust, and waits for a person: Claude Code's workspace-trust screen,
//! Codex's "Trust this folder?" (only inside a git repository) and Grok Build's "Do you trust the
//! contents of this directory?" (only where the folder holds content its trust gates, such as an
//! `AGENTS.md`). A session the console session starts in a fresh project would sit there
//! unattended. Octoboard presses the confirmation by typing at the terminal, and so the agent
//! records the trust in its own configuration, which is what makes a later launch of that agent
//! trusted. Octoboard never makes that trust decision itself and never writes it into a
//! per-session copy ahead of the press; the one write it makes is carrying the entry Grok wrote
//! over to the user's own store, unchanged ([`CarriedTrust`]), after Octoboard's press and after
//! the person's own answer in the terminal alike ([`on_hook`]), so that Grok, like the other two,
//! does not ask again. Only Octoboard's own successful press records a permission of Octoboard's.
//!
//! The user's agreement comes first, in a dialog of Octoboard's own, and is one permission shared
//! by every agent, in two forms: per project (`Project::trust_consent`), or for a whole directory
//! (`trusted_directories`), which covers every project whose path lies under it, existing or added
//! later. Either form lets a later confirmation of any agent be pressed without asking.
//!
//! **Detection.** Every session's output is watched by a [`TrustState`], fed from the PTY reader
//! thread whether or not a client is attached, against its own agent's [`Screen`]. The screen is
//! recognised by how it is first drawn after the output has been reduced to bare text
//! ([`normalise`]): the agents do not print a space between words, they move the cursor, so the
//! raw bytes never contain the words a person reads. A sighting is signalled once per session.
//! That is enough for "once per occurrence" because the confirmation cannot come back within one
//! process. Claude Code and Grok run no hook before it is answered and Grok runs `SessionStart`
//! right after, so the first hook callback retires the watch; Codex runs none around it, so for
//! Codex and Grok anything written into the session's input since the sighting — the person
//! answering in the terminal — also means the screen is no longer waiting: its prompt is not put
//! to a client again and a go-ahead for it is refused as stale. Every watch is also bounded: it
//! ends after [`WATCH_BYTES`] of output or [`WATCH_SECONDS`] seconds, the confirmation being among
//! the first things each agent prints. A transcript that quotes a screen is not taken for it
//! unless it reproduces the first-draw pattern inside that window before any hook has run; and
//! whatever is sighted, keys are sent only if the checks below pass.
//!
//! **Octoboard's own messages are held** for as long as a screen may be up
//! ([`TrustState::holds_writes`]), since a message's trailing Enter would otherwise answer it —
//! accept Codex's or Grok's, decline Claude Code's — on nobody's say-so:
//!
//! - Claude Code and Grok run a hook as they start, once they are past the screen, so their
//!   messages are held until that first hook and released by it ([`on_hook`]); another startup
//!   modal can follow Claude Code's screen, and only the hook says the session is ready. A session
//!   whose hooks never arrive holds its messages for as long as it runs.
//! - Codex runs none until its first prompt, so its messages are held while the watch is still
//!   looking and, once its screen is sighted, until Octoboard's press of it succeeds, a hook
//!   arrives, or the person's own answer in the terminal is seen to take the screen away. The
//!   press releases them itself; the end of the watch with nothing sighted and the person's answer
//!   are watched for by [`supervise`] (`release_when_the_screen_goes`).
//!
//! So none of Octoboard's own writes can be what ends a wait, and the input counted since a
//! sighting is the person's alone.
//!
//! **What happens next** is decided by [`supervise`]: a console session's working directory is
//! Octoboard's own console directory, so it is pressed at once and nothing is recorded; a project
//! session is pressed at once when the project has the user's permission, and otherwise a
//! `trust_prompt` is broadcast and the screen is left alone until `confirm_trust` comes back. With
//! no client connected, or after "Not now", nothing is sent and the screen waits for the person to
//! answer it in the terminal.
//!
//! **Sending keys is the dangerous part**, so [`answer`] is the only place that does it, takes no
//! text, and checks before each key that the screen was sighted and has not been answered, and
//! that nobody else has written into the session's input since it looked
//! ([`LiveSession::input_writes`]): the user pressing a key in the terminal, or a queued message,
//! would change what the key lands on. The rest depends on the agent:
//!
//! - Claude Code's cursor starts on "No, exit", so the latest output must still show the screen
//!   with the cursor there — a Down from the other option would wrap round to it, and an Enter
//!   there exits. After the Down the output must show the cursor on "Yes, I trust this folder"
//!   and have been quiet for a moment; only then is Enter sent, after a last look at both.
//! - Codex's cursor starts on "1. Trust and continue", so the latest output must still show the
//!   screen with the cursor there, and still after a quiet moment, before the Enter, and nothing
//!   may have been written into its input since the screen was sighted.
//! - Grok has no cursor, and the logo it animates while it waits keeps writing, so neither the
//!   first drawing staying in the recent output nor a quiet terminal can be waited for. It is
//!   pressed with `y` while nothing has shown the screen gone — no hook has run, the process is
//!   still there (a decline exits it), and nothing has been written into its input since the
//!   screen was sighted — and the press counts once Grok has recorded the trust.
//!
//! Whatever cannot be confirmed ends the attempt with the keys not sent, and the screen waits for
//! the person to answer it. Each session is answered at most once, so a failed attempt is not
//! retried behind the person's back.

use std::io::Write;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::hostfs::lexically_normalise;
use crate::paths;
use crate::protocol::{
    error_code, notice_code, trust_reason, Agent, CodedError, Event, Notice, Project, Role, Session,
};
use crate::session::LiveSession;
use crate::state::AppState;

mod carry;
mod press;
mod screen;
#[cfg(test)]
mod tests;

pub use carry::CarriedTrust;
use carry::*;
pub(crate) use press::wait_for;
use press::*;
pub use screen::TrustState;
use screen::*;

/// How much output after a sighting the watch for the person's answer reads at most. Codex prints
/// little at its screen; past this, its next screen is left for a hook to announce.
const WATCHED_ANSWER_BYTES: u64 = 128 * 1024;

/// How often a session's held messages are looked at while its watch runs.
const RELEASE_POLL: Duration = Duration::from_millis(250);

/// Whether Octoboard may answer this session's screen without asking: a console session always
/// may, because its working directory is the console's own, which holds nothing but the
/// instruction file Octoboard wrote there; a project session only with the user's permission,
/// either for that project or for a trusted directory its path lies under. The permission is
/// shared by every agent, so it does not matter which one is asking.
fn consented(session: &Session, project: Option<&Project>, trusted: &[String]) -> bool {
    session.role == Role::Console
        || project.is_some_and(|project| {
            project.trust_consent || under_a_trusted_directory(&project.path, trusted)
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
/// carries one of the three codes that leave the dialog open, since nothing was answered.
fn trustable_parent(project_path: &str, home: &Path) -> Result<PathBuf> {
    let path = normalise_path(project_path);
    if !path.is_absolute() {
        return Err(CodedError::raised(
            error_code::TRUST_PATH_NOT_ABSOLUTE,
            format!(
                "`{project_path}` is not an absolute path, so there is no folder above it to trust"
            ),
            &[("path", project_path)],
        ));
    }
    let parent = match path.parent() {
        Some(parent) if parent.parent().is_some() => parent.to_path_buf(),
        _ => return Err(too_broad(&path)),
    };
    let Some(home) = paths::known_home(home) else {
        return Err(CodedError::raised(
            error_code::TRUST_HOME_UNKNOWN,
            "the home directory could not be determined, so no folder can be checked against it",
            &[],
        ));
    };
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
        &[("path", &directory.to_string_lossy())],
    )
}

/// Starts acting on this session's trust screen whenever it appears. Gone when the session is.
pub fn supervise(state: &Arc<AppState>, live: &Arc<LiveSession>) {
    let Some(mut sightings) = live.trust.take_sightings() else {
        return;
    };
    if !live.trust.screen.hook_at_start {
        release_when_the_screen_goes(state, live);
    }
    let state = state.clone();
    // Weak, so that the session being dropped closes its sightings and ends this task.
    let live = Arc::downgrade(live);
    tokio::spawn(async move {
        while sightings.recv().await.is_some() {
            let Some(live) = live.upgrade() else { break };
            if let Err(err) = on_sighting(&state, &live).await {
                tracing::warn!(session = %live.id, %err, "handling the trust confirmation failed");
            }
        }
    });
}

/// Releases the messages held for a session whose agent runs no hook at start (Codex;
/// [`TrustState::holds_writes`]), which nothing it reports would release before the person's first
/// prompt — and anything released only then would land behind that prompt. So this watches for the
/// two ends a hook does not mark:
///
/// - the watch ending with no screen sighted — a session started with something queued, a
///   relaunch with an instruction for it, say;
/// - a sighted screen the person answered in the terminal: once something has been typed into the
///   session since the sighting, the output printed since the sighting must show the screen
///   replaced (`Screen::replaced_in`: the next screen's text drawn after the last of the screen's
///   own), and still a moment later. A Down only redraws the options, and a key Codex ignores
///   draws an empty frame; both keep the hold, while typing on into the next screen does not.
///   Once more than [`WATCHED_ANSWER_BYTES`] has been printed since the sighting, this stops
///   looking and the hold stays until a hook. The review Codex raises for new hooks names neither
///   option, and is kept from appearing here only by the `--dangerously-bypass-hook-trust` every
///   launch passes (`crate::adapter::codex`).
///
/// Octoboard's successful press and a hook release them on their own.
fn release_when_the_screen_goes(state: &Arc<AppState>, live: &Arc<LiveSession>) {
    let state = state.clone();
    let live = Arc::downgrade(live);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(RELEASE_POLL).await;
            let Some(live) = live.upgrade() else { break };
            let trust = &live.trust;
            if trust.hook_seen.load(Ordering::Acquire) || trust.pressed.load(Ordering::Acquire) {
                break;
            }
            if !trust.sighted.load(Ordering::Acquire) {
                if !trust.holds_writes() {
                    release_held(&state, &live.id);
                    break;
                }
                continue;
            }
            if !trust.input_since_sighting(live.input_writes()) {
                continue;
            }
            let screen = trust.screen;
            let mark = trust.output_at_sighting.load(Ordering::Acquire);
            if live.output_total().saturating_sub(mark) > WATCHED_ANSWER_BYTES {
                // Too much has been printed to judge by order; the hold stays for a hook.
                break;
            }
            let since_sighting = || live.output_since(mark);
            if !screen.replaced_in(&since_sighting()) {
                continue;
            }
            tokio::time::sleep(QUIET).await;
            if screen.replaced_in(&since_sighting()) {
                trust.dismissed.store(true, Ordering::Release);
                release_held(&state, &live.id);
                break;
            }
        }
    });
}

/// Writes out whatever was queued for the session while its trust screen held it.
fn release_held(state: &Arc<AppState>, session_id: &str) {
    if let Ok(session) = state.session_record(session_id) {
        state.spawn_flush_outbox(session_id, state.agent_status(&session));
    }
}

/// A hook of this session reached the daemon, so it is past its trust screen. Lets through what
/// was held for it, and, where the agent's trust record has to be carried (Grok Build), carries the
/// entry the agent wrote when the hook ends a screen Octoboard has not pressed successfully: the
/// person answered it in the terminal, or Octoboard's press is still waiting on this very hook or
/// has failed. A press waiting on it is handed the outcome; otherwise a failure is a notice, and
/// an entry that never appears — the person declined — leaves everything as it is, silently.
/// Nothing of Octoboard's own permission is recorded here; only a successful press does that.
pub fn on_hook(state: &Arc<AppState>, live: &Arc<LiveSession>) {
    let noted = live.trust.note_hook();
    if noted.released {
        release_held(state, &live.id);
    }
    let Some(carried) = live.trust.carried.clone().filter(|_| noted.carry) else {
        return;
    };
    let state = state.clone();
    let live = live.clone();
    tokio::task::spawn_blocking(move || {
        let outcome = carry(&carried);
        let Some(Err(failure)) = live.trust.hand_over_carry(outcome) else {
            return;
        };
        if failure.reason_code == trust_reason::NOT_RECORDED {
            return;
        }
        report_failure(
            &state,
            &live.id,
            live.agent,
            &not_carried_over(live.agent, &carried, &failure),
        );
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
    state.broadcast(prompt(&session, &project));
    Ok(())
}

/// The prompt for one session at its screen, naming the agent that is asking, with the directory
/// its parent-directory button would trust, if there is one to offer.
fn prompt(session: &Session, project: &Project) -> Event {
    Event::TrustPrompt {
        session: session.id.clone(),
        agent: session.agent,
        project: project.id.clone(),
        path: project.path.clone(),
        trust_dir: trustable_parent(&project.path, &paths::home_dir())
            .ok()
            .map(|directory| directory.to_string_lossy().into_owned()),
    }
}

/// The live project sessions that are at their trust screen with nobody answering it, each with
/// its record and its project.
fn waiting_project_sessions(state: &AppState) -> Vec<(Arc<LiveSession>, Session, Project)> {
    state
        .live_sessions()
        .into_iter()
        .filter(|live| still_waiting(live))
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
    if let Err(err) = answer_off_the_runtime(state, live.clone()).await {
        report_failure(state, &live.id, live.agent, &err);
    }
}

/// [`answer`], which blocks, on a blocking thread; once it has pressed the screen of an agent that
/// runs no hook at start (Codex), the messages held for the session are let through. The other two
/// agents' are let through by their first hook ([`on_hook`]).
async fn answer_off_the_runtime(state: &Arc<AppState>, live: Arc<LiveSession>) -> Result<()> {
    let id = live.id.clone();
    let hook_at_start = live.trust.screen.hook_at_start;
    tokio::task::spawn_blocking(move || answer(&live))
        .await
        .map_err(anyhow::Error::from)
        .and_then(|outcome| outcome)?;
    if !hook_at_start {
        release_held(state, &id);
    }
    Ok(())
}

/// The right to answer was not there to take: the screen is not up, or somebody else — a
/// concurrent go-ahead, or the user in the terminal — has answered it already. Nothing went wrong,
/// so nobody is told, and a client that asked is told with a code it shows nothing for.
fn not_waiting() -> anyhow::Error {
    CodedError::raised(
        error_code::TRUST_NOT_WAITING,
        "this session is not waiting at its agent's trust confirmation any more",
        &[],
    )
}

/// Pressing `agent`'s screen failed for `reason`, the English account that `reason_code` names
/// for a client to word. `detail` is what the code's wording is filled with, if anything.
fn answer_failed(
    agent: Agent,
    reason_code: &'static str,
    reason: impl Into<String>,
    detail: &[(&str, &str)],
) -> anyhow::Error {
    let reason = reason.into();
    let mut pairs = vec![
        ("agent", agent.label()),
        ("reason", reason.as_str()),
        ("reason_code", reason_code),
    ];
    pairs.extend_from_slice(detail);
    CodedError::raised(
        error_code::TRUST_ANSWER_FAILED,
        answer_failed_message(agent, &reason),
        &pairs,
    )
}

fn answer_failed_message(agent: Agent, reason: &str) -> String {
    format!(
        "{} could not press {}'s trust confirmation ({reason}). Answer it in the terminal.",
        crate::APP_NAME,
        agent.label()
    )
}

/// Grok's confirmation was pressed, but the entry it wrote could not be carried over to the
/// user's own store, for `reason`. Nothing was written there.
fn not_carried_over(agent: Agent, carried: &CarriedTrust, failure: &CarryFailure) -> anyhow::Error {
    let path = carried.user_store.to_string_lossy();
    let mut pairs = vec![
        ("agent", agent.label()),
        ("path", path.as_ref()),
        ("reason", failure.reason.as_str()),
        ("reason_code", failure.reason_code),
    ];
    if let Some(detail) = &failure.detail {
        pairs.push(("detail", detail));
    }
    CodedError::raised(
        error_code::TRUST_NOT_CARRIED_OVER,
        format!(
            "{app} could not copy {agent}'s trust for this folder into `{path}` ({reason}), so \
             {agent} will ask again the next time it opens this folder.",
            app = crate::APP_NAME,
            agent = agent.label(),
            reason = failure.reason,
        ),
        &pairs,
    )
}

fn is_not_waiting(err: &anyhow::Error) -> bool {
    err.downcast_ref::<CodedError>()
        .is_some_and(|coded| coded.code == error_code::TRUST_NOT_WAITING)
}

/// Logs a failed answer and tells the user, whoever asked for it: the screen is then still up and
/// theirs to answer, or, where only carrying Grok's entry over failed, Grok will ask again next
/// time. A lost claim is not a failure.
fn report_failure(state: &Arc<AppState>, session_id: &str, agent: Agent, err: &anyhow::Error) {
    if is_not_waiting(err) {
        tracing::debug!(session = %session_id, "the trust confirmation is already being answered");
        return;
    }
    let carrying = err
        .downcast_ref::<CodedError>()
        .is_some_and(|coded| coded.code == error_code::TRUST_NOT_CARRIED_OVER);
    if carrying {
        tracing::warn!(session = %session_id, %err, "carrying the trust entry over failed");
    } else {
        tracing::warn!(session = %session_id, %err, "pressing the trust confirmation failed");
    }
    // Its code and params carry over as they are; anything else is told as its English text.
    let carried_over = |code, coded: &CodedError| Notice {
        code,
        message: coded.message.clone(),
        params: coded.params.clone(),
    };
    let notice = match err.downcast_ref::<CodedError>() {
        Some(coded) if coded.code == error_code::TRUST_ANSWER_FAILED => {
            carried_over(notice_code::TRUST_ANSWER_FAILED, coded)
        }
        Some(coded) if coded.code == error_code::TRUST_NOT_CARRIED_OVER => {
            carried_over(notice_code::TRUST_NOT_CARRIED_OVER, coded)
        }
        _ => {
            let reason = err.to_string();
            Notice::new(
                notice_code::TRUST_ANSWER_FAILED,
                answer_failed_message(agent, &reason),
                &[("agent", agent.label()), ("reason", &reason)],
            )
        }
    };
    state.broadcast(notice.about(session_id));
}

/// The prompts to put to a client that has just been sent a snapshot: one for each live project
/// session that is at its screen with nobody answering it and whose project has no permission, of
/// its own or through a trusted directory. A prompt is broadcast once, so a client that missed it —
/// it was not connected, or a snapshot replaced what it had — is told here, and one that already
/// has it ignores the repeat.
pub fn pending_prompts(state: &AppState) -> Vec<Event> {
    let trusted = state.store.trusted_directories().unwrap_or_default();
    waiting_project_sessions(state)
        .into_iter()
        .filter(|(_, session, project)| !consented(session, Some(project), &trusted))
        .map(|(_, session, project)| prompt(&session, &project))
        .collect()
}

/// The user's go-ahead for one session's screen, from the dialog a `trust_prompt` opened, whichever
/// agent is asking. Refused unless that session is a running project session still waiting at its
/// screen — a stale dialog, a session that has since gone, or a request naming something else
/// entirely changes nothing. The permission is recorded only once the screen has been pressed — the
/// project's when `remember` asks for it, its parent directory's when `trust_parent_dir` does; a
/// failure to press is reported to the user as a notice as well as to the caller, because the
/// dialog it came from may be closed by then. A parent directory that is too broad is refused
/// before anything is pressed.
pub async fn confirm(
    state: &Arc<AppState>,
    session_id: &str,
    remember: bool,
    trust_parent_dir: bool,
) -> Result<()> {
    let session = state.session_record(session_id)?;
    let Some(live) = state.live_session(session_id) else {
        return Err(CodedError::raised(
            error_code::SESSION_NOT_RUNNING,
            "this session is not running",
            &[("session", session_id)],
        ));
    };
    if !still_waiting(&live) {
        return Err(not_waiting());
    }
    let Some(project_id) = &session.project_id else {
        return Err(CodedError::raised(
            error_code::CONSOLE_SESSION_TRUST_NOT_ASKED,
            format!(
                "a console session's trust confirmation is pressed by {} without asking",
                crate::APP_NAME
            ),
            &[],
        ));
    };
    let mut project = state
        .store
        .get_project(project_id)?
        .ok_or_else(|| CodedError::unknown_project(project_id))?;

    let directory = if trust_parent_dir {
        Some(trustable_parent(&project.path, &paths::home_dir())?)
    } else {
        None
    };

    if let Err(err) = answer_off_the_runtime(state, live).await {
        report_failure(state, session_id, session.agent, &err);
        return Err(err);
    }

    if let Some(directory) = directory {
        add_trusted_directory(state, &directory)?;
    } else if remember && !project.trust_consent {
        state.store.set_project_trust_consent(&project.id, true)?;
        project.trust_consent = true;
        let project_id = project.id.clone();
        state.broadcast(Event::ProjectUpserted { project });
        // The dialog promised that later confirmations for this project are pressed without
        // asking, and so are the ones already waiting, whichever agent shows them.
        for (live, _, waiting) in waiting_project_sessions(state) {
            if waiting.id == project_id {
                let state = state.clone();
                tokio::spawn(async move { answer_and_report(&state, &live).await });
            }
        }
    }
    Ok(())
}

/// Trusts a directory and everything below it, tells every client, and answers the screens already
/// waiting in projects under it, whichever agent shows them. Each project's own permission is
/// untouched. Needs the runtime: the answers go off on tasks of their own.
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

/// Stops trusting a directory, and tells every client. Projects' own permissions stay, and so do
/// the sessions already running and the trust each agent has already recorded.
pub fn remove_trusted_directory(state: &Arc<AppState>, path: &str) -> Result<()> {
    let path = normalise_path(path).to_string_lossy().into_owned();
    if state.store.remove_trusted_directory(&path)? {
        state.broadcast(Event::TrustedDirectoriesUpdated {
            trusted_directories: state.store.trusted_directories()?,
        });
    }
    Ok(())
}

/// Whether this session's screen is up and nobody has started answering it, here or in the
/// terminal.
fn still_waiting(live: &LiveSession) -> bool {
    live.trust.waiting_with_input(live.input_writes())
}
