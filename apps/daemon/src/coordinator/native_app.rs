//! Opening a session's conversation in its agent's own desktop application. Which agents have one
//! is the descriptors' (`Capabilities::native_app_scheme`, what the host is asked about) and how
//! one is asked to show a conversation the provider's (`AgentProvider::native_app`); this only
//! checks the session and the host. The application and Octoboard would both write to a
//! conversation a live process holds or a resume would start, so a session that is not archived
//! is archived first, and one with a process only when it is at rest. The applications read the
//! default account's directory only, so one whose latest record that directory does not hold may
//! not show in them ([`default_directory_holds_latest`]); that is told to the client
//! (`probe_native_apps`, [`probe`]), which warns, and does not keep it from being opened.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;

use crate::adapter::{native_app_unavailable as unavailable, provider, NativeApp, NativeAppOpen};
use crate::platform::open::native_app::url_scheme_handled;
use crate::protocol::{error_code, AgentId, CodedError, Session, SessionStatus};
use crate::records::locate::Locator;
use crate::state::AppState;

/// The most sessions one `probe_native_apps` judges.
pub const PROBED_SESSIONS_LIMIT: usize = 200;

/// The URL scheme `agent`'s desktop application registers, for an agent that has one.
fn scheme_of(agent: AgentId) -> Option<&'static str> {
    agent.descriptor().capabilities.native_app_scheme
}

/// The agents whose desktop application is installed on this host. Blocking.
pub fn installed_agents() -> Vec<AgentId> {
    AgentId::ALL
        .into_iter()
        .filter(|agent| scheme_of(*agent).is_some_and(url_scheme_handled))
        .collect()
}

/// Opens `id`'s conversation in its agent's desktop application. An archived session is opened as
/// it is. Any other is archived first, as a user's archive does (what is bound to it is archived
/// with it, running sessions included, but no branch or worktree is removed), so that nothing
/// resumes the conversation the application now holds: the trees in `keep_process_ids` are kept
/// from being stopped with it, and the open waits until its process is gone. The session stays
/// archived if its process does not go down in time or the application then fails to open.
/// Refused with `session_working` while the session itself is at work
/// ([`AppState::what_it_is_doing`]), `session_locked` for it or a session bound to it,
/// `session_already_starting` while a launch or another operation holds it,
/// `account_sharing_in_progress` while a change of its account's sharing moves the account's
/// directory, `session_did_not_stop`, the codes of [`super::processes::keep`] for a tree that
/// cannot be kept, `daemon_shutting_down`, `shell_environment_timeout` when the shell environment
/// cannot be captured, and `native_app_unavailable` for an agent with no application, one that is
/// not installed, a session with no conversation yet, a conversation id the agent would not take
/// and a launch that fails. Everything that does not depend on archiving is checked before it, so
/// a refusal for it leaves everything as it was; the one exception is a `session_working` found
/// only by the check repeated after the chosen trees were kept, which leaves them kept, and the
/// session running. The archive cancels the session's pending recoveries as its process leaves
/// (the user now has the conversation); those of a session with no process, or already archived,
/// are cancelled just before the application is asked. `keep_process_ids` is ignored for an
/// archived session.
pub async fn open_session_in_app(
    state: &Arc<AppState>,
    id: &str,
    keep_process_ids: Vec<String>,
) -> Result<()> {
    open_with(
        state,
        id,
        keep_process_ids,
        url_scheme_handled,
        crate::env_shell::cached_snapshot,
        ask_app(state),
    )
    .await
}

/// The asking [`open_session_in_app`] does: cancels the session's pending recoveries and has the
/// application open the conversation. The cancel is a guard for a session with no process to
/// archive, an archived one, whose recoveries no archive cancelled.
fn ask_app(
    state: &Arc<AppState>,
) -> impl FnOnce(&dyn NativeApp, &Session, &str, &Path, &Path, &HashMap<String, String>) -> Result<()>
       + Send
       + 'static {
    let state = Arc::clone(state);
    move |app, session, conversation, cwd, scratch, shell| {
        state.cancel_recoveries(&session.id);
        app.open(&NativeAppOpen {
            session,
            conversation,
            cwd,
            shell,
            scratch,
        })
    }
}

