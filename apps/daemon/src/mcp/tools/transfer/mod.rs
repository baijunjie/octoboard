//! The tools with which an owner moves the responsibility for git branches (`transfer_branches`) or
//! process trees (`transfer_processes`) between sessions: from itself to a session of its own,
//! from one to itself, or from one of its sessions to another.
//!
//! What the two share is here: the two sessions read under their claims (`Parties`), telling them
//! what moved, and refusing a `from` in the middle of a turn. So are the checks on the branches
//! named, which `open_session` and `request_session` make too.

use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use serde_json::{Map, Value};

use crate::branch_origin;
use crate::branch_origin::transfer::{named_branches, BranchTransfer};
use crate::git::repo_files::repository_of;
use crate::mcp::lookup::{optional_string, optional_string_list, resolve_owned_session};
use crate::messaging::delivery::{self, WhenBlocked};
use crate::messaging::naming::named_session;
use crate::protocol::{error_code, CodedError, Project, Session, SessionStatus};
use crate::state::{AppState, LaunchClaim, OutboxHold};
use crate::wire_path;

mod branches;
mod processes;

pub(in crate::mcp) use branches::transfer_branches;
pub(in crate::mcp) use processes::transfer_processes;

/// The names in the list argument `key`, each trimmed, as wire paths and named once. `None` when
/// the argument is absent or names none; a blank name is refused.
pub(super) fn branch_names(
    arguments: &Map<String, Value>,
    key: &str,
) -> Result<Option<Vec<String>>> {
    let Some(names) = optional_string_list(arguments, key)? else {
        return Ok(None);
    };
    let mut branches: Vec<String> = Vec::new();
    for name in &names {
        let name = name.trim();
        if name.is_empty() {
            bail!("`{key}` must not hold a blank name");
        }
        let wire = wire_path::encode(name.as_bytes());
        if !branches.contains(&wire) {
            branches.push(wire);
        }
    }
    Ok((!branches.is_empty()).then_some(branches))
}

/// The repository `project` is in and `branches` (wire paths) of it, checked to be all `holder`'s
/// to give. The refusal speaks to `caller`: of its own branches when it is the holder, else of the
/// holder's.
pub(super) async fn held_by(
    state: &Arc<AppState>,
    caller: &Session,
    holder: &Session,
    project: &Project,
    branches: Vec<String>,
) -> Result<BranchTransfer> {
    // Finding the repository reads the filesystem and the holder check reads the store.
    let (repository, not_held, targets) = {
        let state = state.clone();
        let path = project.path.clone();
        let (branches, holder) = (branches.clone(), holder.id.clone());
        tokio::task::spawn_blocking(move || {
            let Some(repository) = repository_of(Path::new(&path)) else {
                return Ok(None);
            };
            let not_held =
                branch_origin::first_not_held_by(&state, &repository, &branches, &holder)?;
            let targets = branch_origin::targets_in(&state, &repository, &branches);
            Ok::<_, anyhow::Error>(Some((repository, not_held, targets)))
        })
        .await??
        .ok_or_else(|| {
            anyhow!(
                "the project `{}` (\"{}\") is not in a git repository, so there is no branch to \
                 hand to a session there",
                project.id,
                project.name
            )
        })?
    };
    if let Some(branch) = not_held {
        let place = format!(
            "in the repository of the project `{}` (\"{}\"): it does not exist there, or it was \
             not created by",
            project.id, project.name
        );
        let branch = named_branches(&[branch]);
        if holder.id == caller.id {
            bail!(
                "{branch} is not a branch you are responsible for {place} you or transferred to \
                 you, or another session is responsible for it now. Only a branch you are \
                 responsible for can be transferred"
            );
        }
        bail!(
            "{branch} is not a branch {holder} is responsible for {place} that session or \
             transferred to it, or another session is responsible for it now. Only a branch \
             that session is responsible for can be transferred",
            holder = named_session(holder)
        );
    }
    Ok(BranchTransfer {
        repository,
        branches,
        targets,
    })
}

/// The session `key` names — the caller itself when it is left out — as the caller may act on
/// it: itself, or a session it owns.
fn party(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
    key: &str,
) -> Result<Session> {
    match optional_string(arguments, key) {
        Some(wanted) if wanted != caller.id => resolve_owned_session(state, caller, &wanted),
        _ => state.session_record(&caller.id),
    }
}

