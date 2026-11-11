//! Resuming a session, and locating the outside conversation a new session continues.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::{Context, Result};
use uuid::Uuid;

use crate::accounts::relocate;
use crate::adapter;
use crate::env_shell;
use crate::protocol::{
    error_code, Account, AgentId, CodedError, ConversationLocation, Session, SessionKind,
    SessionStatus, TerminalAppearance,
};
use crate::records;
pub(crate) use crate::records::resumability::resumable_agent_session_id;
use crate::session::term;
use crate::state::AppState;
use crate::text::trimmed_or_none;

use super::launch::{checked_launch_overrides, relaunch_session_with, LaunchEdit, LaunchOverrides};
use super::lifecycle::trash_agent_records_with;
use crate::accounts::choice::{account_dir, agent_can_open};

/// The conversation id `requested` to resume, trimmed, or `None` for none (blank counting as
/// none), refused before anything is launched or written. A console session takes none, and a
/// session reporting to another takes none: a resumed session is a plain project session. The id
/// must be a UUID, which every agent's conversation ids are (Grok reads anything else as a
/// title), and no session may record it already, in any project, because two processes must
/// never share one transcript.
pub(super) fn checked_resume_id(
    state: &Arc<AppState>,
    kind: SessionKind,
    project_id: Option<&str>,
    bound_to: Option<&str>,
    requested: Option<String>,
) -> Result<Option<String>> {
    let Some(id) = trimmed_or_none(requested.as_deref()) else {
        return Ok(None);
    };
    if kind == SessionKind::Console {
        return Err(CodedError::raised(
            error_code::RESUME_FOR_CONSOLE_SESSION,
            "a console session cannot resume a conversation",
            &[],
        ));
    }
    if bound_to.is_some() {
        return Err(CodedError::raised(
            error_code::CONFLICTING_FIELDS,
            "`resume_agent_session_id` and `bound_to` cannot both be given",
            &[("first", "resume_agent_session_id"), ("second", "bound_to")],
        ));
    }
    let id = parse_conversation_id(&id)?;
    refuse_if_conversation_recorded(state, &id, project_id, None)?;
    Ok(Some(id))
}

/// `id` as the agents write a conversation id, lowercase and hyphenated, which is how a record is
/// looked for and stored however the user typed it; refused with `resume_id_invalid` when it is no
/// UUID.
pub(crate) fn parse_conversation_id(id: &str) -> Result<String> {
    match Uuid::parse_str(id) {
        Ok(parsed) => Ok(parsed.hyphenated().to_string()),
        Err(_) => Err(CodedError::raised(
            error_code::RESUME_ID_INVALID,
            format!("`{id}` is not a conversation id: it must be a UUID"),
            &[("id", id)],
        )),
    }
}

/// Refuses `id` when a session other than `except` records it as its agent's conversation id, in
/// any project and in any state: two processes must never share one transcript. A session of
/// `project_id` is named so the user can resume it instead; any other, a console session
/// included, is said to be outside the project, with its id for the UI to name.
pub(super) fn refuse_if_conversation_recorded(
    state: &Arc<AppState>,
    id: &str,
    project_id: Option<&str>,
    except: Option<&str>,
) -> Result<()> {
    let Some(holder) = state.store.session_by_agent_session_id(id, except)? else {
        return Ok(());
    };
    Err(if holder.project_id.as_deref() == project_id {
        CodedError::raised(
            error_code::CONVERSATION_IN_SESSION,
            format!(
                "the session \"{}\" already holds this conversation: resume or reopen it instead",
                holder.title
            ),
            &[("id", id), ("session", &holder.id)],
        )
    } else {
        CodedError::raised(
            error_code::CONVERSATION_OUTSIDE_PROJECT,
            "this conversation already belongs to a session outside this project",
            &[("id", id), ("session", &holder.id)],
        )
    })
}

