//! The daemon side of the MCP tools: running one call against the real consoles, projects and
//! sessions.
//!
//! The calling session is resolved from the token the call arrived with, never from an argument,
//! so a child that rewrote its own arguments still cannot act on another session. Which tools it
//! may call follows from that session's role and whether it has the role of a bound session,
//! checked here as well as in the child: the child is a separate process and its announcement is
//! not something the daemon can rely on.
//!
//! Every tool goes through the same coordinator functions the control socket uses. The console
//! session is a second client of the same operations, not a second implementation of them, and a
//! project session that can own sessions is a third that runs the same implementations scoped to
//! its own project.
//!
//! **Which tools a session has can change while it runs.** A session that binds itself to a
//! project session, or is handed to one, can no longer own sessions, and the check here reads the
//! binding as it stands at each call. What the session was told at launch cannot be taken back —
//! some agents record their role text once for the whole conversation — so every such change is
//! told to it in prose as well: in the call's result when it made the change itself, and in a
//! message written into it when its owner did.

use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::branch_origin;
use crate::branch_origin::cleanup;
use crate::browse::wire_path;
use crate::coordinator::{self, OpenRequest};
use crate::git_remote;
use crate::protocol::{
    error_code, now_millis, CodedError, Console, Event, Origin, Page, Project, ProjectSource, Role,
    Session, SessionStatus,
};
use crate::reporting::{self, Delivery, ReportStatus, WhenBlocked};
use crate::session_request::{self, Handover};
use crate::sharing::{self, OwnerCopy};
use crate::state::AppState;

use super::args::{
    console_projects, find_console, find_project, optional_agent, optional_bool, optional_string,
    optional_string_list, own_console, own_project, present_text, raw_text, required_str,
    resolve_project, string_list,
};
use super::bindings;
use super::settings::{
    self, describe_project, launch_argument, project_accounts_argument, resolve_account,
};

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
    if super::tool_by_name(session.role, session.has_bound_role(), tool).is_none() {
        bail!("`{tool}` is not a tool this session can call");
    }

    match tool {
        "list_projects" => list_projects(state, &session),
        "add_project" => add_project(state, &session, arguments).await,
        "update_project" => settings::update_project(state, &session, arguments).await,
        "get_console" => settings::get_console(state, &session),
        "update_console" => settings::update_console(state, &session, arguments).await,
        "get_settings" => settings::get_settings(state),
        "update_settings" => settings::update_settings(state, arguments),
        "new_session" => new_session(state, &session, arguments).await,
        "send_message" => send_message(state, &session, arguments).await,
        "get_session" => get_session(state, &session, arguments).await,
        "rename_session" => rename_session(state, &session, arguments),
        "set_session_pinned" => set_session_pinned(state, &session, arguments),
        "archive_session" => archive_session(state, &session, arguments).await,
        "archive_self" => archive_self(state, &session).await,
        "list_archived" => list_archived(state, &session, arguments),
        "reopen_session" => reopen_session(state, &session, arguments).await,
        "show_page" => show_page(state, &session, arguments),
        "report" => report(state, &session, arguments).await,
        "list_sessions" => list_sessions(state, &session),
        "get_project" => settings::get_project(state, &session),
        "share_info" => share_info(state, &session, arguments).await,
        "transfer_session" => bindings::transfer_session(state, &session, arguments).await,
        "bind_to_session" => bindings::bind_to_session(state, &session, arguments).await,
        "request_session" => request_session(state, &session, arguments).await,
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
            let mut entry = describe_project(project);
            entry["sessions"] = json!(live);
            Ok(entry)
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
    let remote_url = optional_string(arguments, "remote_url");
    if source == ProjectSource::Git {
        check_clone_url(remote_url.as_deref())?;
    }
    let mut settings = coordinator::NewProjectSettings {
        pinned: optional_bool(arguments, "pinned")?.unwrap_or(false),
        ..Default::default()
    };
    for (agent, account) in project_accounts_argument(state, arguments)? {
        *settings.account_mut(agent) = account;
    }
    for (agent, change) in launch_argument(arguments, "launch")? {
        let (model, effort) = settings.launch_mut(agent);
        *model = change.model.flatten();
        *effort = change.effort.flatten();
    }
    let added = coordinator::add_project(
        state,
        coordinator::AddProjectRequest {
            console_id: console_session.console_id.clone(),
            source,
            path: optional_string(arguments, "path"),
            remote_url,
            name: optional_string(arguments, "name"),
            default_agent: optional_agent(arguments, "default_agent")?,
            settings,
            detect_default_agent: true,
            check_launch: true,
            tags: optional_string_list(arguments, "tags")?,
        },
    )
    .await?;
    Ok(json!({ "projects": added.iter().map(describe_project).collect::<Vec<_>>() }))
}

/// Refuses a repository URL the console session may not clone from: one of a shape the import of a
/// configuration document refuses too, so a tool call is no way to reach a path of this machine or
/// run a helper transport, and one with a credential in it, which would be stored with the project
/// and shown in every listing. The user's own git credentials are what a clone uses.
fn check_clone_url(url: Option<&str>) -> Result<()> {
    let Some(url) = url else { return Ok(()) };
    let shown = git_remote::without_credentials(url);
    if git_remote::has_credentials(url) {
        return Err(CodedError::raised(
            error_code::INVALID_REMOTE_URL,
            format!(
                "`{shown}` carries credentials, which are not accepted in a URL: give it without, \
                 and the user's own git credentials are used"
            ),
            &[("url", &shown)],
        ));
    }
    if !git_remote::is_clonable_url(url) {
        return Err(CodedError::raised(
            error_code::INVALID_REMOTE_URL,
            format!("`{shown}` is not an http(s), ssh or git repository URL that can be cloned"),
            &[("url", &shown)],
        ));
    }
    Ok(())
}

async fn new_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let open = session_to_start(state, caller, arguments)?;
    let session = coordinator::open_session(
        state,
        OpenRequest {
            // A session a session starts always reports to it.
            bound_to: Some(caller.id.clone()),
            ..open
        },
    )
    .await?;
    Ok(json!({ "session": session.id, "agent": session.agent }))
}

