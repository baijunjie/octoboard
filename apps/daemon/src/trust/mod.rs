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
//! (`trusted_directories`), which covers every project whose directory really lies under it,
//! existing or added later, judged on the file system rather than on the paths' text ([`Place`]).
//! Either form lets a later confirmation of any agent be pressed without asking.
//!
//! **Detection.** Every session's output is watched by a [`TrustState`], fed from the PTY reader
//! thread whether or not a client is attached, against its own agent's [`Screen`]. The screen is
//! recognised by how it is first drawn after the output has been reduced to bare text
//! ([`normalise`]): the agents do not print a space between words, they move the cursor, so the raw
//! bytes never contain the words a person reads. A sighting is signalled once per session. That is
//! enough for "once per occurrence" because the confirmation cannot come back within one process.
//! Claude Code and Grok run no hook before it is answered and Grok runs `SessionStart` right after,
//! so the first hook callback retires the watch; Codex runs none around it, so for Codex and Grok
//! anything written into the session's input since the sighting — the person answering in the
//! terminal — also means the screen is no longer waiting: its prompt is not put to a client again
//! and a go-ahead for it is refused as stale. The watch ends at the agent's start signal
//! ([`StartSignal`]), never at a limit of output or time, since an agent can print a good deal
//! first (Claude Code's onboarding or Codex's login on an account's first launch, each with a login
//! in a browser) and shows the signal only once past the screen: the first hook for Claude Code and
//! Grok, and for Codex, which runs none until its first prompt, the first terminal title it writes,
//! which it does as its session starts and never before (the launch fixes the title's items, so no
//! configuration switches it off). Nothing a transcript quotes can precede it. A transcript that
//! quotes a screen is not taken for it unless it reproduces the first-draw pattern inside the
//! output window before the watch has ended; and whatever is sighted, keys are sent only if the
//! checks below pass.
//!
//! **Octoboard's own messages are held** for as long as a screen may be up
//! ([`TrustState::holds_writes`]), since a message's trailing Enter would otherwise answer it —
//! accept Codex's or Grok's, decline Claude Code's — on nobody's say-so:
//!
//! - Claude Code and Grok run a hook as they start, once they are past the screen, so their
//!   messages are held until that first hook and released by it ([`on_hook`]); another startup
//!   modal can follow Claude Code's screen, and only the hook says the session is ready. A Claude
//!   Code session whose hooks never arrive holds its messages for as long as it runs; a Grok one
//!   does so only until [`HOOK_FALLBACK`] after its launch, since a managed policy can keep every
//!   hook of Octoboard's from running. Then they are let through ([`release_when_no_hook_comes`]),
//!   unless its screen has been sighted, which keeps them held until the rendered screen no longer
//!   shows it ([`release_when_the_sighted_screen_goes`]). A dialog of the agent's holds them as
//!   it does at any other time ([`crate::agent_dialogs`]).
//! - Claude Code's hook is not enough on its own: with the fullscreen renderer `SessionStart`
//!   runs before it reads its input, and a message written in that gap is taken by its
//!   early-input capture, which drops the paste markers and turns the Enter into a newline. They
//!   are held until the session's parsed terminal also has bracketed paste on
//!   (`Screen::waits_for_paste_mode`), whichever of the two comes last releasing them
//!   ([`on_hook`], [`release_when_paste_mode_comes`]); the inline renderer turns it on first, so
//!   its first hook releases at once. Bracketed paste turned on and off again before the hook, as
//!   Claude Code's terminal probe does at start, does not count.
//! - Codex runs none until its first prompt, so its messages are held while the watch is still
//!   looking (until its first terminal title) and, once its screen is sighted, until Octoboard's
//!   press of it succeeds, a hook arrives, or the person's own answer in the terminal is seen to
//!   take the screen away. The press releases them itself; the end of the watch with nothing
//!   sighted and the person's answer are watched for by [`supervise`]
//!   (`release_when_the_screen_goes`).
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
//!   screen was sighted — and the press counts once Grok has recorded the trust, or, when no hook
//!   comes to carry the record, once the rendered screen is seen to have gone.
//!
//! Whatever cannot be confirmed ends the attempt with the keys not sent, and the screen waits for
//! the person to answer it. Each session is answered at most once, so a failed attempt is not
//! retried behind the person's back.

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::hostfs::{self, lexically_normalise};
use crate::paths;
use crate::protocol::{
    error_code, notice_code, trust_reason, Agent, CodedError, Event, Notice, Project, Role, Session,
};
use crate::session::LiveSession;
use crate::state::AppState;

