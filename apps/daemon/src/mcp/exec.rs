//! The daemon side of the MCP tools: running one call against the real consoles, projects and
//! sessions.
//!
//! The calling session is resolved from the token the call arrived with, never from an argument,
//! so a child that rewrote its own arguments still cannot act on another session. Which tools it
//! may call follows from that session's role and whether it is bound, checked here as well as in
//! the child: the child is a separate process and its announcement is not something the daemon can
//! rely on.
//!
//! Every tool goes through the same coordinator functions the control socket uses. The console
//! session is a second client of the same operations, not a second implementation of them, and an
//! unbound project session is a third that runs the same implementations scoped to its own
//! project.

use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::coordinator::{self, OpenRequest};
use crate::protocol::{
    now_millis, Agent, Event, Origin, Page, Project, ProjectSource, Role, Session, SessionStatus,
};
use crate::reporting::{self, Delivery, Report, ReportStatus, WhenBlocked};
use crate::sharing::{self, OwnerCopy};
use crate::state::AppState;

/// How much of a session's output `get_session` hands back. Enough to see what it is doing and how
/// it got there, short of handing the caller a transcript to wade through.
const OUTPUT_TAIL: usize = 8 * 1024;

/// Runs one tool call. The `Err` case is what the model is shown as the reason the call failed, so
/// the messages are written for it rather than for a log.
pub async fn call(
    state: &Arc<AppState>,
    session_id: &str,
    tool: &str,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let session = state.session_record(session_id)?;
    if super::tool_by_name(session.role, session.bound_to.is_some(), tool).is_none() {
        bail!("`{tool}` is not a tool this session can call");
    }

    match tool {
        "list_projects" => list_projects(state, &session),
        "add_project" => add_project(state, &session, arguments).await,
        "start_session" => start_session(state, &session, arguments).await,
        "send_message" => send_message(state, &session, arguments).await,
        "get_session" => get_session(state, &session, arguments),
        "archive_session" => archive_session(state, &session, arguments),
        "list_archived" => list_archived(state, &session, arguments),
        "reopen_session" => reopen_session(state, &session, arguments).await,
        "show_page" => show_page(state, &session, arguments),
        "report" => report(state, &session, arguments).await,
        "list_project_sessions" => list_project_sessions(state, &session),
        "share_info" => share_info(state, &session, arguments).await,
        // Unreachable while the catalogue and this dispatch agree; a tool added to one and not the
        // other should say so rather than look like a refusal.
        _ => bail!("`{tool}` is announced but not implemented"),
    }
}

// -- orchestration tools -----------------------------------------------------

fn list_projects(state: &Arc<AppState>, console_session: &Session) -> Result<Value> {
    let projects = console_projects(state, &console_session.console_id)?;
    let sessions = state.store.list_sessions()?;
    let entries = projects
        .iter()
        .map(|project| {
            let live = sessions
                .iter()
                .filter(|session| {
                    session.project_id.as_deref() == Some(&project.id)
                        && !session.status.is_dormant()
                })
                .map(|session| describe_session(state, session, console_session))
                .collect::<Result<Vec<Value>>>()?;
            Ok(json!({
                "project": project.id,
                "name": project.name,
                "host": project.host_id,
                "path": project.path,
                "default_agent": project.default_agent,
                "tags": project.tags,
                "sessions": live,
            }))
        })
        .collect::<Result<Vec<Value>>>()?;
    Ok(json!({ "projects": entries }))
}

async fn add_project(
    state: &Arc<AppState>,
    console_session: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let source = match required_str(arguments, "source")? {
        "local" => ProjectSource::Local,
        "parent" => ProjectSource::Parent,
        // `github` is the old name of `git`, still accepted so a console session that learned it
        // keeps working.
        "git" | "github" => ProjectSource::Git,
        other => bail!("`{other}` is not one of `local`, `parent` or `git`"),
    };
    let added = coordinator::add_project(
        state,
        coordinator::AddProjectRequest {
            console_id: console_session.console_id.clone(),
            source,
            path: optional_string(arguments, "path"),
            remote_url: optional_string(arguments, "remote_url"),
            name: optional_string(arguments, "name"),
            default_agent: optional_agent(arguments, "default_agent")?,
            detect_default_agent: true,
            tags: None,
        },
    )
    .await?;
    Ok(json!({
        "projects": added
            .iter()
            .map(|project| json!({
                "project": project.id,
                "name": project.name,
                "path": project.path,
            }))
            .collect::<Vec<_>>(),
    }))
}

async fn start_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let project = match caller.role {
        Role::Console => resolve_project(
            state,
            &caller.console_id,
            required_str(arguments, "project")?,
        )?,
        Role::Project => own_project(state, caller, optional_string(arguments, "project"))?,
    };
    let brief = arguments
        .get("brief")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("`brief` is required and must be an object"))?;
    let goal = brief
        .get("goal")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|goal| !goal.is_empty())
        .ok_or_else(|| anyhow!("`brief.goal` is required"))?;
    let task = reporting::render_brief(
        goal,
        brief.get("context").and_then(Value::as_str),
        brief.get("acceptance").and_then(Value::as_str),
        brief.get("constraints").and_then(Value::as_str),
    );

    let session = coordinator::open_session(
        state,
        OpenRequest {
            console_id: caller.console_id.clone(),
            project_id: Some(project.id),
            agent: optional_agent(arguments, "agent")?,
            // A session a session starts takes the console's account for its agent.
            account: None,
            task: Some(task),
            // Named for the task, not the project: several sessions dispatched into one project
            // would otherwise all carry the project's name and be indistinguishable in the menu.
            title: Some(reporting::title_from_goal(goal)),
            origin: Origin::Console,
            // A session a session starts always reports to it.
            bound_to: Some(caller.id.clone()),
        },
    )
    .await?;
    Ok(json!({ "session": session.id, "agent": session.agent }))
}