/// Refuses a conversation to resume that has no record in the account directory the session will
/// launch with: the pinned one, else the default account's, resolved from the shell snapshot.
/// Answers that snapshot (none for a pinned account) for the launch to use, so the check and the
/// launch cannot disagree about where the default account is. Blocking.
pub(super) fn locate_conversation_to_resume(
    agent: AgentId,
    config_dir: Option<&str>,
    id: &str,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>>,
) -> Result<Option<HashMap<String, String>>> {
    let shell_env = match config_dir {
        Some(_) => None,
        None => Some(snapshot().context("snapshotting the user's shell environment")?),
    };
    let dir = account_dir(agent, config_dir, shell_env.as_ref());
    relocate::locate(agent, &dir, id).map_err(|err| match err.downcast::<CodedError>() {
        Ok(coded) if coded.code == error_code::CONVERSATION_NOT_FOUND => CodedError::raised(
            error_code::CONVERSATION_TO_RESUME_NOT_FOUND,
            format!(
                "no conversation `{id}` of {} was found in `{}`",
                agent.label(),
                dir.display()
            ),
            &[
                ("agent", agent.label()),
                ("path", &dir.to_string_lossy()),
                ("id", id),
            ],
        ),
        Ok(coded) => anyhow::Error::new(coded),
        Err(err) => err,
    })?;
    Ok(shell_env)
}

/// Where the conversation `agent_session_id` is held, for the project `project` asking, as the
/// new-session dialog's Resume tab and the `request_resume_session` tool both ask it: a session
/// that records it first, whatever its agent or project, else the on-disk record in the default
/// account and every account of each agent that is available and added. Refused with
/// `unknown_project` for a `project` that does not exist and `resume_id_invalid` for an id that is
/// no UUID. Reads the disk only.
pub(crate) async fn locate_conversation(
    state: &Arc<AppState>,
    project: &str,
    agent_session_id: &str,
) -> Result<ConversationLocation> {
    state.store.require_project(project)?;
    let id = parse_conversation_id(agent_session_id)?;
    if let Some(holder) = state.store.session_by_agent_session_id(&id, None)? {
        return Ok(held_conversation(&holder, project));
    }
    let mut agents = Vec::new();
    for agent in AgentId::ALL {
        if agent_can_open(state, agent)? {
            agents.push(agent);
        }
    }
    let accounts = state.store.list_accounts()?;
    Ok(tokio::task::spawn_blocking(move || {
        find_conversation_to_resume(&agents, &accounts, &id, env_shell::launch_snapshot)
    })
    .await??
    .unwrap_or(ConversationLocation::NotFound))
}

/// Where a conversation is held that `holder` records, for the project `project` asking: an
/// archived session of it can be reopened, any other session of it is running already, and one
/// outside it (another project's, or a console session) belongs elsewhere whatever its state.
fn held_conversation(holder: &Session, project: &str) -> ConversationLocation {
    if holder.project_id.as_deref() != Some(project) {
        ConversationLocation::OutsideProject {
            session: holder.id.clone(),
        }
    } else if holder.status == SessionStatus::Archived {
        ConversationLocation::ArchivedSession {
            session: holder.id.clone(),
        }
    } else {
        ConversationLocation::LiveSession {
            session: holder.id.clone(),
        }
    }
}

/// Where the record of the conversation `id` is among the accounts a session could be opened with:
/// for each of `agents`, its default account (resolved from the shell snapshot, taken only if
/// there is one to look in) and each of its `accounts`, in the directory
/// [`locate_conversation_to_resume`] would look in. When more than one holds it, the account whose
/// record was written to last, since that is the conversation that went on, as a
/// [`ConversationLocation::Record`] (`None` for the default account, as in a session's own
/// account). A record's age is the modification time of what [`relocate::locate`] names, which for
/// Grok Build is the session's directory, so only approximately when the last write was inside it.
/// A snapshot that fails leaves the default accounts out, so as not to hide a stored account's
/// record, and is the error only when nothing was found anywhere. Blocking.
fn find_conversation_to_resume(
    agents: &[AgentId],
    accounts: &[Account],
    id: &str,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>>,
) -> Result<Option<ConversationLocation>> {
    let mut snapshot = Some(snapshot);
    let mut shell_env: Option<HashMap<String, String>> = None;
    let mut snapshot_error = None;
    let mut newest: Option<(SystemTime, ConversationLocation)> = None;
    let mut locator = crate::records::Locator::default();
    for &agent in agents {
        let default = (None, None);
        let pinned = accounts
            .iter()
            .filter(|account| account.agent == agent)
            .map(|account| (Some(account.id.clone()), Some(account.config_dir.as_str())));
        for (account, config_dir) in std::iter::once(default).chain(pinned) {
            if config_dir.is_none() {
                if let Some(take) = snapshot.take() {
                    match take() {
                        Ok(env) => shell_env = Some(env),
                        Err(err) => {
                            snapshot_error =
                                Some(err.context("snapshotting the user's shell environment"))
                        }
                    }
                }
                if shell_env.is_none() {
                    continue;
                }
            }
            let dir = account_dir(agent, config_dir, shell_env.as_ref());
            let record = match relocate::locate_with(&mut locator, agent, &dir, id) {
                Ok(record) => record,
                Err(err) => match err.downcast::<CodedError>() {
                    Ok(coded) if coded.code == error_code::CONVERSATION_NOT_FOUND => continue,
                    Ok(coded) => return Err(anyhow::Error::new(coded)),
                    Err(err) => return Err(err),
                },
            };
            let written = crate::hostfs::modified(&dir.join(record));
            if newest.as_ref().is_none_or(|(latest, _)| written > *latest) {
                newest = Some((written, ConversationLocation::Record { agent, account }));
            }
        }
    }
    match (newest, snapshot_error) {
        (Some((_, location)), _) => Ok(Some(location)),
        (None, Some(err)) => Err(err),
        (None, None) => Ok(None),
    }
}