mod carry;
mod place;
mod press;
mod screen;
#[cfg(test)]
mod tests;

pub(crate) use carry::write_replacing;
pub use carry::CarriedTrust;
use carry::*;
use place::*;
use press::*;
pub(crate) use press::{wait_for, wait_quiet};
pub use screen::TrustState;
use screen::*;

/// How much output after a sighting the watch for the person's answer reads at most. Codex prints
/// little at its screen; past this, its next screen is left for a hook to announce.
const WATCHED_ANSWER_BYTES: u64 = 128 * 1024;

/// How often a session's held messages are looked at while its watch runs.
const RELEASE_POLL: Duration = Duration::from_millis(250);

/// How long after its launch a Grok Build session may go without a hook before the messages held
/// for it are let through anyway ([`Screen::hook_fallback`]). Its `SessionStart` normally comes
/// within a second of the launch, so a hook that has not come by now is not coming.
const HOOK_FALLBACK: Duration = Duration::from_secs(30);

/// Whether Octoboard may answer this session's screen without asking: a console session always
/// may, because its working directory is the console's own, which holds nothing but the
/// instruction file Octoboard wrote there; a project session only with the user's permission,
/// either for that project or for a trusted directory its directory really lies in
/// ([`covered`]). The permission is shared by every agent, so it does not matter which one is
/// asking.
async fn consented(state: &AppState, session: &Session, project: Option<&Project>) -> Result<bool> {
    if session.role == Role::Console {
        return Ok(true);
    }
    let Some(project) = project else {
        return Ok(false);
    };
    if project.trust_consent {
        return Ok(true);
    }
    let trusted = state.store.trusted_directories()?;
    let projects = [project.path.clone()];
    Ok(covered(state, &projects, &trusted)
        .await
        .contains(&project.path))
}

/// A path as the trusted directories are kept and removed: lexically normalised — `.` dropped,
/// `..` folded into the component before it, trailing slashes gone. Only the text is kept; which
/// projects a trusted directory covers is judged where it really is ([`covered`]).
fn normalise_path(text: &str) -> PathBuf {
    lexically_normalise(Path::new(text))
}

/// Which of the `projects` paths lie in one of the `trusted` directories: where the project's
/// directory, with every symbolic link on its path resolved, is the directory a trusted path leads
/// to or lies below it ([`Place`]). Never the text: `/a/Project` does not cover `/a/Project2`, a
/// project reached through a link inside a trusted directory that leads out of it is not covered,
/// and one whose path is spelled through a link into it, or in another letter case on a file
/// system that ignores case, is. A path that cannot be resolved in time, on either side, covers
/// nothing and is covered by nothing, and a trusted path whose directory is the root, the home
/// directory or one above it covers nothing ([`too_broad_to_trust`]); when those directories
/// cannot be looked at in time, no trusted directory covers anything. The paths are looked at off
/// the runtime, a few at a time, each bounded by [`LOOK_DEADLINE`] ([`looked_within`]).
async fn covered(state: &AppState, projects: &[String], trusted: &[String]) -> HashSet<String> {
    covered_with_home(state, projects, trusted, &paths::home_dir()).await
}

/// [`covered`], with the home directory given.
async fn covered_with_home(
    state: &AppState,
    projects: &[String],
    trusted: &[String],
    home: &Path,
) -> HashSet<String> {
    if projects.is_empty() || trusted.is_empty() {
        return HashSet::new();
    }
    let paths = projects.iter().chain(trusted).map(PathBuf::from).collect();
    let home = home.to_path_buf();
    let (places, broad) = tokio::join!(
        looked_within(&state.trust_looks, paths, LOOK_DEADLINE, Place::of),
        looked_within(
            &state.trust_looks,
            vec![home.clone()],
            LOOK_DEADLINE,
            |home| Some(too_broad_to_trust(home))
        ),
    );
    let Some(broad) = broad.get(&home) else {
        return HashSet::new();
    };
    let directories: Vec<&Place> = trusted
        .iter()
        .filter_map(|directory| places.get(Path::new(directory)))
        .filter(|directory| !directory.is_one_of(broad))
        .collect();
    projects
        .iter()
        .filter(|project| {
            places
                .get(Path::new(project.as_str()))
                .is_some_and(|place| directories.iter().any(|directory| place.lies_in(directory)))
        })
        .cloned()
        .collect()
}