async fn send_message(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let target = resolve_owned_session(state, caller, required_str(arguments, "session")?)?;
    let text = required_str(arguments, "text")?.to_string();
    let delivery = write_off_runtime(state, &target.id, text, WhenBlocked::Queue).await?;
    Ok(json!({
        "delivered": delivery == Delivery::Written,
        "note": delivery_note(delivery),
    }))
}

/// What the caller is told of a message for a session. A queued one is reported rather than
/// refused: queuing is the designed behaviour for a session that is waiting for the user, and
/// nagging it is exactly what the caller must not do.
fn delivery_note(delivery: Delivery) -> &'static str {
    match delivery {
        Delivery::Written => "Delivered.",
        Delivery::Queued => {
            "Queued: this session cannot take a message right now. It will be delivered as soon \
             as it can, so do not send it again."
        }
    }
}

/// Writes into a session from the runtime. The write itself blocks on the PTY for as long as the
/// child is not draining, so it goes to a blocking thread rather than a runtime worker.
async fn write_off_runtime(
    state: &Arc<AppState>,
    id: &str,
    text: String,
    when_blocked: WhenBlocked,
) -> Result<Delivery> {
    let state = state.clone();
    let id = id.to_string();
    tokio::task::spawn_blocking(move || reporting::write_message(&state, &id, &text, when_blocked))
        .await?
}

fn get_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let target = resolve_session(state, caller, required_str(arguments, "session")?)?;
    let output = state
        .live_session(&target.id)
        .map(|live| readable_output(&live.recent_output(OUTPUT_TAIL)));
    let mut description = describe_session(state, &target, caller)?;
    description["recent_output"] = json!(output);
    if target.status == SessionStatus::WaitingUser {
        description["note"] = json!(
            "This session is waiting for the user, not for you. Leave it be until they have \
             answered."
        );
    }
    Ok(description)
}

fn archive_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let target = resolve_owned_session(state, caller, required_str(arguments, "session")?)?;
    coordinator::archive_session(state, &target.id)?;
    Ok(json!({ "session": target.id, "status": SessionStatus::Archived }))
}

fn list_archived(
    state: &Arc<AppState>,
    console_session: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let project = resolve_project(
        state,
        &console_session.console_id,
        required_str(arguments, "project")?,
    )?;
    let sessions = state
        .store
        .list_sessions()?
        .iter()
        .filter(|session| {
            session.project_id.as_deref() == Some(&project.id)
                && session.status == SessionStatus::Archived
        })
        .map(|session| describe_session(state, session, console_session))
        .collect::<Result<Vec<Value>>>()?;
    Ok(json!({ "sessions": sessions }))
}

async fn reopen_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let target = resolve_owned_session(state, caller, required_str(arguments, "session")?)?;
    // The instruction travels with the relaunch rather than being written after it: whichever hook
    // would release it can fire the moment the agent starts, so the queueing belongs inside the
    // launch, where a refusal also undoes it.
    coordinator::resume_session(
        state,
        &target.id,
        arguments.get("text").and_then(Value::as_str),
    )
    .await?;
    Ok(json!({ "session": target.id }))
}

fn show_page(
    state: &Arc<AppState>,
    console_session: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let html = required_str(arguments, "html")?.to_string();
    let page = Page {
        id: Uuid::new_v4().to_string(),
        console_session_id: console_session.id.clone(),
        html,
        // No agent exposes a message id to put here yet; the rewind linkage that would read it is
        // not built.
        anchor_message_id: None,
        created_at: now_millis(),
    };
    state.store.insert_page(&page)?;
    state.broadcast(Event::PageCreated { page: page.clone() });
    Ok(json!({ "page": page.id }))
}

// -- information exchange ----------------------------------------------------

/// The running sessions of the caller's own project, project sessions only: a console session is
/// outside every project and cannot be shared with.
fn list_project_sessions(state: &Arc<AppState>, caller: &Session) -> Result<Value> {
    let sessions = state
        .store
        .list_sessions()?
        .iter()
        .filter(|session| {
            session.role == Role::Project
                && session.project_id == caller.project_id
                && session.id != caller.id
                && !session.status.is_dormant()
        })
        .map(|session| {
            let mut description = describe_session(state, session, caller)?;
            description["your_owner"] = json!(caller.bound_to.as_deref() == Some(&session.id));
            Ok(description)
        })
        .collect::<Result<Vec<Value>>>()?;
    Ok(json!({ "your_owner": caller.bound_to, "sessions": sessions }))
}

async fn share_info(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let wanted = required_str(arguments, "session")?;
    let target = resolve_session(state, caller, wanted)?;
    if target.id == caller.id {
        bail!("session `{wanted}` is your own session; there is nobody to share with");
    }
    let text = required_str(arguments, "text")?.to_string();
    let owned_state = state.clone();
    let (caller_id, target_id) = (caller.id.clone(), target.id.clone());
    // Delivery writes into the sessions' PTYs, which blocks.
    let (delivery, copy) = tokio::task::spawn_blocking(move || {
        sharing::deliver_info(&owned_state, &caller_id, &target_id, &text)
    })
    .await??;
    let mut note = delivery_note(delivery).to_string();
    match copy {
        OwnerCopy::NotNeeded => {}
        OwnerCopy::Sent(Delivery::Written) => note.push_str(" Its owner was sent a copy."),
        OwnerCopy::Sent(Delivery::Queued) => note.push_str(
            " Its owner's copy is queued, and will arrive as soon as the owner can take it.",
        ),
        OwnerCopy::Failed(reason) => note.push_str(&format!(
            " Its owner could not be sent a copy ({reason}); the information itself was not \
             affected."
        )),
    }
    Ok(json!({ "delivered": delivery == Delivery::Written, "note": note }))
}