/// Reads what `new_session` asks for into the session to open, unbound: in the project the
/// console session names, or in a project session's own.
fn session_to_start(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<OpenRequest> {
    let project = match caller.role {
        Role::Console => resolve_project(
            state,
            &caller.console_id,
            required_str(arguments, "project")?,
        )?,
        Role::Project => own_project(state, caller, optional_string(arguments, "project"))?,
    };
    session_in(
        state,
        &own_console(state, caller)?,
        Some(project),
        arguments,
    )
}

/// Reads a brief and the overrides — `new_session`'s arguments, and `request_session`'s alike —
/// into the session to open, unbound, in `project` of `console`, or a console session of `console`
/// for none.
fn session_in(
    state: &Arc<AppState>,
    console: &Console,
    project: Option<Project>,
    arguments: &Map<String, Value>,
) -> Result<OpenRequest> {
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

    let agent = optional_agent(arguments, "agent")?;
    // An account is one agent's, so it is read for the agent the session will be of, as
    // `coordinator::session_agent` settles it.
    let account = match optional_string(arguments, "account") {
        None => None,
        Some(wanted) => {
            let agent = coordinator::session_agent(agent, project.as_ref(), console);
            Some(resolve_account(state, agent, &wanted)?)
        }
    };
    Ok(OpenRequest {
        console_id: console.id.clone(),
        project_id: project.map(|project| project.id),
        agent,
        // Without one, a session a session starts takes the project's account for its agent,
        // else the console's; a blank model or effort is the agent's own default.
        account,
        model: present_text(arguments, "model")?,
        effort: present_text(arguments, "effort")?,
        task: Some(task),
        // Named for the task, not the project: several sessions dispatched into one project
        // would otherwise all carry the project's name and be indistinguishable in the menu.
        title: Some(reporting::title_from_goal(goal)),
        origin: Origin::Console,
        bound_to: None,
        resume_agent_session_id: None,
        session_id: None,
    })
}

async fn send_message(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let target = resolve_owned_session(state, caller, required_str(arguments, "session")?)?;
    let text = required_str(arguments, "text")?.to_string();
    let delivery =
        reporting::write_message_off_runtime(state, &target.id, text, WhenBlocked::Queue).await?;
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

async fn get_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let target = resolve_session(state, required_str(arguments, "session")?)?;
    let output = recent_output(state, &target.id).await;
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

/// The tail of what a session has printed, readable. A running session's comes from its ring
/// buffer; one whose process has ended has the ring buffer's last contents kept, which is what
/// `GET /ws/term/:session` replays, so the same path is read here. `None` when nothing was kept —
/// no process of it has printed anything yet, it ended with the daemon killed outright before the
/// output could be kept, or a new process took it over and discarded the old output at launch.
async fn recent_output(state: &Arc<AppState>, id: &str) -> Option<String> {
    if let Some(live) = state.live_session(id) {
        return Some(readable_output(&live.recent_output(OUTPUT_TAIL)));
    }
    // A file read of up to the whole ring buffer, so not on a runtime worker.
    let state = state.clone();
    let id = id.to_string();
    // A failed read is no output rather than an error: it must not take the session's status and
    // ownership down with it.
    let saved = tokio::task::spawn_blocking(move || state.saved_output(&id))
        .await
        .unwrap_or(None);
    saved.map(|output| {
        let from = output.len().saturating_sub(OUTPUT_TAIL);
        readable_output(&output[from..])
    })
}

/// Archives a session of the caller's, and once its process has ended removes what can go of the
/// branches and worktrees bound to it, never forcing anything; what stays is named in the answer,
/// so the caller can deal with it.
async fn archive_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let wanted = required_str(arguments, "session")?;
    if wanted == caller.id && caller.bound_to.is_none() {
        bail!(
            "session `{wanted}` is your own session, which `archive_session` does not archive; \
             use `archive_self` to archive yourself"
        );
    }
    let target = resolve_owned_session(state, caller, wanted)?;
    let archived = coordinator::archive_session(state, &target.id)?;
    // A task of its own, so the removal finishes even if the call it answers is dropped.
    let left = tokio::spawn(cleanup::remove_once_stopped(
        state.clone(),
        archived,
        Vec::new(),
    ))
    .await
    .unwrap_or_else(|err| {
        tracing::warn!(%err, "removing the branches of an archived session did not finish");
        Vec::new()
    });
    let mut answer = json!({ "session": target.id, "status": SessionStatus::Archived });
    if !left.is_empty() {
        answer["branches_left"] = cleanup::for_agent(&left);
        answer["note"] = json!(
            "Archived. The git branches and worktrees in `branches_left` are bound to the sessions \
             archived and were not removed, for the reason given with each. If they are yours to \
             clean up, deal with what keeps each (commit or discard the changes, merge the \
             branch), then remove them with `git worktree remove` and `git branch -d`."
        );
    }
    Ok(answer)
}

/// Archives the caller itself, which only a session that reports to nobody does: a lead session
/// wraps up with `report` instead. Its own sessions must all be archived already, since an owner
/// archives each once its task is done, so one that is not (running or interrupted alike) means
/// unfinished work and the call is refused, changing nothing. Once the record is archived the
/// process is ended off the caller's path, so this answer still reaches it.
///
/// The branches and worktrees bound to the caller are removed after its process has ended, where
/// they can go without forcing anything. What will stay is judged before the archive, since the
/// answer has to be on its way before the process is ended.
async fn archive_self(state: &Arc<AppState>, caller: &Session) -> Result<Value> {
    // Held across the checks and the archive, so a lock set in between cannot slip past them.
    let _removal = state.claim_session_removal();
    let caller = archivable_self(state, caller)?;
    let left = cleanup::left_by(state, vec![caller.clone()]).await;
    let archived = coordinator::archive_session(state, &caller.id)?;
    cleanup::remove_in_background(state, archived);
    let mut answer = json!({
        "session": caller.id,
        "status": SessionStatus::Archived,
        "note": "Archived; this session's process will now end.",
    });
    if !left.is_empty() {
        answer["branches_left"] = cleanup::for_agent(&left);
        answer["note"] = json!(
            "Archived; this session's process will now end. The git branches and worktrees in \
             `branches_left` are bound to this session and will not be removed, for the reason \
             given with each; the rest that are bound to it are removed once the process has \
             ended."
        );
    }
    Ok(answer)
}

/// The caller's own record, once it is a session that may archive itself now; otherwise why not.
fn archivable_self(state: &Arc<AppState>, caller: &Session) -> Result<Session> {
    let caller = state.session_record(&caller.id)?;
    // A lead session is offered the tool with the unbound session's set, so being bound is checked
    // here.
    if caller.bound_to.is_some() {
        bail!(
            "this session reports to an owner, so it does not archive itself: call `report` with \
             status `done` and no open items, which archives it once the report is accepted"
        );
    }
    if caller.locked {
        bail!("this session is locked, so it cannot be archived; the user has to unlock it first");
    }
    let team = coordinator::team(state, &caller)?;
    let locked: Vec<Session> = team
        .iter()
        .filter(|session| session.locked)
        .cloned()
        .collect();
    if !locked.is_empty() {
        bail!(
            "this session cannot archive itself while sessions under it are locked: {}. The user \
             has to unlock them first",
            coordinator::session_labels(&locked)
        );
    }
    let unarchived: Vec<Session> = team
        .into_iter()
        .filter(|session| session.status != SessionStatus::Archived)
        .collect();
    if let Some(refusal) = unarchived_refusal(&caller.id, &unarchived) {
        bail!(refusal);
    }
    Ok(caller)
}

/// What refuses `archive_self` while sessions under the caller are not archived, or `None` when
/// there are none. A session bound to a lead session is not the caller's to archive, so it is named
/// under its lead with different advice from the caller's direct sessions.
fn unarchived_refusal(caller_id: &str, unarchived: &[Session]) -> Option<String> {
    let (direct, under_leads): (Vec<&Session>, Vec<&Session>) = unarchived
        .iter()
        .partition(|session| session.bound_to.as_deref() == Some(caller_id));
    let mut advice = Vec::new();
    if !direct.is_empty() {
        advice.push(format!(
            "{}: an owner archives its sessions once their tasks are done, so an unarchived one \
             means its task is not finished; finish it and archive it with `archive_session` first",
            coordinator::session_labels(direct)
        ));
    }
    let mut leads: Vec<&str> = under_leads
        .iter()
        .filter_map(|session| session.bound_to.as_deref())
        .collect();
    leads.sort_unstable();
    leads.dedup();
    for lead in leads {
        let named = coordinator::session_labels(
            under_leads
                .iter()
                .copied()
                .filter(|session| session.bound_to.as_deref() == Some(lead)),
        );
        advice.push(format!(
            "{} under the lead session `{lead}`: not yours to archive; wait for the lead session \
             to finish them (its `done` report archives it), or archive `{lead}` with \
             `archive_session`, which takes along its sessions that have no process running and \
             is refused while any has one",
            named
        ));
    }
    (!advice.is_empty()).then(|| {
        format!(
            "this session cannot archive itself while sessions under it are not archived. {}. \
             Then archive yourself",
            advice.join(". ")
        )
    })
}

fn rename_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let target = resolve_owned_session(state, caller, required_str(arguments, "session")?)?;
    let title = raw_text(arguments, "title")?.ok_or_else(|| anyhow!("`title` is required"))?;
    let renamed = coordinator::rename_session(state, &target.id, &title)?;
    describe_session(state, &renamed, caller)
}

fn set_session_pinned(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let target = resolve_owned_session(state, caller, required_str(arguments, "session")?)?;
    let pinned = arguments
        .get("pinned")
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow!("`pinned` is required and must be true or false"))?;
    let updated = coordinator::set_session_pinned(state, &target.id, pinned)?;
    let mut description = describe_session(state, &updated, caller)?;
    description["pinned"] = json!(updated.pinned);
    Ok(description)
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

/// Every running session but the caller, in every console and project, console sessions
/// included: information may go to any of them, and an owner to hand a session to or bind to is one
/// of them.
fn list_sessions(state: &Arc<AppState>, caller: &Session) -> Result<Value> {
    let sessions = state
        .store
        .list_sessions()?
        .iter()
        .filter(|session| session.id != caller.id && !session.status.is_dormant())
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
    let target = resolve_session(state, wanted)?;
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
    let open_items = string_list(arguments, "open_items");

    let note =
        reporting::deliver_own_report(state, &session.id, summary.to_string(), status, open_items)
            .await?;
    Ok(json!({ "note": note }))
}