/// Relaunches a session without a process. `instruction` is given to it here rather than by the
/// caller: as the launch prompt for a Claude Code session that resumes a conversation, otherwise
/// queued to be written once it is running — whichever hook releases it can fire the moment the
/// agent starts, so it has to be in the queue before the launch and out again if the launch never
/// happens.
///
/// A session bound to an archived owner (a console session, or a project session that started it)
/// brings its archived owners back first, level by level from the topmost down — a session bound to
/// an archived bound coordinator brings back the bound coordinator's archived console session, then
/// the bound coordinator — so what it reports reaches a process. The walk stops at the first owner
/// that is not archived; one that is only interrupted is not relaunched. If an owner cannot be
/// relaunched the whole resume fails with its reason: the owners already brought back stay
/// reopened, and the rest, the session included, stay as they were. Reopening an owner reopens
/// nothing bound to it, and no other session bound to the same owners is resumed: the group comes
/// back one session at a time, from the one the user asked for.
///
/// Before anything is relaunched, the session and each owner about to be launched are checked for a
/// conversation the agent can still resume ([`check_resumable`]). The session itself is refused
/// with `conversation_unresumable` when its conversation is proven gone, with nothing changed; an
/// owner whose conversation is proven gone is relaunched into a fresh conversation instead. A
/// record that sits only where its agent archives records is brought back first
/// ([`unarchive_record`]), the session's own before any owner is launched. An owner's leftover
/// records of the conversation it gives up go to the Trash. The login-shell snapshot is taken once
/// and serves the checks, the unarchive and every launch.
pub async fn resume_session(
    state: &Arc<AppState>,
    id: &str,
    instruction: Option<&str>,
    terminal_appearance: Option<TerminalAppearance>,
) -> Result<()> {
    resume_session_with(
        state,
        id,
        instruction,
        terminal_appearance,
        env_shell::launch_snapshot,
    )
    .await
}

/// [`resume_session`] with the login-shell snapshot supplied by the caller. It is taken once and
/// serves the resumability checks, any unarchive and every relaunch, so all of them look at
/// the same accounts; tests pass a stub so no real login shell runs.
pub(super) async fn resume_session_with(
    state: &Arc<AppState>,
    id: &str,
    instruction: Option<&str>,
    terminal_appearance: Option<TerminalAppearance>,
    shell_snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
) -> Result<()> {
    resume_session_with_launch(
        state,
        id,
        instruction,
        terminal_appearance,
        LaunchEdit::default(),
        shell_snapshot,
        term::launch,
    )
    .await
}