// -- reporting ---------------------------------------------------------------

async fn report(
    state: &Arc<AppState>,
    session: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let summary = required_str(arguments, "summary")?;
    let status_text = required_str(arguments, "status")?;
    let status = ReportStatus::parse(status_text).ok_or_else(|| {
        anyhow!("`{status_text}` is not one of `done`, `failed` or `needs_decision`")
    })?;
    let open_items: Vec<String> = arguments
        .get("open_items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    // Recorded before delivery: the session is credited with having reported even if its owner
    // cannot take the message yet, so its stop does not also produce a synthesised report.
    state.mark_reported(&session.id);

    let summary = summary.to_string();
    let reporter_id = session.id.clone();
    let owned_state = state.clone();
    // Delivery writes into the owner's PTY, which blocks.
    let note = tokio::task::spawn_blocking(move || {
        reporting::deliver_report(
            &owned_state,
            &reporter_id,
            Report {
                summary: &summary,
                status,
                open_items: &open_items,
                synthesised: false,
            },
        )
    })
    .await?;
    // Taken back whenever `deliver_report` failed, so the owner gets a synthesised report instead
    // of neither. A failure after the owner already has the report would make that a duplicate,
    // which is the better way round.
    let note = note.inspect_err(|_| state.clear_reported(&session.id))?;
    Ok(json!({ "note": note }))
}

// -- argument and record helpers ---------------------------------------------

fn required_str<'a>(arguments: &'a Map<String, Value>, key: &str) -> Result<&'a str> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("`{key}` is required"))
}

fn optional_string(arguments: &Map<String, Value>, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn optional_agent(arguments: &Map<String, Value>, key: &str) -> Result<Option<Agent>> {
    match optional_string(arguments, key) {
        None => Ok(None),
        Some(value) => match value.as_str() {
            "claude" => Ok(Some(Agent::Claude)),
            "codex" => Ok(Some(Agent::Codex)),
            "grok" => Ok(Some(Agent::Grok)),
            other => bail!("`{other}` is not one of `claude`, `codex` or `grok`"),
        },
    }
}

fn console_projects(state: &Arc<AppState>, console_id: &str) -> Result<Vec<Project>> {
    Ok(state
        .store
        .list_projects()?
        .into_iter()
        .filter(|project| project.console_id == console_id)
        .collect())
}

/// Finds a project by id, or by name when that names exactly one. The console session works from
/// what `list_projects` told it, which is both, and a name it half-remembers must not resolve to
/// whichever project happens to sort first.
fn resolve_project(state: &Arc<AppState>, console_id: &str, wanted: &str) -> Result<Project> {
    let projects = console_projects(state, console_id)?;
    if let Some(project) = projects.iter().find(|project| project.id == wanted) {
        return Ok(project.clone());
    }
    let matches: Vec<&Project> = projects
        .iter()
        .filter(|project| project.name == wanted)
        .collect();
    match matches.as_slice() {
        [project] => Ok((*project).clone()),
        [] => bail!("this console has no project `{wanted}`"),
        _ => bail!(
            "`{wanted}` names {} projects in this console; use the project's id instead",
            matches.len()
        ),
    }
}

/// The project an unbound project session starts sessions in: its own, always. A `project` it
/// names must be that one, by id or by name, so a call aimed at another project is refused rather
/// than quietly run in this one.
fn own_project(state: &Arc<AppState>, caller: &Session, named: Option<String>) -> Result<Project> {
    let project = caller
        .project_id
        .as_deref()
        .map(|id| state.store.get_project(id))
        .transpose()?
        .flatten()
        .ok_or_else(|| anyhow!("this session's project is no longer on record"))?;
    if let Some(named) = named {
        if named != project.id && named != project.name {
            bail!(
                "you can start sessions only in your own project, `{}` (\"{}\"), not in `{named}`",
                project.id,
                project.name
            );
        }
    }
    Ok(project)
}

/// Finds a session the caller is allowed to read: for a console session, any of its own console's;
/// for a project session, any of its own project's. A session id from another console, or from
/// another project of a project session's, is refused rather than acted on, so neither can reach
/// into somebody else's.
fn resolve_session(state: &Arc<AppState>, caller: &Session, wanted: &str) -> Result<Session> {
    let session = state
        .store
        .get_session(wanted)?
        .ok_or_else(|| anyhow!("there is no session `{wanted}`"))?;
    if session.console_id != caller.console_id {
        bail!("session `{wanted}` belongs to another console");
    }
    if caller.role == Role::Project && session.role == Role::Console {
        bail!(
            "session `{wanted}` is a console session, which a project session cannot reach; if \
             you meant to say something to the user, say it in this terminal"
        );
    }
    if caller.role == Role::Project && session.project_id != caller.project_id {
        bail!(
            "session `{wanted}` belongs to another project; you can reach only your own project's"
        );
    }
    Ok(session)
}

/// Finds a session the caller may act on: one it can read that is bound to it. Every tool that
/// writes into another session goes through this and nothing else, so the rule that reads are wide
/// and writes are the caller's own is kept in one place. A session that is unbound, or bound to
/// another session, is refused with a reason (naming the owner when there is one), so the model
/// leaves it alone rather than retrying.
fn resolve_owned_session(state: &Arc<AppState>, caller: &Session, wanted: &str) -> Result<Session> {
    let session = resolve_session(state, caller, wanted)?;
    // Including a console session's own: one that ended itself would leave its console's project
    // sessions reporting to nothing.
    if wanted == caller.id {
        bail!(
            "session `{wanted}` is your own session, which is not yours to act on; if you meant \
             to say something, say it to the user"
        );
    }
    if session.role == Role::Console {
        bail!(
            "session `{wanted}` is another console session; acting on one is the user's to do, \
             not yours"
        );
    }
    match session.bound_to.as_deref() {
        Some(owner) if owner == caller.id => Ok(session),
        Some(owner) => {
            let owner_record = state.store.get_session(owner)?;
            // "Another" only when the owner is the caller's own kind: a project session is not
            // told that a console session is another one of its kind.
            let kind = owner_record
                .as_ref()
                .map_or("session".to_string(), |owner| {
                    let article = if owner.role == caller.role {
                        "another"
                    } else {
                        "a"
                    };
                    format!("{article} {} session", owner_kind(owner))
                });
            let title = owner_record
                .map(|owner| format!(" (\"{}\")", owner.title))
                .unwrap_or_default();
            bail!(
                "session `{wanted}` is bound to {kind}, `{owner}`{title}. It is that session's \
                 to drive, not yours; leave it alone"
            )
        }
        None => bail!(
            "session `{wanted}` is not bound to any session: the user opened it themselves and \
             kept it outside the orchestration. It is theirs, not yours; leave it alone"
        ),
    }
}

/// What kind of session an owner is, as `owner_kind` reports it.
fn owner_kind(owner: &Session) -> &'static str {
    match owner.role {
        Role::Console => "console",
        Role::Project => "project",
    }
}