/// [`open_session_in_app`] with the things that reach the host handed in, so that a test opens
/// nothing: `installed` asks whether a scheme is handled, `snapshot` takes the shell environment,
/// and `ask` does the asking.
async fn open_with(
    state: &Arc<AppState>,
    id: &str,
    keep_process_ids: Vec<String>,
    installed: fn(&str) -> bool,
    snapshot: fn() -> Result<HashMap<String, String>>,
    ask: impl FnOnce(&dyn NativeApp, &Session, &str, &Path, &Path, &HashMap<String, String>) -> Result<()>
        + Send
        + 'static,
) -> Result<()> {
    // Held until the open has ended, archive included: a resume, a switch or a launch begun
    // meanwhile would start a process on the conversation the application is being shown. Taken
    // as a switch takes it, which needs no process down; the archive takes none of its own, so
    // the two do not meet.
    let claim = state.begin_switch(id)?;
    let session = state.session_record(id)?;
    crate::accounts::shared_config::refuse_if_changing(state, session.account_id.as_deref())?;
    let archived = session.status == SessionStatus::Archived && state.live_session(id).is_none();
    let had_process = state.live_session(id).is_some() || !session.status.has_no_process();
    if had_process {
        refuse_if_at_work(state, &session)?;
    }
    let (Some(scheme), Some(app)) = (
        scheme_of(session.agent),
        provider(session.agent).native_app(),
    ) else {
        return Err(unavailable(&format!(
            "{} has no desktop application",
            session.agent.label()
        )));
    };
    let Some(conversation) = crate::records::resumability::resumable_agent_session_id(&session)
    else {
        return Err(unavailable("this session has no conversation yet"));
    };
    let conversation = super::resume::parse_conversation_id(&conversation)
        .map_err(|err| unavailable(&format!("{err:#}")))?;
    let cwd = super::launch::session_cwd(state, &session)?;
    let scratch = crate::paths::session_scratch(&session.id);
    // Both before anything is archived: a refusal for either leaves everything as it was.
    let shell = tokio::task::spawn_blocking(move || {
        if !installed(scheme) {
            return Ok(None);
        }
        snapshot()
            .map(Some)
            .map_err(crate::env_shell::capture_failed)
    })
    .await??;
    let Some(shell) = shell else {
        return Err(unavailable(&format!(
            "the {} application is not installed",
            session.agent.label()
        )));
    };
    if !archived {
        archive_and_wait(state, id, had_process, keep_process_ids).await?;
    }
    let opened = tokio::task::spawn_blocking(move || {
        ask(app, &session, &conversation, &cwd, &scratch, &shell)
    })
    .await?;
    drop(claim);
    opened
}

/// Refuses, with `session_working`, a session that is at work ([`AppState::what_it_is_doing`]).
fn refuse_if_at_work(state: &AppState, session: &Session) -> Result<()> {
    match state.what_it_is_doing(session) {
        Some(doing) => Err(CodedError::raised(
            error_code::SESSION_WORKING,
            format!(
                "this session is {doing}, and its conversation cannot be opened in the app while \
                 it is: open it once it is waiting for instructions or interrupted"
            ),
            &[("session", &session.id)],
        )),
        None => Ok(()),
    }
}