/// The two sessions of a transfer, `from` and `to` (each the caller or a session it owns), read
/// under claims that keep a relaunch, a switch or a handoff off each one that is not the caller,
/// with the messages of a `from` other than the caller held so that none starts a turn while the
/// transfer is checked again and made. The claims last as long as this does.
struct Parties {
    from: Session,
    to: Session,
    // Declared before the claims, which drop in declaration order: the held messages are released
    // first, while the sessions are still claimed.
    hold: Option<OutboxHold>,
    _to_claim: Option<LaunchClaim>,
    _from_claim: Option<LaunchClaim>,
}

impl Parties {
    /// Reads `from` and `to` from `arguments`, refusing one session as both, then claims them and
    /// reads them again: everything read before the claims can be stale by the time anything
    /// moves.
    fn claim(
        state: &Arc<AppState>,
        caller: &Session,
        arguments: &Map<String, Value>,
    ) -> Result<Self> {
        let from_id = party(state, caller, arguments, "from")?.id;
        let to_id = party(state, caller, arguments, "to")?.id;
        if from_id == to_id {
            bail!(
                "`from` and `to` are the same session, `{from_id}`; there is nothing to transfer"
            );
        }
        let _from_claim = (from_id != caller.id)
            .then(|| state.begin_switch(&from_id))
            .transpose()?;
        let _to_claim = (to_id != caller.id)
            .then(|| state.begin_switch(&to_id))
            .transpose()?;
        let hold = (from_id != caller.id).then(|| state.hold_outbox(&from_id));
        Ok(Self {
            from: party(state, caller, arguments, "from")?,
            to: party(state, caller, arguments, "to")?,
            hold,
            _to_claim,
            _from_claim,
        })
    }

    /// Tells `from` and `to` what moved, `messages` holding what each is told in that order,
    /// unless one is the caller, which knows from the result; then lets `from`'s held messages go.
    /// `what` names the transfer in the log.
    ///
    /// Returns the sessions that could not be told, as they are on record now, for the result to
    /// name ([`untold_note`]): one with no process, or one that took no message. The records are
    /// right either way.
    async fn tell(
        &mut self,
        state: &Arc<AppState>,
        caller: &Session,
        messages: [String; 2],
        what: &str,
    ) -> Vec<Session> {
        let mut untold = Vec::new();
        for (session, text) in [&self.from, &self.to].into_iter().zip(messages) {
            if session.id == caller.id {
                continue;
            }
            // Read again: the status in hand is from before the move.
            let current = state
                .store
                .get_session(&session.id)
                .ok()
                .flatten()
                .unwrap_or_else(|| session.clone());
            if current.status.has_no_process() {
                untold.push(current);
                continue;
            }
            if let Err(err) =
                delivery::write_message_off_runtime(state, &session.id, text, WhenBlocked::Queue)
                    .await
            {
                tracing::warn!(
                    session = %session.id,
                    %err,
                    "telling a session of a {what} transfer failed"
                );
                untold.push(current);
            }
        }
        drop(self.hold.take());
        untold
    }
}

/// What a transfer's result says of each session in `untold`, which [`Parties::tell`] could not
/// tell: that the caller is to.
fn untold_note(untold: &[Session]) -> String {
    untold
        .iter()
        .map(|session| {
            format!(
                " Octoboard could not send {} a message about the transfer: tell it yourself \
                 once it can take a message.",
                named_session(session)
            )
        })
        .collect()
}

/// Refuses `from` when it is in the middle of a turn — working, starting, or held up for the user
/// (a permission prompt, a question, a session request waiting) — since such a session goes on
/// using what it would be asked to give up, as `why` says after "a session in the middle of a
/// turn". Both the record and the status the agent is at count: a prompt the agent shows (Codex's
/// "Action Required" terminal title) can be seen before the record has caught up with it.
fn refuse_if_at_work(state: &Arc<AppState>, from: &Session, why: &str) -> Result<()> {
    let Some(busy) = busy_phrase(state, from) else {
        return Ok(());
    };
    Err(CodedError::raised(
        error_code::SESSION_WORKING,
        format!(
            "{} is {busy}: a session in the middle of a turn {why}",
            named_session(from),
        ),
        &[("session", &from.id)],
    ))
}

/// How `session` is occupied when it is in the middle of a turn, as a phrase after "is", or
/// `None` when it is not: working, starting, or held up for the user. Both the record and the
/// status the agent is at count.
fn busy_phrase(state: &Arc<AppState>, session: &Session) -> Option<&'static str> {
    [session.status, state.agent_status(session)]
        .into_iter()
        .find_map(|status| match status {
            SessionStatus::Working => Some("in the middle of a turn"),
            SessionStatus::Starting => Some("still starting"),
            SessionStatus::WaitingUser => Some("waiting for the user, so in the middle of a turn"),
            _ => None,
        })
}
