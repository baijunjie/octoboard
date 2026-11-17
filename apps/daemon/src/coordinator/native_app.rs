//! Opening a session's conversation in its agent's own desktop application. Which agents have one
//! is the descriptors' (`Capabilities::native_app_scheme`, what the host is asked about) and how
//! one is asked to show a conversation the provider's (`AgentProvider::native_app`); this only
//! checks the session and the host. The application and Octoboard would both write to a
//! conversation a live process holds, so the session must have none: nothing is stopped on the
//! user's behalf. The applications read the default account's directory only, so one whose latest
//! record that directory does not hold may not show in them ([`default_directory_holds_latest`]);
//! that is told to the client (`probe_native_apps`, [`probe`]), which warns, and does not keep it
//! from being opened.

use std::sync::Arc;

use anyhow::Result;

use crate::adapter::{native_app_unavailable as unavailable, provider, NativeAppOpen};
use crate::platform::open::native_app::url_scheme_handled;
use crate::protocol::{error_code, AgentId, CodedError, Session};
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

/// Opens `id`'s conversation in its agent's desktop application. Refused with
/// `session_already_running` unless the session has no process, with `session_already_starting`
/// while a launch or another operation holds it, with `account_sharing_in_progress` while a change
/// of its account's sharing moves the account's directory, and with `native_app_unavailable` for an
/// agent with no application, one that is not installed, a session with no conversation yet, a
/// conversation id the agent would not take and a launch that fails. A refusal before the
/// application is asked leaves everything as it was: the session's pending recoveries are cancelled
/// (the user now has the conversation) only once it is about to be.
pub async fn open_session_in_app(state: &Arc<AppState>, id: &str) -> Result<()> {
    // Held until the open has ended: a resume, a switch or a launch begun meanwhile would start a
    // process on the conversation the application is being shown.
    let claim = state.begin_launch_leaving_recoveries(id)?;
    let session = state.session_record(id)?;
    if !session.status.has_no_process() {
        return Err(CodedError::raised(
            error_code::SESSION_ALREADY_RUNNING,
            "this session is not interrupted or archived",
            &[("session", id)],
        ));
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
    let state = Arc::clone(state);
    let opened = tokio::task::spawn_blocking(move || {
        if !url_scheme_handled(scheme) {
            return Err(unavailable(&format!(
                "the {} application is not installed",
                session.agent.label()
            )));
        }
        let shell =
            crate::env_shell::cached_snapshot().map_err(crate::env_shell::capture_failed)?;
        state.cancel_recoveries(&session.id);
        app.open(&NativeAppOpen {
            session: &session,
            conversation: &conversation,
            cwd: &cwd,
            shell: &shell,
            scratch: &scratch,
        })
    })
    .await?;
    drop(claim);
    opened
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