// -- session requests --------------------------------------------------------

/// Asks the user for a session with no owner, and waits for the answer; see
/// `crate::session_request`. Read and checked as `new_session` is, in whichever project or
/// console the call names.
async fn request_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let (console, project) = match required_str(arguments, "kind")? {
        "project" => {
            if optional_string(arguments, "console").is_some() {
                bail!(
                    "`console` is for `kind: console`; a project session is in its project's \
                     console"
                );
            }
            let project = match optional_string(arguments, "project") {
                Some(wanted) => find_project(state, caller, &wanted)?,
                None if caller.role == Role::Project => own_project(state, caller, None)?,
                None => bail!("`project` is required for a session in a project"),
            };
            let console = state
                .store
                .get_console(&project.console_id)?
                .ok_or_else(|| anyhow!("the project's console is no longer on record"))?;
            (console, Some(project))
        }
        "console" => {
            if optional_string(arguments, "project").is_some() {
                bail!("`project` is for `kind: project`; a console session is in no project");
            }
            if optional_string_list(arguments, "branches")?.is_some_and(|names| !names.is_empty()) {
                bail!(
                    "`branches` is for `kind: project`; a console session is in no project, so \
                     there is no branch to hand it"
                );
            }
            let console = match optional_string(arguments, "console") {
                Some(wanted) => find_console(state, &wanted)?,
                None => own_console(state, caller)?,
            };
            (console, None)
        }
        other => bail!("`kind` is `{other}`, which is not one of `project` or `console`"),
    };
    let handover = match &project {
        Some(project) => branches_to_hand_over(state, caller, project, arguments).await?,
        None => None,
    };
    let open = session_in(state, &console, project, arguments)?;
    // Whoever reads the brief is to know who wrote it, and that it is not that session's to drive.
    let handed = handover.as_ref().map_or_else(String::new, |handover| {
        let (noun, them) = session_request::branch_noun_and_pronoun(&handover.branches);
        format!(
            " It also handed the {noun} {} over to you, with the same approval: you are now the \
             session responsible for {them}, so carry on the work on {them}.",
            session_request::named_branches(&handover.branches)
        )
    });
    let task = open.task.map(|brief| {
        format!(
            "You were started at the request of the session `{id}` (\"{title}\"), with the user's \
             approval. Nobody owns you but the user: that session is not your owner, and it and \
             you can only share information with each other.{handed} This is the brief it \
             wrote:\n\n{brief}",
            id = caller.id,
            title = sharing::one_line(&caller.title),
        )
    });
    let asked = OpenRequest {
        task,
        // The user's own, not the caller's: nobody owns it but them.
        origin: Origin::User,
        ..open
    };
    session_request::ask_for_session(
        state,
        caller,
        asked,
        handover,
        session_request::ANSWER_TIME_LIMIT,
    )
    .await
}