/// The directory each of the `projects` paths has to offer for trusting: its parent
/// ([`trustable_parent`]), and only where trusting that would cover the project — not for a
/// project reached through a link that leads out of its parent, nor for one that cannot be
/// resolved in time. A project with none to offer is absent.
async fn offered(state: &AppState, projects: Vec<String>) -> HashMap<String, PathBuf> {
    let home = paths::home_dir();
    looked_within(
        &state.trust_looks,
        projects.into_iter().map(PathBuf::from).collect(),
        LOOK_DEADLINE,
        move |project| {
            let parent = trustable_parent(&project.to_string_lossy(), &home).ok()?;
            Place::of(project)?
                .lies_in(&Place::of(&parent)?)
                .then_some(parent)
        },
    )
    .await
    .into_iter()
    .map(|(project, parent)| (project.to_string_lossy().into_owned(), parent))
    .collect()
}

/// [`trustable_parent`] for a go-ahead, off the runtime and bounded by [`LOOK_DEADLINE`]: a parent
/// that cannot be checked in time — its look outran the deadline, or one at the same project is
/// still stuck — is refused with `trust_directory_unreachable`, so that nothing is pressed on a
/// check that was not made.
async fn parent_to_trust(state: &AppState, project_path: &str) -> Result<PathBuf> {
    let home = paths::home_dir();
    let path = PathBuf::from(project_path);
    let mut found = looked_within(
        &state.trust_looks,
        vec![path.clone()],
        LOOK_DEADLINE,
        move |project| Some(trustable_parent(&project.to_string_lossy(), &home)),
    )
    .await;
    found.remove(&path).unwrap_or_else(|| {
        Err(CodedError::raised(
            error_code::TRUST_DIRECTORY_UNREACHABLE,
            format!(
                "`{project_path}` or the folder above it did not respond in time, so that folder \
                 could not be checked"
            ),
            &[("path", project_path)],
        ))
    })
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
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(a), Ok(b)) => hostfs::identity(&a) == hostfs::identity(&b),
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
    if !live.trust.screen.hook_at_start() {
        release_when_the_screen_goes(state, live);
    }
    if live.trust.screen.waits_for_paste_mode {
        release_when_paste_mode_comes(state, live);
    }
    if let Some(bound) = live.trust.screen.hook_fallback {
        release_when_no_hook_comes(state, live, bound);
        release_when_the_sighted_screen_goes(state, live);
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
                publish_status(&state, &live);
                release_held(&state, &live.id);
                break;
            }
        }
    });
}

/// Releases the messages held for a session whose agent turns bracketed paste on only once it
/// reads its input ([`Screen::waits_for_paste_mode`]), when that comes after its first hook: the
/// drain does not retry by itself, and nothing else is left to call for it. When bracketed paste
/// comes first the first hook releases them ([`on_hook`]), and the release here finds nothing left.
/// It waits for both, not for bracketed paste alone: bracketed paste seen before the hook can be
/// undone again ([`TrustState::note_paste_mode`]), and the hook then releases nothing.
fn release_when_paste_mode_comes(state: &Arc<AppState>, live: &Arc<LiveSession>) {
    let state = state.clone();
    let live = Arc::downgrade(live);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(RELEASE_POLL).await;
            let Some(live) = live.upgrade() else { break };
            if live.trust.follows_paste_mode() {
                continue;
            }
            release_held(&state, &live.id);
            break;
        }
    });
}