/// [`resume_session_with`] with the model and reasoning effort the session is resumed with, as an
/// owner resuming it over MCP asks, and the process start `launch`, called once for each session
/// relaunched. They are checked against what the session's agent offers under its own account
/// before anything is launched, recorded as the session's own just before its relaunch, and put
/// back when the relaunch fails. Any archived owner reopened with it keeps its settings.
///
/// The login-shell snapshot and `launch` are supplied by the caller: `env_shell::launch_snapshot`
/// and `term::launch` outside the tests, which cannot start an agent.
pub(crate) async fn resume_session_with_launch(
    state: &Arc<AppState>,
    id: &str,
    instruction: Option<&str>,
    terminal_appearance: Option<TerminalAppearance>,
    launch_edit: LaunchEdit,
    shell_snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
    launch: impl Fn(term::LaunchRequest) -> Result<term::Launch> + Clone + Send + 'static,
) -> Result<()> {
    // Claimed before anything else, so two overlapping relaunches cannot both get through, and
    // before the record is read, so a delete cannot slip in between reading and launching.
    let claim = state.begin_launch(id)?;
    // Taken once: the checks, the unarchive and every launch of this resume see the one
    // environment, and a default account is resolved from the same shell the agent will run in.
    let shell = tokio::task::spawn_blocking(shell_snapshot).await??;
    let owners = archived_owners(state, id)?;
    // Every session this resume would launch is checked before the first is, so a target whose
    // record is gone leaves the whole group as it was instead of reopening the owners above it.
    let mut checked_owners = Vec::new();
    for owner in &owners {
        checked_owners.push(check_resumable(state, owner, &shell).await?);
    }
    let target = check_resumable(state, id, &shell).await?;
    if target.judgement.resumability == records::Resumability::Unresumable {
        return Err(conversation_unresumable(&target.session));
    }
    // Refused for a value the agent does not offer before anything is launched or recorded.
    let overrides = checked_launch_overrides(
        state,
        &target.session,
        target.session.account_id.clone(),
        launch_edit,
    )
    .await?;
    // Before any owner is launched, so a failed unarchive leaves the whole group as it was.
    unarchive_record(&target, &shell).await?;
    for checked in checked_owners {
        let owner = checked.session.id.clone();
        // The owners were listed before the awaits of the earlier relaunches, so a resume of a
        // sibling bound session may have brought this one back since: skip it unless it still
        // needs reopening. Two resumes can both read an owner as archived; the launch claim lets
        // one through and refuses the other, and the user's next click goes through.
        if !needs_reopening(state, &owner)? {
            continue;
        }
        let owner_claim = state.begin_launch(&owner)?;
        // The verdict is acted on destructively, so it must still be about the conversation the
        // owner has now: a hook may have recorded a new one since it was judged.
        let current = state.session_record(&owner)?;
        let checked = if current.agent_session_id != checked.session.agent_session_id
            || current.has_conversation != checked.session.has_conversation
        {
            check_resumable(state, &owner, &shell).await?
        } else {
            checked
        };
        if checked.judgement.resumability == records::Resumability::Unresumable {
            forget_gone_conversation(state, &checked.session, &shell).await?;
        } else {
            unarchive_record(&checked, &shell).await?;
        }
        relaunch_session_with(
            state,
            &owner_claim,
            &owner,
            None,
            true,
            Some(shell.clone()),
            None,
            launch.clone(),
        )
        .await?;
    }
    if let Some(overrides) = &overrides {
        overrides.record(state, id)?;
    }
    let relaunched = relaunch_session_with(
        state,
        &claim,
        id,
        instruction,
        true,
        Some(shell),
        terminal_appearance,
        launch,
    )
    .await;
    if relaunched.is_err() && overrides.is_some() {
        // A resume that fails leaves the session as it was, the model and effort included.
        let before = &target.session;
        let put_back = LaunchOverrides {
            model: before.model.clone(),
            effort: before.effort.clone(),
            compatibility: before.compatibility.clone(),
        };
        if let Err(revert) = put_back.record(state, id) {
            tracing::warn!(
                session = %id,
                %revert,
                "putting the session's model, effort and compatibility switches back failed"
            );
        }
    }
    relaunched
}

/// Refuses, before a request to resume `id` is put to the user, what [`resume_session`] would
/// refuse for a conversation its agent no longer has (`conversation_unresumable`), with nothing
/// changed. Returns the archived owners a resume brings back with it, topmost first.
pub(crate) async fn check_can_resume(state: &Arc<AppState>, id: &str) -> Result<Vec<String>> {
    check_can_resume_with(state, id, env_shell::launch_snapshot).await
}

/// [`check_can_resume`] with the login-shell snapshot supplied by the caller, which tests stub.
pub(crate) async fn check_can_resume_with(
    state: &Arc<AppState>,
    id: &str,
    shell_snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
) -> Result<Vec<String>> {
    let shell = tokio::task::spawn_blocking(shell_snapshot).await??;
    let owners = archived_owners(state, id)?;
    let target = check_resumable(state, id, &shell).await?;
    if target.judgement.resumability == records::Resumability::Unresumable {
        return Err(conversation_unresumable(&target.session));
    }
    Ok(owners)
}

/// A session and what [`check_resumable`] found of its conversation.
struct Checked {
    session: Session,
    judgement: records::Judgement,
}