/// Archives `id` and what is bound to it for an open, keeping the trees in `keep_process_ids` (the
/// rest of what the process started is stopped by the exit path, and not carried over, since
/// nothing relaunches it), and, for a session that `had_process`, waits until that is gone.
async fn archive_and_wait(
    state: &Arc<AppState>,
    id: &str,
    had_process: bool,
    keep_process_ids: Vec<String>,
) -> Result<()> {
    // Counted by the handover barrier from before anything is kept until the archive is made.
    let admission = state.admit_change()?;
    {
        // Held until the archive is made, and not through the wait for the process, so that no
        // lock is set between the check below and the archive, which holds a claim of its own:
        // the claims count, so they nest.
        let _removal = state.claim_session_removal();
        // Before anything is kept, so that a refusal for a lock leaves the trees as they were.
        super::lifecycle::ensure_group_unlocked(state, id)?;
        if had_process {
            super::processes::keep_chosen(state, id, keep_process_ids).await?;
            // The judgement of a destructive act is made after the last wait, with nothing
            // awaited between it and the archive: a turn may have begun while the trees were
            // being kept, which then stay kept.
            let session = state.session_record(id)?;
            if state.live_session(id).is_some() || !session.status.has_no_process() {
                refuse_if_at_work(state, &session)?;
            }
        }
        super::lifecycle::archive_keeping_branches(state, &admission, id)?;
    }
    if !had_process {
        return Ok(());
    }
    super::launch::wait_until_stopped(state, id).await.map_err(|err| {
        match err.downcast_ref::<CodedError>() {
            Some(coded) if coded.code == error_code::SESSION_DID_NOT_STOP => CodedError::raised(
                error_code::SESSION_DID_NOT_STOP,
                "the session's process did not end in time, so its conversation was not opened in the app; the session is archived",
                &[("session", id), ("cause", "open_in_app")],
            ),
            _ => err,
        }
    })
}

/// Whether the default account's directory, the one both applications read, holds the latest record
/// of `session`'s conversation: the record the agent's store names as current there exists, and
/// either the session is of the default account, or its own account's directory holds no record of
/// its own, or that record is the very file the default one is (reached through links), or the
/// default copy is neither shorter nor older than the account's. Blocking.
pub fn default_directory_holds_latest(locator: &mut Locator, session: &Session) -> bool {
    let Some(id) = crate::records::resumability::resumable_agent_session_id(session) else {
        return false;
    };
    let agent = session.agent;
    let Some(default_root) = locator.root(agent, None) else {
        return false;
    };
    let Some(default) = locator.current_record(agent, &default_root, &id) else {
        return false;
    };
    let Some(own_dir) = session.config_dir.as_deref() else {
        return true;
    };
    let Some(own_root) = locator.root(agent, Some(own_dir)) else {
        return false;
    };
    let Some(own) = locator.current_record(agent, &own_root, &id) else {
        return true;
    };
    if crate::hostfs::is_same_path(&default, &own) {
        return true;
    }
    match (std::fs::metadata(&default), std::fs::metadata(&own)) {
        (Ok(default), Ok(own)) => {
            default.len() >= own.len()
                && default
                    .modified()
                    .ok()
                    .zip(own.modified().ok())
                    .is_some_and(|(default, own)| default >= own)
        }
        _ => false,
    }
}

/// What a `probe_native_apps` found out.
pub struct Probe {
    /// The agents whose desktop application is installed.
    pub agents: Vec<AgentId>,
    /// The asked sessions whose conversation the applications can see
    /// ([`default_directory_holds_latest`]).
    pub seen: Vec<String>,
    /// The asked sessions judged not to be. A session asked about and in neither list was not
    /// judged: an unknown id, one of an agent with no application, one past
    /// [`PROBED_SESSIONS_LIMIT`], or any when the shell environment could not be captured.
    pub unseen: Vec<String>,
}

/// Which agents' applications are installed and, for `sessions`, whether the applications can see
/// each conversation. A failed capture of the shell environment (which says where the default
/// directories are) judges none of the sessions but still answers the agents. The judgement walks
/// the record directories of the agents the sessions are of once per call, not once per session,
/// so a probe of a page of rows costs one walk; it is not free for a long history. Blocking.
pub fn probe(state: &Arc<AppState>, sessions: &[String]) -> Result<Probe> {
    let mut probe = Probe {
        agents: installed_agents(),
        seen: Vec::new(),
        unseen: Vec::new(),
    };
    if sessions.is_empty() {
        return Ok(probe);
    }
    let Ok(shell) = crate::env_shell::cached_snapshot() else {
        return Ok(probe);
    };
    let mut locator = Locator::with_shell(shell);
    for id in sessions.iter().take(PROBED_SESSIONS_LIMIT) {
        let Some(session) = state.store.get_session(id)? else {
            continue;
        };
        if scheme_of(session.agent).is_none() {
            continue;
        }
        if default_directory_holds_latest(&mut locator, &session) {
            probe.seen.push(session.id);
        } else {
            probe.unseen.push(session.id);
        }
    }
    Ok(probe)
}

#[cfg(test)]
mod tests;