/// `caller` is the session asking. Everything here is the same whoever asks except `yours`, which
/// is relative to `caller`: a console may hold several console sessions, and each reads a
/// session's owner alike but is told only of its own that they are its to act on.
fn describe_session(state: &Arc<AppState>, session: &Session, caller: &Session) -> Result<Value> {
    let owner_kind = match &session.bound_to {
        Some(owner) => state.store.get_session(owner)?.as_ref().map(owner_kind),
        None => None,
    };
    Ok(json!({
        "session": session.id,
        "title": session.title,
        "agent": session.agent,
        "status": session.status,
        "role": session.role,
        "project": session.project_id,
        // The session this one reports to. Null for a project session means unbound: the user
        // opened it themselves and kept it outside the orchestration. It is also null for a
        // console session, which is never bound, so `role` is what tells the two apart. Only a
        // session whose owner is the caller is the caller's to act on.
        "owner": session.bound_to,
        // Whether that owner is a console session or a project session; null with no owner.
        "owner_kind": owner_kind,
        "yours": session.bound_to.as_deref() == Some(caller.id.as_str()),
        "started_at": session.started_at,
        "ended_at": session.ended_at,
    }))
}

/// Makes raw PTY output readable: escape sequences and the cursor-control bytes an agent's
/// renderer emits by the thousand carry no information once the frames are gone, and left in they
/// are most of what the console session would be reading.
fn readable_output(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\x1b' => match chars.peek() {
                // CSI: parameters and intermediates, then one final byte in `@`..`~`.
                Some('[') => {
                    chars.next();
                    while let Some(&next) = chars.peek() {
                        chars.next();
                        if ('@'..='~').contains(&next) {
                            break;
                        }
                    }
                }
                // OSC and the other string-introducing sequences run to a terminator rather than
                // to a single final byte.
                Some(']') | Some('P') | Some('X') | Some('^') | Some('_') => {
                    chars.next();
                    while let Some(next) = chars.next() {
                        if next == '\x07' {
                            break;
                        }
                        if next == '\x1b' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                // Anything else is a two-byte escape; drop both.
                Some(_) => {
                    chars.next();
                }
                None => {}
            },
            '\n' | '\t' => out.push(ch),
            '\r' => {}
            ch if (ch as u32) < 0x20 || ch as u32 == 0x7f => {}
            ch => out.push(ch),
        }
    }
    // Carriage-return redrawing leaves runs of blank lines behind once the escapes are gone.
    let mut collapsed = String::with_capacity(out.len());
    let mut blanks = 0;
    for line in out.lines() {
        if line.trim().is_empty() {
            blanks += 1;
            if blanks > 1 {
                continue;
            }
        } else {
            blanks = 0;
        }
        collapsed.push_str(line.trim_end());
        collapsed.push('\n');
    }
    collapsed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{AgentAvailability, Availability, Console, ProjectSource};
    use crate::store::LOCAL_HOST_ID;
    use crate::test_support::ScratchDir;

    fn console() -> Console {
        Console {
            id: "console-1".to_string(),
            name: "Console".to_string(),
            workdir: "/tmp/console-1".to_string(),
            console_session_agent: Agent::Claude,
            default_agent: Agent::Claude,
            claude_account_id: None,
            codex_account_id: None,
            grok_account_id: None,
            icon: None,
            created_at: 0,
        }
    }

    fn console_session(id: &str) -> Session {
        Session {
            id: id.to_string(),
            agent: Agent::Claude,
            agent_session_id: None,
            console_id: "console-1".to_string(),
            project_id: None,
            host_id: LOCAL_HOST_ID.to_string(),
            role: Role::Console,
            origin: Origin::User,
            title: "Hub".to_string(),
            status: SessionStatus::Idle,
            has_conversation: false,
            bound_to: None,
            colour: None,
            ordinal: None,
            account_id: None,
            config_dir: None,
            pinned: false,
            started_at: 0,
            ended_at: None,
        }
    }

    /// The refusal `coordinator::open_session` raises for an unavailable agent is not only the
    /// control socket's — a console session reaches it through its own `start_session` tool, and
    /// comes back with a tool error carrying the reason in prose, as "The console session's
    /// tools" in `docs/product/hub-orchestration.md` promises for any refusal.
    #[tokio::test]
    async fn an_unavailable_agent_refuses_the_console_sessions_own_start_session_tool() {
        let (state, _dir) =
            crate::test_support::app_state("mcp-exec-start-session-agent-unavailable");
        state.store.insert_console(&console()).unwrap();
        state
            .store
            .insert_project(&Project {
                id: "project-1".to_string(),
                console_id: "console-1".to_string(),
                host_id: LOCAL_HOST_ID.to_string(),
                name: "Project".to_string(),
                path: "/tmp/project-1".to_string(),
                default_agent: None,
                source: ProjectSource::Local,
                remote_url: None,
                claude_trust_consent: false,
                pinned: false,
                tags: Vec::new(),
            })
            .unwrap();
        let owner = console_session("owner");
        state.store.insert_session(&owner).unwrap();
        state.set_agent_availability(vec![AgentAvailability {
            agent: Agent::Claude,
            availability: Availability::Unavailable,
            default_account_dir: Some("/home/user/.claude".to_string()),
        }]);

        let arguments = json!({
            "project": "project-1",
            "brief": { "goal": "do the thing" },
        })
        .as_object()
        .unwrap()
        .clone();

        let err = call(&state, &owner.id, "start_session", &arguments)
            .await
            .expect_err("refused");
        assert!(err.to_string().contains("Claude Code"), "{err}");
    }

    fn project_session(id: &str, console_id: &str, bound_to: Option<&str>) -> Session {
        Session {
            role: Role::Project,
            console_id: console_id.to_string(),
            project_id: Some("project-1".to_string()),
            title: format!("Work {id}"),
            bound_to: bound_to.map(str::to_string),
            ..console_session(id)
        }
    }

    /// Two console sessions of one console, one project, and a session each: bound to `a`, bound to
    /// `b`, unbound, one archived under each owner, and one in another console.
    fn shared_console_state(name: &str) -> (Arc<AppState>, ScratchDir) {
        let (state, dir) = crate::test_support::app_state(&format!("mcp-exec-{name}"));
        state.store.insert_console(&console()).unwrap();
        state
            .store
            .insert_console(&Console {
                id: "console-2".to_string(),
                ..console()
            })
            .unwrap();
        for (id, console_id) in [("project-1", "console-1"), ("project-2", "console-2")] {
            state
                .store
                .insert_project(&Project {
                    id: id.to_string(),
                    console_id: console_id.to_string(),
                    host_id: LOCAL_HOST_ID.to_string(),
                    name: id.to_string(),
                    path: format!("/tmp/{id}"),
                    default_agent: None,
                    source: ProjectSource::Local,
                    remote_url: None,
                    claude_trust_consent: false,
                    pinned: false,
                    tags: Vec::new(),
                })
                .unwrap();
        }
        let mut sessions = vec![
            console_session("a"),
            console_session("b"),
            project_session("of-a", "console-1", Some("a")),
            project_session("of-b", "console-1", Some("b")),
            project_session("loose", "console-1", None),
            Session {
                project_id: Some("project-2".to_string()),
                ..project_session("foreign", "console-2", None)
            },
        ];
        for (id, owner) in [("archived-of-a", "a"), ("archived-of-b", "b")] {
            let mut archived = project_session(id, "console-1", Some(owner));
            archived.status = SessionStatus::Archived;
            sessions.push(archived);
        }
        for session in &sessions {
            state.store.insert_session(session).unwrap();
        }
        (state, dir)
    }

    fn arguments(value: Value) -> Map<String, Value> {
        value.as_object().unwrap().clone()
    }

    /// Reads are console-wide: each console session sees the other's sessions and who owns them.
    #[tokio::test]
    async fn reads_cover_the_whole_console_with_each_sessions_owner() {
        let (state, _dir) = shared_console_state("tool-surface-reads");
        for (caller, other_owner_session, other_owner) in [("a", "of-b", "b"), ("b", "of-a", "a")] {
            let listed = call(&state, caller, "list_projects", &Map::new())
                .await
                .unwrap();
            // Only this console's project is listed, so the other console's session is not.
            assert_eq!(listed["projects"].as_array().unwrap().len(), 1);
            let sessions = listed["projects"][0]["sessions"].as_array().unwrap();
            let ids: Vec<&str> = sessions
                .iter()
                .map(|session| session["session"].as_str().unwrap())
                .collect();
            assert_eq!(ids.len(), 3, "{ids:?}");
            assert!(
                ids.contains(&"loose") && !ids.contains(&"foreign"),
                "{ids:?}"
            );
            let theirs = sessions
                .iter()
                .find(|session| session["session"] == other_owner_session)
                .unwrap();
            assert_eq!(theirs["owner"], other_owner);
            assert_eq!(theirs["owner_kind"], "console");
            assert_eq!(theirs["yours"], false);

            let fetched = call(
                &state,
                caller,
                "get_session",
                &arguments(json!({ "session": other_owner_session })),
            )
            .await
            .unwrap();
            assert_eq!(fetched["owner"], other_owner);
            assert_eq!(fetched["yours"], false);
        }
        let loose = call(
            &state,
            "a",
            "get_session",
            &arguments(json!({ "session": "loose" })),
        )
        .await
        .unwrap();
        assert!(loose["owner"].is_null());
    }

    /// The archive of a project is listed whole, owners marked, rather than filtered to the caller.
    #[tokio::test]
    async fn the_archive_lists_every_owners_sessions() {
        let (state, _dir) = shared_console_state("tool-surface-archive");
        let listed = call(
            &state,
            "a",
            "list_archived",
            &arguments(json!({ "project": "project-1" })),
        )
        .await
        .unwrap();
        let mut owners: Vec<(&str, &str)> = listed["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|session| {
                (
                    session["session"].as_str().unwrap(),
                    session["owner"].as_str().unwrap(),
                )
            })
            .collect();
        owners.sort();
        assert_eq!(owners, [("archived-of-a", "a"), ("archived-of-b", "b")]);
    }

    /// The one ownership check, exercised through each write-side tool: a session that is unbound,
    /// bound to the other console session, a console session, or from another console is refused
    /// with a reason. For the two tools whose success would change the session, the session is also
    /// checked to be untouched; `send_message` has no such visible effect to compare.
    #[tokio::test]
    async fn writes_are_refused_for_sessions_the_caller_does_not_own() {
        let (state, _dir) = shared_console_state("tool-surface-writes");
        let cases = [
            (
                "send_message",
                "of-b",
                "bound to another console session, `b`",
            ),
            ("send_message", "loose", "not bound to any session"),
            ("send_message", "b", "another console session;"),
            ("send_message", "foreign", "belongs to another console"),
            (
                "archive_session",
                "of-b",
                "bound to another console session, `b`",
            ),
            ("archive_session", "loose", "not bound to any session"),
            ("archive_session", "a", "your own session"),
            (
                "reopen_session",
                "archived-of-b",
                "bound to another console session, `b`",
            ),
        ];
        for (tool, target, reason) in cases {
            let before = state.store.get_session(target).unwrap().unwrap();
            let err = call(
                &state,
                "a",
                tool,
                &arguments(json!({ "session": target, "text": "hello" })),
            )
            .await
            .expect_err("refused");
            assert!(err.to_string().contains(reason), "{tool} {target}: {err}");
            let after = state.store.get_session(target).unwrap().unwrap();
            if tool != "send_message" {
                assert_eq!(before.status, after.status, "{tool} {target}");
                assert_eq!(before.ended_at, after.ended_at, "{tool} {target}");
            }
        }

        // The caller's own is not refused by the ownership check.
        call(
            &state,
            "a",
            "archive_session",
            &arguments(json!({ "session": "of-a" })),
        )
        .await
        .unwrap();
        let archived = state.store.get_session("of-a").unwrap().unwrap();
        assert_eq!(archived.status, SessionStatus::Archived);
    }

    /// `shared_console_state` plus an unbound project session, `starter`, in `project-1` with one
    /// session bound to it, `mine`, and a second project of the same console with a session of its
    /// own.
    fn peer_state(name: &str) -> (Arc<AppState>, ScratchDir) {
        let (state, dir) = shared_console_state(name);
        state
            .store
            .insert_project(&Project {
                id: "project-3".to_string(),
                console_id: "console-1".to_string(),
                host_id: LOCAL_HOST_ID.to_string(),
                name: "project-3".to_string(),
                path: "/tmp/project-3".to_string(),
                default_agent: None,
                source: ProjectSource::Local,
                remote_url: None,
                claude_trust_consent: false,
                pinned: false,
                tags: Vec::new(),
            })
            .unwrap();
        for session in [
            project_session("starter", "console-1", None),
            project_session("mine", "console-1", Some("starter")),
            Session {
                project_id: Some("project-3".to_string()),
                ..project_session("other-project", "console-1", None)
            },
        ] {
            state.store.insert_session(&session).unwrap();
        }
        (state, dir)
    }

    /// What a session is offered follows from its role and binding, and the daemon holds to it:
    /// a session bound to another session has no orchestration tools, whatever its child
    /// announced.
    #[tokio::test]
    async fn only_an_unbound_project_session_may_call_the_orchestration_tools() {
        let (state, _dir) = peer_state("peer-tool-gating");
        for (caller, tool) in [
            ("mine", "start_session"),
            ("mine", "send_message"),
            ("of-a", "archive_session"),
            ("starter", "list_projects"),
            ("starter", "add_project"),
            ("starter", "show_page"),
            ("a", "report"),
        ] {
            let err = call(
                &state,
                caller,
                tool,
                &arguments(json!({ "session": "mine", "text": "x" })),
            )
            .await
            .expect_err("not offered");
            assert!(
                err.to_string().contains("is not a tool"),
                "{caller} {tool}: {err}"
            );
        }
    }

    /// An unbound project session starts sessions only in its own project: another project of the
    /// same console is refused, by id or by name, before anything is launched.
    #[tokio::test]
    async fn an_unbound_project_session_cannot_start_a_session_in_another_project() {
        let (state, _dir) = peer_state("peer-start-scope");
        let before = state.store.list_sessions().unwrap().len();
        for project in ["project-3", "project-2"] {
            let err = call(
                &state,
                "starter",
                "start_session",
                &arguments(json!({ "project": project, "brief": { "goal": "x" } })),
            )
            .await
            .expect_err("refused");
            assert!(
                err.to_string().contains("only in your own project"),
                "{project}: {err}"
            );
        }
        assert_eq!(state.store.list_sessions().unwrap().len(), before);
    }

    /// Without a `project`, or naming the one the caller is in, the call gets as far as the launch
    /// the console session's tool uses: an unavailable agent is refused there, in prose.
    #[tokio::test]
    async fn an_unbound_project_sessions_start_session_reaches_the_launch_in_its_own_project() {
        let (state, _dir) = peer_state("peer-start-launch");
        state.set_agent_availability(vec![AgentAvailability {
            agent: Agent::Claude,
            availability: Availability::Unavailable,
            default_account_dir: Some("/home/user/.claude".to_string()),
        }]);
        for project in [None, Some("project-1")] {
            let mut input = arguments(json!({ "brief": { "goal": "x" }, "agent": "claude" }));
            if let Some(project) = project {
                input.insert("project".to_string(), json!(project));
            }
            let err = call(&state, "starter", "start_session", &input)
                .await
                .expect_err("agent unavailable");
            assert!(
                err.to_string().contains("Claude Code"),
                "{project:?}: {err}"
            );
        }
    }

    /// Reads reach the caller's own project and say who owns what, including a project session as
    /// owner; another project's sessions, and console sessions, are out of reach.
    #[tokio::test]
    async fn an_unbound_project_session_reads_its_own_project() {
        let (state, _dir) = peer_state("peer-reads");
        let read = |session: &'static str| {
            let state = state.clone();
            async move {
                call(
                    &state,
                    "starter",
                    "get_session",
                    &arguments(json!({ "session": session })),
                )
                .await
            }
        };
        let mine = read("mine").await.unwrap();
        assert_eq!(mine["owner"], "starter");
        assert_eq!(mine["owner_kind"], "project");
        assert_eq!(mine["yours"], true);
        let theirs = read("of-a").await.unwrap();
        assert_eq!(theirs["owner_kind"], "console");
        assert_eq!(theirs["yours"], false);
        assert!(read("loose").await.unwrap()["owner_kind"].is_null());
        for (outside, reason) in [
            ("other-project", "belongs to another project"),
            ("a", "is a console session"),
            ("foreign", "belongs to another console"),
        ] {
            let err = read(outside).await.expect_err(outside);
            assert!(err.to_string().contains(reason), "{outside}: {err}");
        }

        // A console session sees the same session with the project session as its owner.
        let from_console = call(
            &state,
            "a",
            "get_session",
            &arguments(json!({ "session": "mine" })),
        )
        .await
        .unwrap();
        assert_eq!(from_console["owner_kind"], "project");
        assert_eq!(from_console["yours"], false);
    }

    /// The ownership rule is the same one for a project session as owner: only what is bound to
    /// the caller may be acted on, and everything else is refused with who owns it.
    #[tokio::test]
    async fn an_unbound_project_session_acts_only_on_the_sessions_bound_to_it() {
        let (state, _dir) = peer_state("peer-writes");
        let cases = [
            ("send_message", "of-a", "bound to a console session, `a`"),
            ("send_message", "loose", "not bound to any session"),
            ("send_message", "starter", "your own session"),
            (
                "send_message",
                "other-project",
                "belongs to another project",
            ),
            ("send_message", "a", "is a console session"),
            ("archive_session", "of-b", "bound to a console session, `b`"),
            ("archive_session", "loose", "not bound to any session"),
            (
                "reopen_session",
                "archived-of-a",
                "bound to a console session, `a`",
            ),
        ];
        for (tool, target, reason) in cases {
            let before = state.store.get_session(target).unwrap().unwrap();
            let err = call(
                &state,
                "starter",
                tool,
                &arguments(json!({ "session": target, "text": "hello" })),
            )
            .await
            .expect_err("refused");
            assert!(err.to_string().contains(reason), "{tool} {target}: {err}");
            let after = state.store.get_session(target).unwrap().unwrap();
            assert_eq!(before.status, after.status, "{tool} {target}");
        }

        // A project session owns nobody else's: `mine` is `starter`'s and not `loose`'s.
        let err = call(
            &state,
            "loose",
            "archive_session",
            &arguments(json!({ "session": "mine" })),
        )
        .await
        .expect_err("another owner's session");
        assert!(
            err.to_string()
                .contains("bound to another project session, `starter`"),
            "{err}"
        );

        call(
            &state,
            "starter",
            "archive_session",
            &arguments(json!({ "session": "mine" })),
        )
        .await
        .unwrap();
        let archived = state.store.get_session("mine").unwrap().unwrap();
        assert_eq!(archived.status, SessionStatus::Archived);
    }

    /// Every project session sees the other running sessions of its own project with their owners,
    /// whether it is bound or not, and which one owns it; archived sessions, console sessions and
    /// other projects' sessions are left out.
    #[tokio::test]
    async fn a_project_session_lists_its_own_projects_running_sessions() {
        let (state, _dir) = peer_state("peer-list");
        let listed = call(&state, "mine", "list_project_sessions", &Map::new())
            .await
            .unwrap();
        assert_eq!(listed["your_owner"], "starter");
        let mut ids: Vec<&str> = listed["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|session| session["session"].as_str().unwrap())
            .collect();
        ids.sort();
        assert_eq!(ids, ["loose", "of-a", "of-b", "starter"]);
        for session in listed["sessions"].as_array().unwrap() {
            assert_eq!(
                session["your_owner"],
                session["session"] == "starter",
                "{session}"
            );
        }
        let of_a = listed["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|session| session["session"] == "of-a")
            .unwrap();
        assert_eq!(of_a["owner_kind"], "console");

        let unbound = call(&state, "loose", "list_project_sessions", &Map::new())
            .await
            .unwrap();
        assert!(unbound["your_owner"].is_null());
    }

    /// Information reaches only a project session of the caller's own project: not another
    /// project's, not a console session, not the caller itself.
    #[tokio::test]
    async fn information_is_refused_outside_the_callers_project() {
        let (state, _dir) = peer_state("peer-share-scope");
        for (target, reason) in [
            ("other-project", "belongs to another project"),
            ("a", "is a console session"),
            ("foreign", "belongs to another console"),
            ("mine", "your own session"),
        ] {
            let err = call(
                &state,
                "mine",
                "share_info",
                &arguments(json!({ "session": target, "text": "hello" })),
            )
            .await
            .expect_err(target);
            assert!(err.to_string().contains(reason), "{target}: {err}");
        }
    }

    /// Polls a stand-in session's output until it holds `needle`.
    fn output_containing(live: &crate::session::LiveSession, needle: &str) -> String {
        let deadline = std::time::Instant::now() + crate::test_support::PATIENCE;
        loop {
            let output = String::from_utf8_lossy(&live.recent_output(8 * 1024)).into_owned();
            if output.contains(needle) || std::time::Instant::now() >= deadline {
                return output;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    fn running(state: &Arc<AppState>, id: &str) -> crate::test_support::StandIn {
        let live = crate::test_support::idle_stand_in(id);
        state.register_live(live.clone());
        crate::session::spawn_reader_thread(live.clone(), 8 * 1024);
        live
    }

    /// Information arrives framed as information from a session that is not the receiver's owner,
    /// and the receiver's owner, a console session or a project session, is sent a copy; when the
    /// sender is the owner it is told so instead, and nobody is copied.
    #[tokio::test]
    async fn information_is_framed_and_copied_to_the_receivers_owner() {
        let (state, _dir) = peer_state("peer-share-delivery");
        let share = |caller: &'static str, target: &'static str| {
            let state = state.clone();
            async move {
                call(
                    &state,
                    caller,
                    "share_info",
                    &arguments(json!({ "session": target, "text": "the port moved" })),
                )
                .await
            }
        };
        let loose = running(&state, "loose");
        let of_a = running(&state, "of-a");
        let console_a = running(&state, "a");
        let starter = running(&state, "starter");
        let mine = running(&state, "mine");

        // Unbound receiver: framed, and there is no owner to copy.
        let result = share("starter", "loose").await.unwrap();
        assert_eq!(result["note"], "Delivered.");
        let seen = output_containing(&loose, "the port moved");
        assert!(
            seen.contains("Information from session starter — Work starter"),
            "{seen}"
        );
        assert!(seen.contains("not your owner"), "{seen}");

        // Receiver owned by a console session: that session is sent the copy.
        let result = share("loose", "of-a").await.unwrap();
        assert!(result["note"].as_str().unwrap().contains("sent a copy"));
        assert!(output_containing(&of_a, "the port moved").contains("not your owner"));
        let copy = output_containing(&console_a, "the port moved");
        assert!(
            copy.contains("Copy of information shared with your session of-a"),
            "{copy}"
        );
        assert!(copy.contains("Sent by session loose"), "{copy}");

        // Receiver owned by a project session: likewise.
        share("loose", "mine").await.unwrap();
        assert!(output_containing(&mine, "the port moved").contains("not your owner"));
        let copy = output_containing(&starter, "Copy of information");
        assert!(copy.contains("shared with your session mine"), "{copy}");

        // From the owner itself: no "not your owner", and a note with no copy in it.
        let result = share("starter", "mine").await.unwrap();
        assert_eq!(result["note"], "Delivered.");
        let seen = output_containing(&mine, "is your owner");
        assert!(seen.contains("Information from session starter"), "{seen}");
    }

    /// A copy that cannot be delivered, because the owner is not running, does not fail the
    /// information: the receiver has it, and the sender is told the owner has not.
    #[tokio::test]
    async fn an_owner_that_cannot_be_copied_does_not_fail_the_delivery() {
        let (state, _dir) = peer_state("peer-share-copy-fails");
        let of_b = running(&state, "of-b");
        let result = call(
            &state,
            "loose",
            "share_info",
            &arguments(json!({ "session": "of-b", "text": "heads up" })),
        )
        .await
        .unwrap();
        assert_eq!(result["delivered"], true);
        assert!(result["note"]
            .as_str()
            .unwrap()
            .contains("could not be sent a copy"));
        assert!(output_containing(&of_b, "heads up").contains("heads up"));

        // A receiver that is not running is the one refusal.
        let err = call(
            &state,
            "loose",
            "share_info",
            &arguments(json!({ "session": "of-a", "text": "heads up" })),
        )
        .await
        .expect_err("not running");
        assert!(err.to_string().contains("not running"), "{err}");
    }

    #[test]
    fn escape_sequences_and_control_bytes_are_stripped_from_the_output_tail() {
        let raw = b"\x1b[2J\x1b[1;32mbuilding\x1b[0m\r\n\x1b]0;title\x07done\n";
        assert_eq!(readable_output(raw), "building\ndone\n");
    }

    #[test]
    fn runs_of_blank_lines_collapse() {
        assert_eq!(readable_output(b"a\n\n\n\n b \n"), "a\n\n b\n");
    }

    /// Partial UTF-8 is normal: the tail starts at a byte offset into the ring buffer, not at a
    /// character boundary.
    #[test]
    fn a_tail_cut_mid_character_still_reads() {
        let mut raw = "ok ".as_bytes().to_vec();
        raw.extend_from_slice(&"日".as_bytes()[..2]);
        assert!(readable_output(&raw).starts_with("ok "));
    }
}