/// Looks for the conversation record of a session without a process in its own account, resolving a
/// default account through `shell`, the snapshot the launch will use. Only a proven absence is
/// [`records::Resumability::Unresumable`]; anything unknown launches as it always has, and a
/// session that has a process is not looked at.
async fn check_resumable(
    state: &Arc<AppState>,
    id: &str,
    shell: &HashMap<String, String>,
) -> Result<Checked> {
    let session = state.session_record(id)?;
    if !session.status.has_no_process() {
        return Ok(Checked {
            session,
            judgement: records::Judgement {
                resumability: records::Resumability::Resumable,
                needs_unarchive: false,
            },
        });
    }
    let shell = shell.clone();
    tokio::task::spawn_blocking(move || {
        let judgement = records::judge(&records::Locator::with_shell(shell), &session);
        Checked { session, judgement }
    })
    .await
    .map_err(Into::into)
}

/// Gives up the conversation of `session`, whose record the agent no longer has, so its next
/// launch opens a fresh one: what is still left of it on disk goes to the Trash first, while the
/// session still names it.
async fn forget_gone_conversation(
    state: &Arc<AppState>,
    session: &Session,
    shell: &HashMap<String, String>,
) -> Result<()> {
    trash_agent_records_with(
        state,
        vec![session.clone()],
        records::Locator::with_shell(shell.clone()),
        state.host_tools.get().trash,
    )
    .await;
    state.forget_conversation(&session.id)
}

/// The refusal of a resume whose session the agent can no longer resume: launching it would only
/// make the agent exit, and a reopen would move an archived session out of the archive for it.
fn conversation_unresumable(session: &Session) -> anyhow::Error {
    CodedError::raised(
        error_code::CONVERSATION_UNRESUMABLE,
        format!(
            "{} no longer has the conversation of this session, so it cannot be resumed",
            session.agent.label()
        ),
        &[("agent", session.agent.label()), ("session", &session.id)],
    )
}

/// Brings back the record of a checked session that only sits where its agent archives records,
/// which a resume does not read, through its provider's store (`RecordStore::unarchive`). A
/// failure leaves the session untouched: this runs before its status is written.
async fn unarchive_record(checked: &Checked, shell: &HashMap<String, String>) -> Result<()> {
    if !checked.judgement.needs_unarchive {
        return Ok(());
    }
    let (session, shell) = (checked.session.clone(), shell.clone());
    tokio::task::spawn_blocking(move || {
        let agent_session_id = session.agent_session_id.as_deref().unwrap_or_default();
        adapter::provider(session.agent).records().unarchive(
            &shell,
            session.config_dir.as_deref().map(Path::new),
            agent_session_id,
        )
    })
    .await?
}

/// Whether an owner listed by [`archived_owners`] is still to be relaunched by a reopen: it is
/// archived, or a launch of it is in flight (its claim is held). That launch writes the owner's
/// status before its process is up and puts it back to archived if it fails, so it is left to the
/// launch claim, which refuses this reopen rather than letting it start sessions under an owner
/// that may not stay up. An owner that is neither (brought back since, with its process up, or only
/// interrupted) is skipped, and one that is gone is left to fail on its claim.
fn needs_reopening(state: &Arc<AppState>, owner: &str) -> Result<bool> {
    Ok(match state.store.get_session(owner)? {
        Some(record) => record.status == SessionStatus::Archived || state.is_launching(owner),
        None => true,
    })
}

/// The archived owners a session without a process brings back before itself, topmost first: its
/// owner when that is archived, then that owner's owner when that is archived too, stopping at the
/// first that is not (or is not on record). Empty for a session that has a process.
pub(crate) fn archived_owners(state: &Arc<AppState>, id: &str) -> Result<Vec<String>> {
    let session = state.session_record(id)?;
    let mut owners = Vec::new();
    let mut next = session.bound_to.filter(|_| session.status.has_no_process());
    while let Some(owner_id) = next {
        let Some(owner) = state
            .store
            .get_session(&owner_id)?
            .filter(|owner| owner.status == SessionStatus::Archived)
        else {
            break;
        };
        // Bindings are at most two levels deep and never form a cycle; the check only keeps
        // corrupt data from looping here.
        if owners.contains(&owner.id) || owner.id == id {
            break;
        }
        next = owner.bound_to.clone();
        owners.push(owner.id);
    }
    owners.reverse();
    Ok(owners)
}

#[cfg(test)]
mod tests;