/// The branches `request_session` hands over, from its `branches` argument: local branches of the
/// repository `project` is in that the caller is responsible for, each named once. `None` when
/// the argument names none. Refused before the user is asked, with the first branch that is not
/// the caller's to give.
async fn branches_to_hand_over(
    state: &Arc<AppState>,
    caller: &Session,
    project: &Project,
    arguments: &Map<String, Value>,
) -> Result<Option<Handover>> {
    let Some(names) = optional_string_list(arguments, "branches")? else {
        return Ok(None);
    };
    let mut branches: Vec<String> = Vec::new();
    for name in &names {
        let name = name.trim();
        if name.is_empty() {
            bail!("`branches` must not hold a blank name");
        }
        let wire = wire_path::encode(name.as_bytes());
        if !branches.contains(&wire) {
            branches.push(wire);
        }
    }
    if branches.is_empty() {
        return Ok(None);
    }
    // Finding the repository reads the filesystem and the holder check reads the store.
    let (repository, not_held) = {
        let state = state.clone();
        let path = project.path.clone();
        let (branches, caller) = (branches.clone(), caller.id.clone());
        tokio::task::spawn_blocking(move || {
            let Some(repository) = branch_origin::repository_of(Path::new(&path)) else {
                return Ok(None);
            };
            let not_held =
                branch_origin::first_not_held_by(&state, &repository, &branches, &caller)?;
            Ok::<_, anyhow::Error>(Some((repository, not_held)))
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
        bail!(
            "{} is not a branch you are responsible for in the repository of the project `{}` \
             (\"{}\"): it does not exist there, or it was not created by you or handed to you, or \
             another session is responsible for it now. Only a branch you are responsible for can \
             be handed over",
            session_request::named_branches(&[branch]),
            project.id,
            project.name
        );
    }
    Ok(Some(Handover {
        repository,
        branches,
    }))
}

// -- argument and record helpers ---------------------------------------------

/// Finds a session the caller may read: any session on record, of any console or project. What a
/// session may *do* to another is narrower, and is [`resolve_owned_session`]'s.
fn resolve_session(state: &Arc<AppState>, wanted: &str) -> Result<Session> {
    state
        .store
        .get_session(wanted)?
        .ok_or_else(|| anyhow!("there is no session `{wanted}`"))
}

/// Finds a session the caller may act on: one it can read that is bound to it. Every tool that
/// writes into another session goes through this and nothing else, so the rule that reads are wide
/// and writes are the caller's own is kept in one place. A session that is unbound, or bound to
/// another session, is refused with a reason (naming the owner when there is one), so the model
/// leaves it alone rather than retrying.
pub(super) fn resolve_owned_session(
    state: &Arc<AppState>,
    caller: &Session,
    wanted: &str,
) -> Result<Session> {
    let session = resolve_session(state, wanted)?;
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
            "session `{wanted}` is a console session; acting on one is the user's to do, not \
             yours"
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
                    format!("{article} {} session", owner.role.kind())
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

/// `caller` is the session asking. Everything here is the same whoever asks except `yours`, which
/// is relative to `caller`: a console may hold several console sessions, and each reads a
/// session's owner alike but is told only of its own that they are its to act on.
fn describe_session(state: &Arc<AppState>, session: &Session, caller: &Session) -> Result<Value> {
    let owner_kind = match &session.bound_to {
        Some(owner) => state
            .store
            .get_session(owner)?
            .map(|owner| owner.role.kind()),
        None => None,
    };
    Ok(json!({
        "session": session.id,
        "title": session.title,
        "agent": session.agent,
        "status": session.status,
        "role": session.role,
        "console": session.console_id,
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
pub(super) mod tests {
    use super::*;
    use crate::protocol::{
        Agent, AgentAvailability, Availability, Console, Project, ProjectAccount, ProjectSource,
    };
    use crate::store::LOCAL_HOST_ID;
    use crate::test_support::{output_containing, running, ScratchDir};

    fn console() -> Console {
        Console {
            id: "console-1".to_string(),
            name: "Console".to_string(),
            workdir: "/tmp/console-1".to_string(),
            console_session_agent: Agent::Claude,
            default_agent: Agent::Claude,
            project_session_agent: None,
            claude_project_session_model: None,
            claude_project_session_effort: None,
            codex_project_session_model: None,
            codex_project_session_effort: None,
            grok_project_session_model: None,
            grok_project_session_effort: None,
            claude_account_id: None,
            codex_account_id: None,
            grok_account_id: None,
            claude_console_session_model: None,
            claude_console_session_effort: None,
            codex_console_session_model: None,
            codex_console_session_effort: None,
            grok_console_session_model: None,
            grok_console_session_effort: None,
            icon: None,
            console_session_prompt: None,
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
            terminal_title: None,
            record_titles: Default::default(),
            usage: None,
            observed_model: None,
            observed_effort: None,
            model: None,
            effort: None,
            has_conversation: false,
            bound_to: None,
            lead: false,
            colour: None,
            ordinal: None,
            account_id: None,
            config_dir: None,
            pinned: false,
            locked: false,
            started_at: 0,
            ended_at: None,
        }
    }

    /// The refusal `coordinator::open_session` raises for an unavailable agent is not only the
    /// control socket's — a console session reaches it through its own `new_session` tool, and
    /// comes back with a tool error carrying the reason in prose, as "The console session's
    /// tools" in `docs/product/hub-orchestration.md` promises for any refusal.
    #[tokio::test]
    async fn an_unavailable_agent_refuses_the_console_sessions_own_new_session_tool() {
        let (state, _dir) =
            crate::test_support::app_state("mcp-exec-new-session-agent-unavailable");
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
                claude_account_id: ProjectAccount::Inherit,
                codex_account_id: ProjectAccount::Inherit,
                grok_account_id: ProjectAccount::Inherit,
                claude_session_model: None,
                claude_session_effort: None,
                codex_session_model: None,
                codex_session_effort: None,
                grok_session_model: None,
                grok_session_effort: None,
                trust_consent: false,
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

        let err = call(&state, &owner.id, "new_session", &arguments)
            .await
            .expect_err("refused");
        assert!(err.to_string().contains("Claude Code"), "{err}");
    }

    /// A session request is refused before the user is asked whenever the same call to
    /// `new_session` would be, and so is one whose arguments do not say where the session would
    /// be.
    #[tokio::test]
    async fn a_session_request_is_refused_before_the_user_is_asked() {
        let (state, _dir) = shared_console_state("request-refused-early");
        state.set_agent_availability(vec![AgentAvailability {
            agent: Agent::Claude,
            availability: Availability::Unavailable,
            default_account_dir: Some("/home/user/.claude".to_string()),
        }]);
        for (caller, extra, expected) in [
            ("loose", json!({ "kind": "project" }), "Claude Code"),
            ("loose", json!({ "kind": "nobody" }), "`kind` is `nobody`"),
            ("a", json!({ "kind": "project" }), "`project` is required"),
            (
                "loose",
                json!({ "kind": "project", "console": "console-1" }),
                "`console` is for",
            ),
            (
                "loose",
                json!({ "kind": "console", "console": "nowhere" }),
                "no console",
            ),
        ] {
            let mut args = arguments(json!({ "brief": { "goal": "do the thing" } }));
            args.extend(extra.as_object().unwrap().clone());
            let err = call(&state, caller, "request_session", &args)
                .await
                .expect_err("refused");
            assert!(err.to_string().contains(expected), "{err}");
            assert!(!state.session_requests.is_waiting_for(caller));
        }
    }

    /// Where the session asked for would be: the named project of whichever console, a console
    /// session of the caller's console, or, for a project session naming none, its own project.
    #[tokio::test]
    async fn a_session_request_is_for_wherever_the_call_names() {
        let (state, _dir) = shared_console_state("request-where");
        for (caller, extra, (role, console, project)) in [
            (
                "a",
                json!({ "kind": "project", "project": "project-2" }),
                (Role::Project, "console-2", Some("project-2")),
            ),
            (
                "loose",
                json!({ "kind": "console" }),
                (Role::Console, "console-1", None),
            ),
            (
                "of-a",
                json!({ "kind": "project" }),
                (Role::Project, "console-1", Some("project-1")),
            ),
        ] {
            let mut args = arguments(json!({ "brief": { "goal": "do the thing" } }));
            args.extend(extra.as_object().unwrap().clone());
            let Event::SessionRequest {
                role: waiting_role,
                console: waiting_console,
                project: waiting_project,
                ..
            } = withdrawn_request(&state, caller, args).await
            else {
                unreachable!()
            };
            assert_eq!(
                (
                    waiting_role,
                    waiting_console.as_str(),
                    waiting_project.as_deref()
                ),
                (role, console, project),
                "{caller}"
            );
        }
    }

    /// The branches a request hands over are the caller's to give in the project's own repository:
    /// anything else is refused before the user is asked, and what is accepted reaches the dialog
    /// once and by name.
    #[tokio::test]
    async fn a_session_request_hands_over_only_branches_the_caller_is_responsible_for() {
        let (state, _dir) = shared_console_state("request-branches");
        let repo = crate::test_support::repo_with("request-branches-repo", &[(b"a", b"a")]);
        state
            .store
            .insert_project(&Project {
                id: "repo".to_string(),
                name: "repo".to_string(),
                path: repo.to_string_lossy().into_owned(),
                ..state.store.get_project("project-1").unwrap().unwrap()
            })
            .unwrap();
        crate::test_support::branch_made_by(&state, &repo, "mine", "loose");
        crate::test_support::branch_made_by(&state, &repo, "theirs", "a");
        crate::test_support::git(&repo, &["branch", "unrecorded"]);
        let request = |extra: Value| {
            let mut args = arguments(json!({ "brief": { "goal": "do the thing" } }));
            args.extend(extra.as_object().unwrap().clone());
            args
        };

        for (extra, expected) in [
            (
                json!({ "kind": "project", "project": "repo", "branches": ["theirs"] }),
                "`theirs` is not a branch you are responsible for",
            ),
            (
                json!({ "kind": "project", "project": "repo", "branches": ["mine", "unrecorded"] }),
                "`unrecorded` is not a branch you are responsible for",
            ),
            (
                json!({ "kind": "project", "project": "repo", "branches": ["mine", "absent"] }),
                "`absent` is not a branch you are responsible for",
            ),
            (
                json!({ "kind": "project", "project": "repo", "branches": ["mine", " "] }),
                "blank name",
            ),
            (
                json!({ "kind": "project", "project": "repo", "branches": "mine" }),
                "array of strings",
            ),
            (
                json!({ "kind": "project", "project": "project-2", "branches": ["mine"] }),
                "not in a git repository",
            ),
            (
                json!({ "kind": "console", "branches": ["mine"] }),
                "`branches` is for `kind: project`",
            ),
        ] {
            let err = refused_promptly(&state, &request(extra)).await;
            assert!(err.to_string().contains(expected), "{err}");
            assert!(!state.session_requests.is_waiting_for("loose"));
        }

        // An empty list is no branches at all, for either kind, and a repeated name is one branch.
        for (extra, expected) in [
            (
                json!({ "kind": "console", "branches": [] }),
                Vec::<&str>::new(),
            ),
            (
                json!({ "kind": "project", "project": "repo", "branches": [] }),
                Vec::new(),
            ),
            (
                json!({ "kind": "project", "project": "repo", "branches": ["mine", "mine"] }),
                vec!["mine"],
            ),
        ] {
            let event = withdrawn_request(&state, "loose", request(extra)).await;
            assert!(
                matches!(&event, Event::SessionRequest { branches, .. } if branches == &expected),
                "{event:?}"
            );
        }

        // A branch deleted and made again is another branch, which the caller never was
        // responsible for, though a branch of that name is there.
        crate::test_support::branch_made_by(&state, &repo, "recreated", "loose");
        crate::test_support::git(&repo, &["branch", "-D", "recreated"]);
        crate::test_support::git(&repo, &["commit", "-q", "--allow-empty", "-m", "later"]);
        crate::test_support::git(&repo, &["branch", "recreated"]);
        let err = refused_promptly(
            &state,
            &request(json!({ "kind": "project", "project": "repo", "branches": ["recreated"] })),
        )
        .await;
        assert!(
            err.to_string()
                .contains("`recreated` is not a branch you are responsible for"),
            "{err}"
        );
    }

    /// An approval starts the session with a brief that says who wrote it, and, when the request
    /// handed branches over, that they are its to work on.
    #[tokio::test]
    async fn the_started_sessions_brief_says_which_branches_it_was_handed() {
        let (state, _dir) = shared_console_state("request-brief");
        let repo = crate::test_support::repo_with("request-brief-repo", &[(b"a", b"a")]);
        state
            .store
            .insert_project(&Project {
                id: "repo".to_string(),
                name: "repo".to_string(),
                path: repo.to_string_lossy().into_owned(),
                ..state.store.get_project("project-1").unwrap().unwrap()
            })
            .unwrap();
        for branch in ["one", "two", "three"] {
            crate::test_support::branch_made_by(&state, &repo, branch, "loose");
        }
        let _caller = running(&state, "loose");

        for (branches, between) in [
            (
                json!(["one"]),
                " It also handed the branch `one` over to you, with the same approval: you are now \
                 the session responsible for it, so carry on the work on it. This is the brief it \
                 wrote:",
            ),
            (
                json!(["two", "three"]),
                " It also handed the branches `two`, `three` over to you, with the same approval: \
                 you are now the session responsible for them, so carry on the work on them. This \
                 is the brief it wrote:",
            ),
            (json!([]), " This is the brief it wrote:"),
        ] {
            let args = arguments(json!({
                "kind": "project",
                "project": "repo",
                "branches": branches,
                "brief": { "goal": "do the thing" },
            }));
            let task = approved_task(&state, "loose", args).await;
            assert!(
                task.contains(&format!("share information with each other.{between}")),
                "{task}"
            );
        }
    }

    /// The refusal of the request `args` as `loose`, which is made before the user is asked: a call
    /// that is not refused would wait for an answer, so it is cut short.
    async fn refused_promptly(state: &Arc<AppState>, args: &Map<String, Value>) -> anyhow::Error {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            call(state, "loose", "request_session", args),
        )
        .await
        .expect("the call is refused without waiting for the user")
        .expect_err("refused")
    }

    /// Makes the request `args` as `caller`, withdraws it once it waits, and returns what it asked.
    async fn withdrawn_request(
        state: &Arc<AppState>,
        caller: &'static str,
        args: Map<String, Value>,
    ) -> Event {
        let call = {
            let state = state.clone();
            tokio::spawn(async move { call(&state, caller, "request_session", &args).await })
        };
        let event = wait_for_request(state, caller).await;
        session_request::withdraw_for_session(state, caller);
        call.await.unwrap().expect_err("withdrawn");
        event
    }

    /// Makes the request `args` as `caller` and approves it with a launch that starts nothing but
    /// records a session; returns the brief the session was started with.
    async fn approved_task(
        state: &Arc<AppState>,
        caller: &'static str,
        args: Map<String, Value>,
    ) -> String {
        let call = {
            let state = state.clone();
            tokio::spawn(async move { call(&state, caller, "request_session", &args).await })
        };
        let Event::SessionRequest { request_id, .. } = wait_for_request(state, caller).await else {
            unreachable!()
        };
        let task = Arc::new(std::sync::Mutex::new(None));
        let launch = |open: coordinator::OpenRequest| {
            let (state, task) = (state.clone(), task.clone());
            async move {
                *task.lock().unwrap() = open.task.clone();
                let started = Session {
                    project_id: open.project_id.clone(),
                    ..project_session(&Uuid::new_v4().to_string(), &open.console_id, None)
                };
                state.store.insert_session(&started)?;
                running(&state, &started.id);
                Ok(started)
            }
        };
        session_request::answer_with(state, &request_id, true, launch)
            .await
            .unwrap()
            .unwrap();
        call.await.unwrap().expect("approved");
        let task = task
            .lock()
            .unwrap()
            .take()
            .expect("the session was started");
        task
    }

    /// The waiting request of `caller`, once it is there.
    async fn wait_for_request(state: &Arc<AppState>, caller: &str) -> Event {
        let deadline = std::time::Instant::now() + crate::test_support::PATIENCE;
        loop {
            let found = session_request::pending_events(state).into_iter().find(
                |event| matches!(event, Event::SessionRequest { session, .. } if session == caller),
            );
            if let Some(event) = found {
                return event;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the request never waited"
            );
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
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
                    claude_account_id: ProjectAccount::Inherit,
                    codex_account_id: ProjectAccount::Inherit,
                    grok_account_id: ProjectAccount::Inherit,
                    claude_session_model: None,
                    claude_session_effort: None,
                    codex_session_model: None,
                    codex_session_effort: None,
                    grok_session_model: None,
                    grok_session_effort: None,
                    trust_consent: false,
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

    pub(in crate::mcp) fn arguments(value: Value) -> Map<String, Value> {
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

    /// A session whose process has ended still has its last output read, through the file the
    /// terminal socket replays, capped and with escape sequences stripped. One with nothing kept
    /// has no output to show.
    #[tokio::test]
    async fn a_session_whose_process_has_ended_still_shows_its_kept_output() {
        let (state, dir) = shared_console_state("tool-surface-kept-output");
        let output_dir = dir.join("output");
        std::fs::create_dir_all(&output_dir).unwrap();
        let mut kept = vec![b'a'; 10 * 1024];
        kept.extend_from_slice(b"\x1b[1mlast words\x1b[0m");
        std::fs::write(output_dir.join("archived-of-a"), kept).unwrap();

        let read = |session: &'static str| {
            let state = state.clone();
            async move {
                call(
                    &state,
                    "a",
                    "get_session",
                    &arguments(json!({ "session": session })),
                )
                .await
                .unwrap()
            }
        };
        let archived = read("archived-of-a").await;
        assert_eq!(archived["status"], "archived");
        let tail = archived["recent_output"].as_str().expect("kept output");
        assert!(tail.trim_end().ends_with("last words"), "{tail:?}");
        assert!(!tail.contains('\x1b'), "{tail:?}");
        assert!(tail.len() <= 8 * 1024, "{}", tail.len());
        assert!(read("archived-of-b").await["recent_output"].is_null());
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
            ("send_message", "b", "is a console session;"),
            ("send_message", "foreign", "not bound to any session"),
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
    pub(in crate::mcp) fn peer_state(name: &str) -> (Arc<AppState>, ScratchDir) {
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
                claude_account_id: ProjectAccount::Inherit,
                codex_account_id: ProjectAccount::Inherit,
                grok_account_id: ProjectAccount::Inherit,
                claude_session_model: None,
                claude_session_effort: None,
                codex_session_model: None,
                codex_session_effort: None,
                grok_session_model: None,
                grok_session_effort: None,
                trust_consent: false,
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
            ("mine", "new_session"),
            ("mine", "send_message"),
            ("of-a", "archive_session"),
            ("starter", "list_projects"),
            ("starter", "add_project"),
            ("starter", "show_page"),
            ("a", "report"),
            ("mine", "transfer_session"),
            ("mine", "bind_to_session"),
            ("a", "bind_to_session"),
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

    /// `peer_state` with `starter` bound to the console session `a` as its lead session, still
    /// owning `mine`.
    fn lead_state(name: &str) -> (Arc<AppState>, ScratchDir) {
        let (state, dir) = peer_state(name);
        state
            .store
            .set_binding("starter", None, "a", true, false)
            .unwrap()
            .unwrap();
        (state, dir)
    }

    /// A lead session keeps the unbound project session's tools, so its `new_session` gets as
    /// far as the launch (an unavailable agent is refused there), binding the new session to it,
    /// and its `bind_to_session` is refused in prose; a session bound to it still has no
    /// `new_session`.
    #[tokio::test]
    async fn a_lead_session_keeps_its_tools_and_its_sessions_still_cannot_start_any() {
        let (state, _dir) = lead_state("lead-tools");
        state.set_agent_availability(vec![AgentAvailability {
            agent: Agent::Claude,
            availability: Availability::Unavailable,
            default_account_dir: Some("/home/user/.claude".to_string()),
        }]);
        let brief = arguments(json!({ "brief": { "goal": "x" }, "agent": "claude" }));

        let err = call(&state, "starter", "new_session", &brief)
            .await
            .expect_err("agent unavailable");
        assert!(err.to_string().contains("Claude Code"), "{err}");
        let err = call(&state, "mine", "new_session", &brief)
            .await
            .expect_err("not offered");
        assert!(err.to_string().contains("is not a tool"), "{err}");

        // It is still offered `bind_to_session`, and its call is refused in prose: only an unbound
        // session binds itself.
        let request = arguments(json!({ "owner": "b", "summary": "y" }));
        let err = call(&state, "starter", "bind_to_session", &request)
            .await
            .expect_err("already bound");
        assert!(err.to_string().contains("already bound to `a`"), "{err}");
    }

    /// The console session reads a lead session as bound to a console session and yours, and the
    /// lead session's own sessions as bound to a project session and not yours; acting on one of
    /// those is refused, naming the lead session as its owner.
    #[tokio::test]
    async fn the_console_session_reads_a_lead_sessions_sessions_but_cannot_drive_them() {
        let (state, _dir) = lead_state("lead-reads");
        for (target, owner, kind, yours) in [
            ("starter", "a", "console", true),
            ("mine", "starter", "project", false),
        ] {
            let read = call(
                &state,
                "a",
                "get_session",
                &arguments(json!({ "session": target })),
            )
            .await
            .unwrap();
            assert_eq!(read["owner"], owner, "{target}");
            assert_eq!(read["owner_kind"], kind, "{target}");
            assert_eq!(read["yours"], yours, "{target}");
        }
        for tool in ["send_message", "archive_session"] {
            let err = call(
                &state,
                "a",
                tool,
                &arguments(json!({ "session": "mine", "text": "hello" })),
            )
            .await
            .expect_err("not the console session's");
            assert!(
                err.to_string()
                    .contains("bound to a project session, `starter`"),
                "{tool}: {err}"
            );
        }
        assert_eq!(
            state.store.get_session("mine").unwrap().unwrap().status,
            SessionStatus::Idle
        );
    }

    /// The status of a session, set directly: the setup of the `archive_self` tests.
    fn with_status(state: &Arc<AppState>, id: &str, status: SessionStatus) {
        let mut session = state.store.get_session(id).unwrap().unwrap();
        session.status = status;
        state.store.update_session(&session).unwrap();
    }

    async fn archive_self_error(state: &Arc<AppState>, caller: &str) -> String {
        call(state, caller, "archive_self", &Map::new())
            .await
            .expect_err("refused")
            .to_string()
    }

    /// An owner archives its sessions once their tasks are done, so one that is not archived —
    /// running or interrupted, under an unbound project session or two levels under a console
    /// session, below its lead session — refuses the call, names that session alone, and changes
    /// nothing.
    #[tokio::test]
    async fn a_session_with_an_unarchived_session_under_it_cannot_archive_itself() {
        let (peers, _peers_dir) = peer_state("archive-self-unarchived-peer");
        let (leads, _leads_dir) = lead_state("archive-self-unarchived-lead");
        // `a` owns `of-a` and the lead session `starter`, which owns `mine`: the other two are
        // archived, so `mine` is what is named.
        with_status(&leads, "of-a", SessionStatus::Archived);
        with_status(&leads, "starter", SessionStatus::Archived);
        for (state, caller) in [(&peers, "starter"), (&leads, "a")] {
            for status in [SessionStatus::Idle, SessionStatus::Interrupted] {
                with_status(state, "mine", status);
                let message = archive_self_error(state, caller).await;
                assert!(message.contains("`mine` (\"Work mine\")"), "{message}");
                assert!(!message.contains("`of-a`"), "{message}");
                if caller == "a" {
                    assert!(
                        message.contains("under the lead session `starter`"),
                        "{message}"
                    );
                } else {
                    assert!(message.contains("`archive_session`"), "{message}");
                }
                let after = state.store.get_session(caller).unwrap().unwrap();
                assert_ne!(after.status, SessionStatus::Archived, "{caller}");
                assert!(after.ended_at.is_none(), "{caller}");
            }
        }
    }

    /// A lead session is offered the tool with the unbound session's set, yet it reports to its
    /// console session and wraps up with `report`, so the call is refused in prose and changes
    /// nothing.
    #[tokio::test]
    async fn a_lead_session_is_told_to_report_done_instead_of_archiving_itself() {
        let (state, _dir) = lead_state("archive-self-lead");
        // Archived, so that being bound is the only thing that could refuse it.
        with_status(&state, "mine", SessionStatus::Archived);
        let message = archive_self_error(&state, "starter").await;
        assert!(message.contains("`report`"), "{message}");
        let after = state.store.get_session("starter").unwrap().unwrap();
        assert_ne!(after.status, SessionStatus::Archived);
    }

    /// A locked session is not archived, by itself either, and a locked session under it is
    /// named.
    #[tokio::test]
    async fn a_locked_session_cannot_archive_itself() {
        let (state, _dir) = peer_state("archive-self-locked");
        with_status(&state, "mine", SessionStatus::Archived);
        state.set_session_locked("mine", true).unwrap();
        let message = archive_self_error(&state, "starter").await;
        assert!(message.contains("`mine` (\"Work mine\")"), "{message}");
        state.set_session_locked("mine", false).unwrap();
        state.set_session_locked("starter", true).unwrap();
        let message = archive_self_error(&state, "starter").await;
        assert!(message.contains("is locked"), "{message}");
        let after = state.store.get_session("starter").unwrap().unwrap();
        assert_ne!(after.status, SessionStatus::Archived);
    }

    /// `archive_session` on the caller's own session points to `archive_self`.
    #[tokio::test]
    async fn archiving_ones_own_session_with_archive_session_points_to_archive_self() {
        let (state, _dir) = peer_state("archive-self-pointer");
        let err = call(
            &state,
            "starter",
            "archive_session",
            &arguments(json!({ "session": "starter" })),
        )
        .await
        .expect_err("refused");
        assert!(err.to_string().contains("`archive_self`"), "{err}");
    }

    /// With every session under it archived, the call archives the caller and nobody else, and its
    /// process is ended, for a console session and an unbound project session alike.
    #[tokio::test]
    async fn archiving_oneself_archives_the_caller_alone() {
        let (state, _dir) = peer_state("archive-self-success");
        // `a` owns `of-a` and `archived-of-a`; `starter` owns `mine`.
        with_status(&state, "of-a", SessionStatus::Archived);
        with_status(&state, "mine", SessionStatus::Archived);
        let live = running(&state, "starter");
        for caller in ["a", "starter"] {
            let answer = call(&state, caller, "archive_self", &Map::new())
                .await
                .unwrap();
            assert_eq!(answer["session"], caller);
            assert_eq!(answer["status"], "archived");
            let after = state.store.get_session(caller).unwrap().unwrap();
            assert_eq!(after.status, SessionStatus::Archived, "{caller}");
            assert!(after.ended_at.is_some(), "{caller}");
        }
        for untouched in ["b", "of-b", "loose", "other-project"] {
            let session = state.store.get_session(untouched).unwrap().unwrap();
            assert_eq!(session.status, SessionStatus::Idle, "{untouched}");
        }
        // Ending the process is left to a blocking task; wait for it.
        let deadline = std::time::Instant::now() + crate::test_support::PATIENCE;
        while !live.poll_exit() && std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(live.poll_exit());
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
                "new_session",
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
    async fn an_unbound_project_sessions_new_session_reaches_the_launch_in_its_own_project() {
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
            let err = call(&state, "starter", "new_session", &input)
                .await
                .expect_err("agent unavailable");
            assert!(
                err.to_string().contains("Claude Code"),
                "{project:?}: {err}"
            );
        }
    }

    /// Reads reach any session, of any project or console, console sessions included, and say who
    /// owns what, including a project session as owner.
    #[tokio::test]
    async fn a_project_session_reads_any_session() {
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
                .unwrap()
            }
        };
        let mine = read("mine").await;
        assert_eq!(mine["owner"], "starter");
        assert_eq!(mine["owner_kind"], "project");
        assert_eq!(mine["yours"], true);
        let theirs = read("of-a").await;
        assert_eq!(theirs["owner_kind"], "console");
        assert_eq!(theirs["yours"], false);
        assert!(read("loose").await["owner_kind"].is_null());
        for (outside, console) in [
            ("other-project", "console-1"),
            ("a", "console-1"),
            ("foreign", "console-2"),
        ] {
            let read = read(outside).await;
            assert_eq!(read["console"], console, "{outside}");
            assert_eq!(read["yours"], false, "{outside}");
        }
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
            ("send_message", "other-project", "not bound to any session"),
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

    /// Every session sees every other running session, of every console and project, console
    /// sessions included, with their owners, which one owns it and which are its own; archived
    /// sessions are left out.
    #[tokio::test]
    async fn every_session_lists_the_running_sessions_everywhere() {
        let (state, _dir) = peer_state("peer-list");
        let listed = call(&state, "mine", "list_sessions", &Map::new())
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
        assert_eq!(
            ids,
            [
                "a",
                "b",
                "foreign",
                "loose",
                "of-a",
                "of-b",
                "other-project",
                "starter"
            ]
        );
        for session in listed["sessions"].as_array().unwrap() {
            assert_eq!(
                session["your_owner"],
                session["session"] == "starter",
                "{session}"
            );
        }

        let from_console = call(&state, "a", "list_sessions", &Map::new())
            .await
            .unwrap();
        assert!(from_console["your_owner"].is_null());
        let of_a = from_console["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|session| session["session"] == "of-a")
            .unwrap();
        assert_eq!(
            (&of_a["owner_kind"], &of_a["yours"]),
            (&json!("console"), &json!(true))
        );
    }

    /// Information reaches any running session but the caller: one of another project or console,
    /// and a console session, which can share information itself too.
    #[tokio::test]
    async fn information_reaches_any_running_session_but_the_caller() {
        let (state, _dir) = peer_state("peer-share-scope");
        let console_a = running(&state, "a");
        let foreign = running(&state, "foreign");
        let loose = running(&state, "loose");
        for (caller, target, live) in [
            ("mine", "a", &console_a),
            ("mine", "foreign", &foreign),
            ("a", "loose", &loose),
        ] {
            let result = call(
                &state,
                caller,
                "share_info",
                &arguments(json!({ "session": target, "text": "the port moved" })),
            )
            .await
            .unwrap();
            assert_eq!(result["delivered"], true, "{caller} to {target}");
            let seen = output_containing(live, "the port moved");
            assert!(
                seen.contains("not your owner"),
                "{caller} to {target}: {seen}"
            );
        }
        let err = call(
            &state,
            "mine",
            "share_info",
            &arguments(json!({ "session": "mine", "text": "hello" })),
        )
        .await
        .expect_err("the caller itself");
        assert!(err.to_string().contains("your own session"), "{err}");
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

    /// `shared_console_state`, with the console's working directory somewhere a console's save
    /// may write its instruction files, a codex account, and a remote URL carrying a token on the
    /// console's project.
    fn settings_state(name: &str) -> (Arc<AppState>, ScratchDir) {
        let (state, dir) = shared_console_state(name);
        let workdir = dir.join("workdir");
        std::fs::create_dir(&workdir).unwrap();
        state
            .store
            .update_console(&Console {
                workdir: workdir.to_string_lossy().into_owned(),
                ..console()
            })
            .unwrap();
        state
            .store
            .insert_account(&crate::protocol::Account {
                id: "acct-codex".to_string(),
                agent: Agent::Codex,
                name: "Work".to_string(),
                config_dir: "/secret/codex".to_string(),
                approval_reviewer: None,
            })
            .unwrap();
        git_remote::record_remote_url(
            &state,
            "project-1",
            Some("https://user:token@example.com/r.git".to_string()),
        );
        (state, dir)
    }

    async fn refused(state: &Arc<AppState>, caller: &str, tool: &str, args: Value) -> String {
        call(state, caller, tool, &arguments(args))
            .await
            .expect_err("refused")
            .to_string()
    }

    /// `update_project` leaves what it is not given alone, adds and removes tags without caring
    /// about case, resolves an account by name within the agent, and refuses what the shared
    /// function refuses (a model the agent does not offer, another agent's account, a blank name)
    /// without writing any of it; another console's project is out of reach.
    #[tokio::test]
    async fn update_project_changes_only_what_it_is_given() {
        let (state, _dir) = settings_state("update-project");
        let update = |args: Value| {
            let state = state.clone();
            async move { call(&state, "a", "update_project", &arguments(args)).await }
        };

        let first = update(json!({
            "project": "project-1",
            "tags": ["Backend", "api"],
            "pinned": true,
            "accounts": { "codex": "work", "grok": "default" },
            "launch": { "claude": { "model": "opus", "effort": "high" } },
        }))
        .await
        .unwrap();
        assert_eq!(first["tags"], json!(["Backend", "api"]));
        assert_eq!(first["accounts"]["codex"], "acct-codex");
        assert_eq!(first["accounts"]["grok"], "default");
        assert_eq!(first["accounts"]["claude"], "inherit");
        assert_eq!(first["launch"]["claude"]["model"], "opus");

        let second = update(json!({
            "project": "project-1",
            "add_tags": ["API", "docs"],
            "remove_tags": ["backend"],
        }))
        .await
        .unwrap();
        assert_eq!(second["tags"], json!(["api", "docs"]));
        assert_eq!(second["pinned"], true);
        assert_eq!(second["launch"]["claude"]["effort"], "high");
        assert_eq!(second["name"], "project-1");

        // `null` is no list, so it does not empty the tags; a value that is not a list, or a
        // boolean that is not one, is refused rather than ignored.
        let kept = update(json!({ "project": "project-1", "tags": null }))
            .await
            .unwrap();
        assert_eq!(kept["tags"], json!(["api", "docs"]));
        for (args, expected) in [
            (json!({ "tags": "api" }), "array of strings"),
            (json!({ "add_tags": [1] }), "array of strings"),
            (json!({ "pinned": "false" }), "true or false"),
        ] {
            let mut args = args;
            args["project"] = json!("project-1");
            let message = refused(&state, "a", "update_project", args).await;
            assert!(message.contains(expected), "{message}");
        }

        let cleared = update(json!({ "project": "project-1", "default_agent": null }))
            .await
            .unwrap();
        assert!(cleared["default_agent"].is_null());

        for (args, expected) in [
            (json!({ "tags": ["x"], "add_tags": ["y"] }), "cannot both"),
            (json!({ "name": "  " }), "`name` is required"),
            (
                json!({ "launch": { "claude": { "model": "gpt-9" } } }),
                "is not a model",
            ),
            (
                json!({ "accounts": { "claude": "acct-codex" } }),
                "unknown account",
            ),
            (
                json!({ "accounts": { "claude": "Work" } }),
                "unknown account",
            ),
        ] {
            let mut args = args;
            args["project"] = json!("project-1");
            let message = refused(&state, "a", "update_project", args).await;
            assert!(message.contains(expected), "{message}");
        }
        let stored = state.store.get_project("project-1").unwrap().unwrap();
        assert_eq!(stored.tags, ["api", "docs"]);
        assert_eq!(stored.claude_session_model.as_deref(), Some("opus"));

        let message = refused(
            &state,
            "a",
            "update_project",
            json!({ "project": "project-2" }),
        )
        .await;
        assert!(message.contains("no project"), "{message}");
    }

    /// Both listings carry the settings and show the remote without the token in it, and a project
    /// session's `get_project` takes no project: whatever it names, it reads its own.
    #[tokio::test]
    async fn project_settings_are_listed_without_credentials_and_get_project_stays_in_its_own() {
        let (state, _dir) = settings_state("get-project");
        let listed = call(&state, "a", "list_projects", &Map::new())
            .await
            .unwrap();
        let entry = &listed["projects"][0];
        assert_eq!(entry["remote_url"], "https://example.com/r.git");
        assert_eq!(entry["source"], "local");
        assert_eq!(entry["trust_consent"], false);
        assert_eq!(entry["accounts"]["claude"], "inherit");
        assert!(entry["launch"]["grok"]["model"].is_null());
        assert_eq!(entry["sessions"].as_array().unwrap().len(), 3);

        for caller in ["of-a", "loose"] {
            let own = call(
                &state,
                caller,
                "get_project",
                &arguments(json!({ "project": "project-2" })),
            )
            .await
            .unwrap();
            assert_eq!(own["project"], "project-1");
            assert_eq!(own["remote_url"], "https://example.com/r.git");
            assert!(own.get("sessions").is_none());
        }
        let other = call(&state, "foreign", "get_project", &Map::new())
            .await
            .unwrap();
        assert_eq!(other["project"], "project-2");
        // A console session reads projects through `list_projects`.
        assert!(refused(&state, "a", "get_project", json!({}))
            .await
            .contains("not a tool"));
    }

    /// A session is retitled and pinned only by the session it reports to: not one bound to
    /// another session, not an unbound one, not the caller itself, and a bound project session has
    /// no such tools at all. A blank title is refused.
    #[tokio::test]
    async fn only_the_owner_renames_or_pins_a_session() {
        let (state, _dir) = settings_state("rename-pin");
        let renamed = call(
            &state,
            "a",
            "rename_session",
            &arguments(json!({ "session": "of-a", "title": " New title " })),
        )
        .await
        .unwrap();
        assert_eq!(renamed["title"], "New title");
        let pinned = call(
            &state,
            "a",
            "set_session_pinned",
            &arguments(json!({ "session": "of-a", "pinned": true })),
        )
        .await
        .unwrap();
        assert_eq!(pinned["pinned"], true);
        assert!(state.store.get_session("of-a").unwrap().unwrap().pinned);

        let message = refused(
            &state,
            "a",
            "rename_session",
            json!({ "session": "of-a", "title": " " }),
        )
        .await;
        assert!(message.contains("`title` is required"), "{message}");
        for target in ["of-b", "loose", "a"] {
            for (tool, args) in [
                ("rename_session", json!({ "session": target, "title": "x" })),
                (
                    "set_session_pinned",
                    json!({ "session": target, "pinned": true }),
                ),
            ] {
                let message = refused(&state, "a", tool, args).await;
                assert!(
                    message.contains("leave it alone") || message.contains("your own"),
                    "{message}"
                );
            }
        }
        assert_eq!(
            state.store.get_session("of-b").unwrap().unwrap().title,
            "Work of-b"
        );
        let message = refused(
            &state,
            "of-a",
            "rename_session",
            json!({ "session": "loose", "title": "x" }),
        )
        .await;
        assert!(message.contains("not a tool"), "{message}");
    }

    /// The console session reads and changes its own console only, never sees the avatar, and an
    /// account that does not belong to the agent is refused.
    #[tokio::test]
    async fn the_console_session_reads_and_changes_its_own_console() {
        let (state, _dir) = settings_state("console-settings");
        let mut with_icon = state.store.get_console("console-1").unwrap().unwrap();
        with_icon.icon = Some("data:image/png;base64,AAAA".to_string());
        state.store.update_console(&with_icon).unwrap();

        let read = call(&state, "a", "get_console", &Map::new()).await.unwrap();
        assert_eq!(read["console"], "console-1");
        assert_eq!(read["has_icon"], true);
        assert_eq!(read["accounts"]["claude"], "default");
        assert!(read["project_session_agent"].is_null());
        assert!(!read.to_string().contains("AAAA"));

        let updated = call(
            &state,
            "a",
            "update_console",
            &arguments(json!({
                "name": "Renamed",
                "default_agent": "codex",
                "accounts": { "codex": "Work" },
                "launch": { "claude": { "model": "sonnet" } },
            })),
        )
        .await
        .unwrap();
        assert_eq!(updated["name"], "Renamed");
        assert_eq!(updated["default_agent"], "codex");
        assert!(updated["project_session_agent"].is_null());
        assert_eq!(updated["accounts"]["codex"], "acct-codex");
        assert_eq!(updated["launch"]["claude"]["model"], "sonnet");

        // The project session pair is a setting of its own, read and written beside `launch`.
        assert!(updated["project_session_launch"]["claude"]["model"].is_null());
        let pair = call(
            &state,
            "a",
            "update_console",
            &arguments(json!({ "project_session_launch": { "claude": { "model": "sonnet" } } })),
        )
        .await
        .unwrap();
        assert_eq!(pair["project_session_launch"]["claude"]["model"], "sonnet");
        assert_eq!(pair["launch"]["claude"]["model"], "sonnet");
        let stored = state.store.get_console("console-1").unwrap().unwrap();
        assert_eq!(
            stored.claude_project_session_model.as_deref(),
            Some("sonnet")
        );
        let cleared_pair = call(
            &state,
            "a",
            "update_console",
            &arguments(json!({ "project_session_launch": { "claude": { "model": null } } })),
        )
        .await
        .unwrap();
        assert!(cleared_pair["project_session_launch"]["claude"]["model"].is_null());
        assert_eq!(cleared_pair["launch"]["claude"]["model"], "sonnet");

        // Absent leaves the project session agent alone; `null` makes it follow again.
        let set = call(
            &state,
            "a",
            "update_console",
            &arguments(json!({ "project_session_agent": "grok" })),
        )
        .await
        .unwrap();
        assert_eq!(set["project_session_agent"], "grok");
        assert_eq!(set["default_agent"], "codex");
        let kept = call(
            &state,
            "a",
            "update_console",
            &arguments(json!({ "name": "Renamed" })),
        )
        .await
        .unwrap();
        assert_eq!(kept["project_session_agent"], "grok");
        let cleared = call(
            &state,
            "a",
            "update_console",
            &arguments(json!({ "project_session_agent": null })),
        )
        .await
        .unwrap();
        assert!(cleared["project_session_agent"].is_null());
        let stored = state.store.get_console("console-1").unwrap().unwrap();
        assert_eq!(stored.icon, with_icon.icon);
        assert_eq!(
            state.store.get_console("console-2").unwrap().unwrap().name,
            "Console"
        );

        for (args, expected) in [
            (json!({ "name": " " }), "`name` is required"),
            (json!({ "project_session_agent": "gemini" }), "gemini"),
            (
                json!({ "accounts": { "claude": "acct-codex" } }),
                "unknown account",
            ),
            (
                json!({ "launch": { "claude": { "model": "gpt-9" } } }),
                "is not a model",
            ),
        ] {
            let message = refused(&state, "a", "update_console", args).await;
            assert!(message.contains(expected), "{message}");
        }
    }

    /// The settings listing names every agent's default account, never an account's directory, and
    /// the only write is to the two settings a console session may change.
    #[tokio::test]
    async fn settings_are_listed_without_directories_and_two_of_them_are_writable() {
        let (state, _dir) = settings_state("app-settings");
        let updated = call(
            &state,
            "a",
            "update_settings",
            &arguments(json!({ "default_clone_dir": "/tmp/clones" })),
        )
        .await
        .unwrap();
        assert_eq!(updated["default_clone_dir"], "/tmp/clones");
        let read = call(&state, "a", "get_settings", &Map::new())
            .await
            .unwrap();
        assert_eq!(read["accounts"].as_array().unwrap().len(), 4);
        assert_eq!(read["accounts"][0]["id"], "default");
        assert_eq!(read["accounts"][0]["default"], true);
        assert!(read["trusted_directories"].is_array());
        assert!(!read.to_string().contains("/secret"));
        let message = refused(
            &state,
            "a",
            "update_settings",
            json!({ "default_clone_dir": "relative" }),
        )
        .await;
        assert!(message.contains("not an absolute path"), "{message}");
    }

    /// A clone URL with a credential in it is refused without echoing the credential, so is one
    /// that is not a remote at all; a local association takes its settings and returns them.
    #[tokio::test]
    async fn add_project_refuses_a_credentialed_url_and_takes_its_settings() {
        let (state, dir) = settings_state("add-project");
        // Should the refusal ever fail, a clone lands in the scratch directory, not the user's.
        let clones = dir.join("clones");
        state
            .store
            .set_default_clone_dir(Some(&clones.to_string_lossy()))
            .unwrap();
        for (url, expected) in [
            (
                "https://user:token@example.com/r.git",
                "carries credentials",
            ),
            ("https://token@example.com/r.git", "carries credentials"),
            ("/etc/passwd", "can be cloned"),
        ] {
            let args = json!({ "source": "git", "remote_url": url });
            let message = refused(&state, "a", "add_project", args).await;
            assert!(message.contains(expected), "{message}");
            assert!(
                !message.contains("token@") && !message.contains("user:"),
                "{message}"
            );
        }
        assert_eq!(state.store.list_projects().unwrap().len(), 2);

        let repo = dir.join("repo");
        std::fs::create_dir(&repo).unwrap();
        let added = call(
            &state,
            "a",
            "add_project",
            &arguments(json!({
                "source": "local",
                "path": repo.to_string_lossy(),
                "tags": ["one"],
                "pinned": true,
                "accounts": { "codex": "acct-codex" },
                "launch": { "claude": { "effort": "low" } },
            })),
        )
        .await
        .unwrap();
        let project = &added["projects"][0];
        assert_eq!(project["pinned"], true);
        assert_eq!(project["tags"], json!(["one"]));
        assert_eq!(project["accounts"]["codex"], "acct-codex");
        assert_eq!(project["launch"]["claude"]["effort"], "low");
    }

    /// `new_session`'s account must belong to the session's agent, and its model must be one the
    /// agent offers; both are refused before anything is launched.
    #[tokio::test]
    async fn new_session_refuses_another_agents_account_and_a_model_not_offered() {
        let (state, _dir) = settings_state("new-session-overrides");
        for (extra, expected) in [
            (json!({ "account": "acct-codex" }), "unknown account"),
            (json!({ "model": "gpt-9" }), "is not a model"),
        ] {
            let mut args = json!({ "project": "project-1", "brief": { "goal": "do it" } });
            args.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            let message = refused(&state, "a", "new_session", args).await;
            assert!(message.contains(expected), "{message}");
        }
        assert_eq!(state.store.list_sessions().unwrap().len(), 8);
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