/// Lets through the messages held for a session whose agent should have run a hook by now and has
/// not (Grok Build; [`Screen::hook_fallback`]): once `bound` has passed since the launch, with no
/// hook seen and no trust screen sighted. A sighted screen stays held until it is answered
/// ([`release_when_the_sighted_screen_goes`]). Nothing here looks for a dialog of the agent's: the
/// drain refuses a session that shows one, and the dialog watch writes out what waited when it
/// goes ([`crate::agent_dialogs`]).
fn release_when_no_hook_comes(state: &Arc<AppState>, live: &Arc<LiveSession>, bound: Duration) {
    let state = state.clone();
    let live = Arc::downgrade(live);
    tokio::spawn(async move {
        tokio::time::sleep(bound).await;
        let Some(live) = live.upgrade() else { return };
        if live.trust.release_without_hook() {
            release_held(&state, &live.id);
        }
    });
}

/// Releases the messages held for a session whose agent's hook cannot be relied on (Grok Build;
/// [`Screen::hook_fallback`]) once its sighted trust screen has been answered, which nothing it
/// reports would say: under a managed policy that allows only managed hooks none of Octoboard's
/// runs, so neither Octoboard's press (which waits for the hook that carries the trust record)
/// nor the person's answer in the terminal is ever followed by one. A screen sighted after the
/// fallback has let the messages through is held for the same way.
///
/// The answer is read off the rendered screen, which shows what is up now, rather than off what
/// was printed since the sighting: Grok animates its logo for as long as it waits at the screen,
/// so output after the sighting says nothing about it, and Octoboard's own press is not counted as
/// input to gate on. The screen is gone when none of its text is on the rendered screen
/// ([`Screen::gone_in`]) twice, a moment apart; a hook ends the watch as it ends the hold.
fn release_when_the_sighted_screen_goes(state: &Arc<AppState>, live: &Arc<LiveSession>) {
    let state = state.clone();
    let live = Arc::downgrade(live);
    tokio::spawn(async move {
        let gone = |live: &LiveSession| live.trust.screen.gone_in(live.screen_text().as_bytes());
        loop {
            tokio::time::sleep(RELEASE_POLL).await;
            let Some(live) = live.upgrade() else { break };
            let trust = &live.trust;
            if trust.hook_seen.load(Ordering::Acquire) {
                break;
            }
            if !trust.sighted.load(Ordering::Acquire) || !gone(&live) {
                continue;
            }
            tokio::time::sleep(QUIET).await;
            if trust.hook_seen.load(Ordering::Acquire) {
                break;
            }
            if gone(&live) {
                trust.dismissed.store(true, Ordering::Release);
                trust.needs_user.store(false, Ordering::Release);
                publish_status(&state, &live);
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

/// A hook of this session reached the daemon, so it is past its trust screen. Lets through what was
/// held for it, and, where the agent's trust record has to be carried (Grok Build), carries the
/// entry the agent wrote when the hook ends a screen whose record nothing has carried yet: the
/// person answered it in the terminal, Octoboard's press is still waiting on this very hook or has
/// failed, or the press counted once the rendered screen was seen to go with no hook. A press
/// waiting on it is handed the outcome; otherwise a failure is a notice, and an entry that never
/// appears — the person declined — leaves everything as it is, silently. Nothing of Octoboard's own
/// permission is recorded here; only a successful press does that.
pub fn on_hook(state: &Arc<AppState>, live: &Arc<LiveSession>) {
    let noted = live.trust.note_hook();
    if live.trust.needs_user.swap(false, Ordering::AcqRel) {
        publish_status(state, live);
    }
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
    if consented(state, &session, project.as_ref()).await? {
        answer_and_report(state, live).await;
        return Ok(());
    }
    let project = project.ok_or_else(|| anyhow!("the session's project is gone"))?;
    // The look at the file system took a while, and the person may have answered meanwhile.
    if !still_waiting(live) {
        return Ok(());
    }
    live.trust.needs_user.store(true, Ordering::Release);
    state.publish_session(&session);
    crate::reporting::spawn_tell_owner_it_waits(state, &live.id);
    let trust_dir = offered(state, vec![project.path.clone()])
        .await
        .remove(&project.path);
    // Again after the second look: a prompt for a screen that has been answered since — in the
    // terminal, or pressed for a directory trusted meanwhile — would ask about nothing, and could
    // reach a client after the status that would have dropped it.
    if still_waiting(live) && live.trust.needs_user() {
        state.broadcast(prompt(&session, &project, trust_dir));
    }
    Ok(())
}

/// The prompt for one session at its screen, naming the agent that is asking, with the directory
/// its parent-directory button would trust, if there is one to offer ([`offered`]).
fn prompt(session: &Session, project: &Project, trust_dir: Option<PathBuf>) -> Event {
    Event::TrustPrompt {
        session: session.id.clone(),
        agent: session.agent,
        project: project.id.clone(),
        path: project.path.clone(),
        trust_dir: trust_dir.map(|directory| directory.to_string_lossy().into_owned()),
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
    let hook_at_start = live.trust.screen.hook_at_start();
    let answering = live.clone();
    let outcome = tokio::task::spawn_blocking(move || answer(&answering))
        .await
        .map_err(anyhow::Error::from)
        .and_then(|outcome| outcome);
    if outcome.is_err() && !live.trust.pressed.load(Ordering::Acquire) {
        live.trust.needs_user.store(true, Ordering::Release);
    }
    publish_status(state, &live);
    if live.trust.needs_user() {
        crate::reporting::spawn_tell_owner_it_waits(state, &live.id);
    }
    outcome?;
    if !hook_at_start {
        release_held(state, &id);
    }
    Ok(())
}

fn publish_status(state: &AppState, live: &LiveSession) {
    if let Ok(session) = state.session_record(&live.id) {
        state.publish_session(&session);
    }
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
pub async fn pending_prompts(state: &AppState) -> Vec<Event> {
    let trusted = state.store.trusted_directories().unwrap_or_default();
    let waiting: Vec<(Arc<LiveSession>, Session, Project)> = waiting_project_sessions(state)
        .into_iter()
        .filter(|(_, session, project)| session.role != Role::Console && !project.trust_consent)
        .collect();
    let paths: Vec<String> = waiting
        .iter()
        .map(|(_, _, project)| project.path.clone())
        .collect();
    let covered = covered(state, &paths, &trusted).await;
    let asked: Vec<(Arc<LiveSession>, Session, Project)> = waiting
        .into_iter()
        .filter(|(_, _, project)| !covered.contains(&project.path))
        .collect();
    let asked_paths = asked
        .iter()
        .map(|(_, _, project)| project.path.clone())
        .collect();
    let offered = offered(state, asked_paths).await;
    // The looks took a while: a confirmation answered meanwhile is not asked about again.
    asked
        .into_iter()
        .filter(|(live, _, _)| still_waiting(live))
        .map(|(_, session, project)| {
            let trust_dir = offered.get(&project.path).cloned();
            prompt(&session, &project, trust_dir)
        })
        .collect()
}

/// The user's go-ahead for one session's screen, from the dialog a `trust_prompt` opened, whichever
/// agent is asking. Refused unless that session is a running project session still waiting at its
/// screen — a stale dialog, a session that has since gone, or a request naming something else
/// entirely changes nothing. The permission is recorded only once the screen has been pressed — the
/// project's when `remember` asks for it, its parent directory's when `trust_parent_dir` does; a
/// failure to press is reported to the user as a notice as well as to the caller, because the
/// dialog it came from may be closed by then. A parent directory that is too broad is refused
/// before anything is pressed, and so is one that cannot be checked in time ([`parent_to_trust`]).
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
        Some(parent_to_trust(state, &project.path).await?)
    } else {
        None
    };

    if let Err(err) = answer_off_the_runtime(state, live).await {
        report_failure(state, session_id, session.agent, &err);
        return Err(err);
    }

    if let Some(directory) = directory {
        add_trusted_directory(state, &directory).await?;
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
/// waiting in projects that really lie in it ([`covered`]), whichever agent shows them. Each
/// project's own permission is untouched. Needs the runtime: the answers go off on tasks of their
/// own.
async fn add_trusted_directory(state: &Arc<AppState>, directory: &Path) -> Result<()> {
    let directory = directory.to_string_lossy().into_owned();
    if !state.store.add_trusted_directory(&directory)? {
        return Ok(());
    }
    state.broadcast(Event::TrustedDirectoriesUpdated {
        trusted_directories: state.store.trusted_directories()?,
    });
    let waiting = waiting_project_sessions(state);
    let paths: Vec<String> = waiting
        .iter()
        .map(|(_, _, project)| project.path.clone())
        .collect();
    let covered = covered(state, &paths, &[directory]).await;
    for (live, _, project) in waiting {
        if covered.contains(&project.path) {
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
