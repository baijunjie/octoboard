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

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use crate::branch_origin::cleanup;
use crate::branch_origin::handover::{
    branch_name, branch_noun_and_pronoun, move_branches, named_branches_with_targets, Handed,
    Handover,
};
use crate::branch_origin::session_branches;
use crate::coordinator::{self, LaunchEdit, OpenRequest};
use crate::git_remote;
use crate::protocol::{
    error_code, Agent, CodedError, Console, Origin, Project, ProjectSource, Role, Session,
    SessionStatus,
};
use crate::reporting::{self, Delivery, ReportStatus, WhenBlocked};
use crate::session_request;
use crate::sharing::{self, OwnerCopy};
use crate::state::{AppState, WriteHold};
use crate::term::{Launch, LaunchRequest};

use super::account_switch;
use super::args::{
    find_console, find_project, optional_agent, optional_bool, optional_string,
    optional_string_list, own_console, own_project, present_text, raw_text, required_str,
    resolve_project, string_list,
};
use super::bindings;
use super::handoff;
use super::handover::{self, branch_names, held_by};
use super::history;
use super::models;
use super::pages;
use super::project_branches;
use super::project_changes;
use super::quotas;
use super::reads;
use super::session_branches as session_branches_tool;
use super::session_model;
use super::settings::{
    self, describe_project, git_workflow_argument, launch_argument, project_accounts_argument,
    resolve_account,
};

/// Runs one tool call. The `Err` case is what the model is shown as the reason the call failed, so
/// the messages are written for it rather than for a log.
pub async fn call(
    state: &Arc<AppState>,
    session_id: &str,
    tool: &str,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let session = state.session_record(session_id)?;
    if super::tool_by_name(session.role, session.is_worker(), tool).is_none() {
        bail!("`{tool}` is not a tool this session can call");
    }

    match tool {
        "list_projects" => reads::list_projects(state, &session, arguments).await,
        "list_consoles" => reads::list_consoles(state, &session),
        "add_project" => add_project(state, &session, arguments).await,
        "update_project" => settings::update_project(state, &session, arguments).await,
        "get_console" => settings::get_console(state, &session),
        "update_console" => settings::update_console(state, &session, arguments).await,
        "get_settings" => settings::get_settings(state),
        "update_settings" => settings::update_settings(state, arguments),
        "new_session" => new_session(state, &session, arguments, crate::term::launch).await,
        "instruct_session" => instruct_session(state, &session, arguments).await,
        "interrupt_session" => interrupt_session(state, &session, arguments).await,
        "get_session" => reads::get_session(state, &session, arguments).await,
        "rename_session" => rename_session(state, &session, arguments).await,
        "set_session_pinned" => set_session_pinned(state, &session, arguments).await,
        "archive_session" => archive_session(state, &session, arguments).await,
        "archive_self" => archive_self(state, &session).await,
        "list_archived" => reads::list_archived(state, &session, arguments).await,
        "reopen_session" => {
            reopen_session(
                state,
                &session,
                arguments,
                crate::env_shell::launch_snapshot,
                crate::term::launch,
            )
            .await
        }
        "handover_branches" => handover::handover_branches(state, &session, arguments).await,
        "handoff_session" => {
            handoff::handoff_session(state, &session, arguments, crate::term::launch).await
        }
        "switch_session_account" => {
            account_switch::switch_session_account(
                state,
                &session,
                arguments,
                crate::env_shell::launch_snapshot,
                crate::term::launch,
            )
            .await
        }
        "set_session_model" => {
            session_model::set_session_model(
                state,
                &session,
                arguments,
                crate::env_shell::launch_snapshot,
                crate::term::launch,
            )
            .await
        }
        "show_page" => pages::show_page(state, &session, arguments),
        "list_pages" => pages::list_pages(state, &session, arguments),
        "delete_pages" => pages::delete_pages(state, &session, arguments),
        "report" => report(state, &session, arguments).await,
        "list_sessions" => reads::list_sessions(state, &session, arguments).await,
        "get_session_branches" => {
            session_branches_tool::get_session_branches(state, &session, arguments).await
        }
        "list_project_changes" => {
            project_changes::list_project_changes(state, &session, arguments).await
        }
        "list_project_branches" => {
            project_branches::list_project_branches(state, &session, arguments).await
        }
        "read_project_diff" => project_changes::read_project_diff(state, &session, arguments).await,
        "list_agent_models" => models::list_agent_models(state, arguments).await,
        "read_session_history" => history::read_session_history(state, &session, arguments).await,
        "get_project" => settings::get_project(state, &session, arguments).await,
        "get_account_quotas" => quotas::get_account_quotas(state, arguments).await,
        "send_message" => send_message(state, &session, arguments).await,
        "transfer_session" => bindings::transfer_session(state, &session, arguments).await,
        "release_session" => bindings::release_session(state, &session, arguments).await,
        "bind_to_session" => bindings::bind_to_session(state, &session, arguments).await,
        "request_session" => request_session(state, &session, arguments).await,
        "request_resume_session" => {
            super::resume_request::request_resume_session(state, &session, arguments).await
        }
        // Unreachable while the catalogue and this dispatch agree; a tool added to one and not the
        // other should say so rather than look like a refusal.
        _ => bail!("`{tool}` is announced but not implemented"),
    }
}

/// The names of the branches each of `sessions` is responsible for, read on the blocking pool
/// (the store and the reflog files, and no process).
pub(super) async fn branch_names_of(
    state: &Arc<AppState>,
    sessions: Vec<Session>,
) -> Result<HashMap<String, Vec<String>>> {
    let state = state.clone();
    tokio::task::spawn_blocking(move || session_branches::names_by_session(&state, &sessions))
        .await?
}

// -- orchestration tools -----------------------------------------------------

async fn add_project(
    state: &Arc<AppState>,
    console_session: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let source = match required_str(arguments, "source")? {
        "local" => ProjectSource::Local,
        "parent" => ProjectSource::Parent,
        "git" => ProjectSource::Git,
        other => bail!("`{other}` is not one of `local`, `parent` or `git`"),
    };
    let remote_url = optional_string(arguments, "remote_url");
    if source == ProjectSource::Git {
        check_clone_url(remote_url.as_deref())?;
    }
    let mut settings = coordinator::NewProjectSettings {
        pinned: optional_bool(arguments, "pinned")?.unwrap_or(false),
        git_workflow: git_workflow_argument(arguments)?,
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
            check_text_limits: true,
            tags: optional_string_list(arguments, "tags")?,
            description: present_text(arguments, "description")?.flatten(),
        },
    )
    .await?;
    let projects = added
        .iter()
        .map(|project| describe_project(state, project))
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({ "projects": projects }))
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

/// Starts the session through `launch`: `term::launch` outside the tests, which cannot start an
/// agent.
async fn new_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
    launch: impl FnOnce(crate::term::LaunchRequest) -> Result<crate::term::Launch> + Send + 'static,
) -> Result<Value> {
    let (project, open) = session_to_start(state, caller, arguments)?;
    if caller.role == Role::Console
        && optional_string_list(arguments, "branches")?.is_some_and(|names| !names.is_empty())
    {
        bail!(
            "`branches` is for a project session; a console session is responsible for no branch, \
             so it has none to hand the new session"
        );
    }
    // Checked before anything is started, as `request_session` checks before it asks.
    let handover = branches_to_hand_over(state, caller, &project, arguments).await?;
    // Written before the move, as `request_session`'s is: the session reads it as it starts.
    let task = match (&handover, open.task) {
        (Some(handover), Some(brief)) => Some(branches_opening_prompt(caller, handover, &brief)),
        (_, task) => task,
    };
    let session = coordinator::open_session_with(
        state,
        OpenRequest {
            task,
            // A session a session starts always reports to it.
            bound_to: Some(caller.id.clone()),
            ..open
        },
        launch,
    )
    .await?;
    // The account is settled by the opening, not by the call alone: the weekly pick, the caller's
    // own, or what the project or console sets, so the caller is told which one it got.
    let (model, effort) = coordinator::launch_settings(state, &session)?;
    let mut answer = json!({
        "session": session.id,
        "agent": session.agent,
        "account": session.account_id.as_deref().unwrap_or("default"),
        "account_name": state
            .store
            .account_display_name(session.agent, session.account_id.as_deref())?,
        "model": model,
        "effort": effort,
    });
    if let Some(handover) = handover {
        let handed = move_branches(state, &handover, caller, &session).await;
        answer["branches"] = json!(match &handed {
            Handed::Moved(moved) => moved.moved.iter().map(|b| branch_name(b)).collect(),
            Handed::Failed(_) => Vec::new(),
        });
        answer["note"] = json!(session_request::branch_outcome(&handed).trim());
    }
    Ok(answer)
}

/// How [`branches_opening_prompt`] begins.
pub(crate) const BRANCHES_OPENING: &str = "The session `";

/// What follows the session's title in [`branches_opening_prompt`], which tells it from a user's
/// message that happens to begin like it.
pub(crate) const BRANCHES_MARKER: &str = "\"), which started you, also handed the ";

/// The opening prompt of a session `new_session` hands branches to: the brief, and that the
/// session starting it handed it the branches and it is now the one responsible for them.
pub(crate) fn branches_opening_prompt(
    caller: &Session,
    handover: &Handover,
    brief: &str,
) -> String {
    let (noun, them) = branch_noun_and_pronoun(&handover.branches);
    format!(
        "{BRANCHES_OPENING}{id}` (\"{title}{BRANCHES_MARKER}{noun} {} over to \
         you: you are now the session responsible for {them}, so carry on the work on {them}. \
         This is the brief it wrote:\n\n{brief}",
        named_branches_with_targets(&handover.branches, &handover.targets),
        id = caller.id,
        title = sharing::one_line(&caller.title),
    )
}

/// Reads what `new_session` asks for into the project it starts in — the one the console session
/// names, or a project session's own — and the session to open there, unbound.
fn session_to_start(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<(Project, OpenRequest)> {
    let project = match caller.role {
        Role::Console => resolve_project(
            state,
            &caller.console_id,
            required_str(arguments, "project")?,
        )?,
        Role::Project => own_project(state, caller, optional_string(arguments, "project"))?,
    };
    let open = session_in(
        state,
        caller,
        &own_console(state, caller)?,
        Some(project.clone()),
        arguments,
    )?;
    Ok((project, open))
}

/// Reads a brief, the title and the overrides — `new_session`'s arguments, and `request_session`'s
/// alike — into the session `caller` asks to open, unbound, in `project` of `console`, or a console
/// session of `console` for none.
fn session_in(
    state: &Arc<AppState>,
    caller: &Session,
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

    // Only in the caller's own console does a session follow the caller's agent and account:
    // another console is set up apart, and its sessions run on what it and its projects set. A
    // console session does not follow its own agent into a project either: the project's default
    // agent is what its sessions run on, unless the call names one.
    let in_own_console = caller.console_id == console.id;
    let follows_callers_agent =
        in_own_console && !(caller.role == Role::Console && project.is_some());
    let chosen_agent = optional_agent(arguments, "agent")?;
    let mut agent = match chosen_agent {
        Some(agent) => Some(agent),
        None if follows_callers_agent
            && (state.store.auto_select_account()?
                || coordinator::agent_can_open(state, caller.agent)?) =>
        {
            Some(caller.agent)
        }
        None => None,
    };
    // An account is one agent's, so it is read for the agent the session will be of, as
    // `coordinator::session_agent` settles it.
    let session_agent = coordinator::session_agent(agent, project.as_ref(), console);
    let mut account = match optional_string(arguments, "account") {
        Some(wanted) => Some(resolve_account(state, session_agent, &wanted)?),
        None if state.store.auto_select_account()? => None,
        None => account_when_unset(state, caller, session_agent, in_own_console)?,
    };
    if optional_string(arguments, "account").is_none() && state.store.auto_select_account()? {
        let inherited = if in_own_console {
            callers_account(state, caller, caller.agent)?.map(|account| (caller.agent, account))
        } else {
            None
        };
        if let Some((picked_agent, picked_account)) = coordinator::automatic_open_choice(
            state,
            session_agent,
            console,
            project.as_ref(),
            inherited,
            chosen_agent.is_none(),
        )? {
            agent = Some(picked_agent);
            account = Some(picked_account);
        }
    }
    Ok(OpenRequest {
        console_id: console.id.clone(),
        project_id: project.map(|project| project.id),
        agent,
        // Without one, the session takes the project's account for its agent, else the
        // console's; a blank model or effort is the agent's own default.
        account,
        model: present_text(arguments, "model")?,
        effort: present_text(arguments, "effort")?,
        task: Some(task),
        // Named for the task, not the project, when the caller gives no name: several sessions
        // dispatched into one project would otherwise all carry the project's name and be
        // indistinguishable in the menu.
        title: Some(
            optional_string(arguments, "title").unwrap_or_else(|| reporting::title_from_goal(goal)),
        ),
        origin: Origin::Console,
        bound_to: None,
        resume_agent_session_id: None,
        session_id: None,
        terminal_appearance: None,
    })
}

/// The account of a session of `agent` that `caller` starts, or hands one off to, when the call
/// names none: with automatic account selection on, the one whose weekly quota resets soonest;
/// otherwise, or when none ranks, `caller`'s own (`callers_account`) in its own console, else the
/// project's account for the agent, else the console's.
pub(super) fn account_when_unset(
    state: &Arc<AppState>,
    caller: &Session,
    agent: Agent,
    in_own_console: bool,
) -> Result<Option<Option<String>>> {
    if state.store.auto_select_account()? {
        if let Some(picked) = coordinator::preferred_weekly_account(state, agent)? {
            // `Some(None)` is an explicit choice of the default account.
            return Ok(Some(picked));
        }
    }
    if in_own_console {
        callers_account(state, caller, agent)
    } else {
        // Inherit the project, else the console.
        Ok(None)
    }
}

/// The account a session of `agent` that `caller` starts runs on when the call names none:
/// `caller`'s own, for a session of `caller`'s own agent, so that work a session hands out goes on
/// in the same agent and account unless the user asked otherwise. `None` — the project's account
/// for the agent, else the console's — for another agent, and when `caller`'s account could not
/// open a session now: removed since, turned off, logged out, or with its quota used up.
pub(super) fn callers_account(
    state: &Arc<AppState>,
    caller: &Session,
    agent: Agent,
) -> Result<Option<Option<String>>> {
    if agent != caller.agent
        || !coordinator::account_can_open(state, agent, caller.account_id.as_deref())?
    {
        return Ok(None);
    }
    Ok(Some(caller.account_id.clone()))
}

async fn instruct_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let target = resolve_owned_session(state, caller, required_str(arguments, "session")?)?;
    let text = required_str(arguments, "text")?.to_string();
    let delivery =
        reporting::write_message_off_runtime(state, &target.id, text, WhenBlocked::Queue)
            .await
            .map_err(|err| point_to_reopen(err, &target))?;
    Ok(json!({
        "delivered": delivery == Delivery::Written,
        "note": delivery_note(delivery),
    }))
}

/// A refusal for a session with no process, said with what the caller can do about it: the
/// session is the caller's, so `reopen_session` relaunches it with the message. The code and the
/// params stay the refusal's own.
fn point_to_reopen(err: anyhow::Error, target: &Session) -> anyhow::Error {
    match err.downcast_ref::<CodedError>() {
        Some(coded)
            if coded.code == error_code::SESSION_NOT_RUNNING && target.status.is_dormant() =>
        {
            anyhow::Error::new(CodedError {
                code: coded.code,
                message: format!(
                    "{}: it is {}. `reopen_session` relaunches it, and its `text` delivers this \
                     message once it is running",
                    coded.message,
                    if target.status == SessionStatus::Archived {
                        "archived"
                    } else {
                        "interrupted"
                    },
                ),
                params: coded.params.clone(),
            })
        }
        _ => err,
    }
}

/// Stops the turn a session of the caller's is running, through the same function the control
/// request runs.
async fn interrupt_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let target = resolve_owned_session(state, caller, required_str(arguments, "session")?)?;
    let crate::interrupt::Interrupted {
        interrupted,
        status,
        note,
    } = crate::interrupt::interrupt_session(state, &target.id).await?;
    Ok(json!({ "interrupted": interrupted, "status": status, "note": note }))
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
        Delivery::Unsent => {
            "Not sent: the message was written into this session's input, but its agent did not \
             send it to the conversation (it is in the input box, or went to a view of its \
             background agents), so the session has not read it. The user has been told to check \
             the session; do not send it again."
        }
        Delivery::Kept => {
            "Queued: this session is interrupted, with no process running. It is kept, and \
             delivered once the session is resumed, so do not send it again."
        }
    }
}

/// Archives a session of the caller's, and once its process has ended removes what can go of the
/// branches and worktrees bound to it, never forcing anything, unless `keep`; what stays is named
/// in the answer, so the caller can deal with it.
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
    let keep = optional_bool(arguments, "keep")?.unwrap_or(false);
    let target = resolve_owned_session(state, caller, wanted)?;
    let archived = coordinator::archive_session(state, &target.id)?;
    let mut answer = json!({ "session": target.id, "status": SessionStatus::Archived });
    if keep {
        // Nothing is removed, so nothing is waited for: the records stay bound to the archived
        // sessions, which `get_session_branches` lists and `handover_branches` can move.
        answer["branches_kept"] = json!(true);
        let kept = branch_names_of(state, archived.clone()).await?;
        answer["note"] = json!(keep_note(&target, &archived, &kept));
        return Ok(answer);
    }
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

/// What the caller is told of the branches `keep` left bound to the sessions an archive ended:
/// `kept` is the names by session id. An archive of a lead session takes the sessions under it too,
/// each of which keeps its own branches, and those are not the caller's to name as `from` to
/// `handover_branches`, which takes the caller itself or a session bound to it: the archived
/// session, once reopened, hands them to itself first.
fn keep_note(
    target: &Session,
    archived: &[Session],
    kept: &HashMap<String, Vec<String>>,
) -> String {
    let mut note = String::from(
        "Archived. Its git branches and worktrees were left as they are, as asked. They stay \
         bound to the sessions that are responsible for them (`get_session_branches` lists \
         them) and come back with those sessions when they are reopened; a session under a lead \
         session is reopened by its lead.",
    );
    let holders = |under: bool| {
        archived
            .iter()
            .filter(move |session| (session.id != target.id) == under)
            .filter(|session| kept.get(&session.id).is_some_and(|names| !names.is_empty()))
    };
    if holders(false).next().is_some() {
        note.push_str(&format!(
            " To move the branches of `{id}` itself to a session of yours now, use \
             `handover_branches` with `from` `{id}`.",
            id = target.id
        ));
    }
    let under: Vec<&Session> = holders(true).collect();
    if !under.is_empty() {
        let kept_by = under
            .iter()
            .map(|session| format!("`{}` ({})", session.id, kept[&session.id].join(", ")))
            .collect::<Vec<_>>()
            .join("; ");
        let ids = under
            .iter()
            .map(|session| format!("`{}`", session.id))
            .collect::<Vec<_>>()
            .join(", ");
        note.push_str(&format!(
            " The sessions archived with it keep theirs: {kept_by}. They are not yours to name \
             as `from`. To move them now, `reopen_session` `{id}` with a `text` asking it to \
             `handover_branches` them from each of {ids} to itself, then hand them on with \
             `handover_branches` `from` `{id}` once it is idle.",
            id = target.id
        ));
    }
    note
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

async fn rename_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let target = resolve_owned_session(state, caller, required_str(arguments, "session")?)?;
    let title = raw_text(arguments, "title")?.ok_or_else(|| anyhow!("`title` is required"))?;
    let renamed = coordinator::rename_session(state, &target.id, &title)?;
    let branches = branch_names_of(state, vec![renamed.clone()]).await?;
    describe_session(
        state,
        &renamed,
        caller,
        &branches,
        &owned_count_of(state, &renamed)?,
    )
}

async fn set_session_pinned(
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
    let branches = branch_names_of(state, vec![updated.clone()]).await?;
    describe_session(
        state,
        &updated,
        caller,
        &branches,
        &owned_count_of(state, &updated)?,
    )
}

/// Reopens through `launch` in the shell environment `snapshot` gives: `term::launch` and
/// `env_shell::launch_snapshot` outside the tests, which cannot start an agent.
async fn reopen_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
    snapshot: impl FnOnce() -> Result<HashMap<String, String>> + Send + 'static,
    launch: impl Fn(LaunchRequest) -> Result<Launch> + Clone + Send + 'static,
) -> Result<Value> {
    let target = resolve_owned_session(state, caller, required_str(arguments, "session")?)?;
    // The instruction travels with the relaunch rather than being written after it. A Claude Code
    // resume takes it as the launch's prompt; for the other agents it is queued inside the
    // launch, because whichever hook would release it can fire the moment the agent starts, and a
    // refusal there also undoes it.
    coordinator::resume_session_edited_with(
        state,
        &target.id,
        arguments.get("text").and_then(Value::as_str),
        // An owner asks over MCP, which has no window appearance to match.
        None,
        // Checked against what the agent offers before anything is reopened.
        LaunchEdit {
            model: present_text(arguments, "model")?,
            effort: present_text(arguments, "effort")?,
        },
        snapshot,
        launch,
    )
    .await?;
    // What the relaunch used, as the launch reads it: the session's own choice, else its project's
    // (or console's) setting. `null` is the agent's own default.
    let reopened = state.session_record(&target.id)?;
    let (model, effort) = coordinator::launch_settings(state, &reopened)?;
    Ok(json!({
        "session": target.id,
        "account": reopened.account_id.as_deref().unwrap_or("default"),
        "account_name": state
            .store
            .account_display_name(reopened.agent, reopened.account_id.as_deref())?,
        "model": model,
        "effort": effort,
    }))
}

// -- information exchange ----------------------------------------------------

async fn send_message(
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
        OwnerCopy::Sent(Delivery::Kept) => {
            note.push_str(" Its owner's copy is kept, and will arrive once the owner is resumed.")
        }
        OwnerCopy::Sent(Delivery::Unsent) => note.push_str(
            " Its owner's copy was written into the owner's input but not sent to its \
             conversation; the user has been told.",
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

    let (note, _) =
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
    let open = session_in(state, caller, &console, project, arguments)?;
    // Read once: the opening prompt says how the request was approved, and the call acts on it.
    let automatic = state.store.auto_approve_session_requests()?;
    let task = open
        .task
        .map(|brief| request_opening_prompt(caller, handover.as_ref(), automatic, &brief));
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
        automatic,
        session_request::ANSWER_TIME_LIMIT,
    )
    .await
}

/// How [`request_opening_prompt`] begins.
pub(crate) const REQUEST_OPENING: &str = "You were started at the request of the session ";

/// The opening prompt of a session started at `caller`'s request: whoever reads the brief is to
/// know who wrote it, how the request was approved, and that it is not that session's to drive.
pub(crate) fn request_opening_prompt(
    caller: &Session,
    handover: Option<&Handover>,
    automatic: bool,
    brief: &str,
) -> String {
    let approval = if automatic {
        "automatically, under the user's setting to approve session requests without asking"
    } else {
        "with the user's approval"
    };
    let handed = handover.map_or_else(String::new, |handover| {
        let (noun, them) = branch_noun_and_pronoun(&handover.branches);
        format!(
            " It also handed the {noun} {} over to you, with the same approval: you are now the \
             session responsible for {them}, so carry on the work on {them}.",
            named_branches_with_targets(&handover.branches, &handover.targets)
        )
    });
    format!(
        "{REQUEST_OPENING}`{id}` (\"{title}\"), {approval}. Nobody \
         owns you but the user: that session is not your owner, and it and you can only share \
         information with each other.{handed} This is the brief it wrote:\n\n{brief}",
        id = caller.id,
        title = sharing::one_line(&caller.title),
    )
}

/// The branches `request_session` and `new_session` hand over, from their `branches` argument:
/// local branches of the repository `project` is in that the caller is responsible for, each
/// named once. `None` when the argument names none. Refused before anything is started or asked,
/// with the first branch that is not the caller's to give.
async fn branches_to_hand_over(
    state: &Arc<AppState>,
    caller: &Session,
    project: &Project,
    arguments: &Map<String, Value>,
) -> Result<Option<Handover>> {
    let Some(branches) = branch_names(arguments, "branches")? else {
        return Ok(None);
    };
    held_by(state, caller, caller, project, branches)
        .await
        .map(Some)
}

// -- argument and record helpers ---------------------------------------------

/// Finds a session the caller may read: any session on record, of any console or project. What a
/// session may *do* to another is narrower, and is [`resolve_owned_session`]'s.
pub(super) fn resolve_session(state: &Arc<AppState>, wanted: &str) -> Result<Session> {
    state
        .store
        .get_session(wanted)?
        .ok_or_else(|| anyhow!("there is no session `{wanted}`"))
}

/// The session a read tool's optional `session` argument names, the caller itself when it is left
/// out (or names the caller), read afresh from the store. Reading, like [`resolve_session`].
pub(super) fn session_or_caller(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Session> {
    match optional_string(arguments, "session") {
        Some(wanted) if wanted != caller.id => resolve_session(state, &wanted),
        _ => state.session_record(&caller.id),
    }
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
    // `send_message` refuses an archived session, so it is not offered for one.
    let send_message = super::qualified_tool_name(caller.agent, "send_message");
    let archived = session.status == SessionStatus::Archived;
    let tell_it = |tail: &str| {
        if archived {
            String::new()
        } else {
            format!(
                " You can still tell it something with `{send_message}`, which it weighs rather \
                 than obeys{tail}"
            )
        }
    };
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
             yours.{}",
            tell_it("")
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
                "session `{wanted}` is bound to {kind}, `{owner}`{title}. Only that session \
                 instructs or drives it, so this is not yours to do.{}",
                tell_it(", and its owner is sent a copy")
            )
        }
        None => bail!(
            "session `{wanted}` is not bound to any session: the user opened it themselves and \
             kept it outside the orchestration. It is theirs, not yours to drive.{}",
            tell_it("")
        ),
    }
}

/// How many sessions are bound to `session`, for it to be described alone with (see
/// [`describe_session`]): counted in the store, not by reading the whole table.
pub(super) fn owned_count_of(
    state: &Arc<AppState>,
    session: &Session,
) -> Result<HashMap<String, usize>> {
    coordinator::owned_count_of(state, session)
}

/// How many sessions are bound to each session, by id, for the sessions of a listing to be
/// described with (see [`describe_session`]). Read once for the listing: counting per session is
/// a scan of the whole table each time.
pub(super) fn owned_counts_of(state: &Arc<AppState>) -> Result<HashMap<String, usize>> {
    Ok(coordinator::owned_counts(&state.store.list_sessions()?))
}

/// `caller` is the session asking. Everything here is the same whoever asks except `yours`, which
/// is relative to `caller`: a console may hold several console sessions, and each reads a
/// session's owner alike but is told only of its own that they are its to act on.
///
/// `branches` is the names of the branches the sessions described are responsible for, by session
/// id, as [`branch_names_of`] reads them for all the sessions of a listing at once; a session with
/// no entry is responsible for none. `owned` is how many sessions are bound to each, as
/// [`owned_counts_of`] reads them; likewise no entry is none.
///
/// `pub(crate)` for the tests of `shared_config`, which describe a session being restarted.
pub(crate) fn describe_session(
    state: &Arc<AppState>,
    session: &Session,
    caller: &Session,
    branches: &HashMap<String, Vec<String>>,
    owned: &HashMap<String, usize>,
) -> Result<Value> {
    let owner_kind = match &session.bound_to {
        Some(owner) => state
            .store
            .get_session(owner)?
            .map(|owner| owner.role.kind()),
        None => None,
    };
    // Looked at before the status is read, so that the status, which reads the screen as that look
    // left it, and the hold cannot disagree about a dialog that came up in between.
    let write_hold = state.write_hold(&session.id);
    // The account by id and name, as `get_account_quotas` and `switch_session_account` name it:
    // `default` is the agent's default account. Never its directory, which is the user's.
    let account_name = state
        .store
        .account_display_name(session.agent, session.account_id.as_deref())?;
    let mut description = json!({
        "session": session.id,
        "title": session.title,
        // What the session list shows for it: the agent's current task before the title, as the
        // user reads it, so that "the session called X" can be matched to a record.
        "display_title": state.display_title(session),
        "agent": session.agent,
        // The agent's own id of the conversation, `request_resume_session`'s `conversation`.
        // Null until the agent has reported one.
        "agent_session_id": session.agent_session_id,
        "origin": session.origin,
        "status": state.shown_status(session),
        // What an idle session has left running in the background: it takes a message like any
        // idle session, but its work is not over. Empty for every other status.
        "background_tasks": state.shown_background_tasks(session),
        // How full the session's context window is, as the UI shows it: owners weigh it when
        // choosing which session gets a task. Null while no size is known: before the session's
        // first report, and after a compaction until its next request.
        "context": session.context,
        "account": session.account_id.as_deref().unwrap_or("default"),
        "account_name": account_name,
        // What the agent's records show the session running with, as the agent spells it. Null
        // until reported; `default_effort` is what the agent runs that model at by itself, given
        // where the records name the model but no effort and its catalogue knows one.
        "observed_model": session.observed_model,
        "observed_effort": session.observed_effort,
        "default_effort": state.default_effort_of(&session.id),
        // Whether the agent has a conversation of this session to continue. It says a turn was
        // taken, not that the agent still holds the record: a `reopen_session` that finds the
        // record gone is refused as `conversation_unresumable`.
        "has_conversation": session.has_conversation,
        "pinned": session.pinned,
        // A locked session cannot be archived.
        "locked": session.locked,
        // The names only: the worktree each stands in is `get_session_branches`'.
        "branches": branches.get(&session.id).cloned().unwrap_or_default(),
        "role": session.role,
        "orchestration_role": session.orchestration_role(),
        "console": session.console_id,
        "project": session.project_id,
        // The session this one reports to. Null for a project session means unbound: the user
        // opened it themselves and kept it outside the orchestration. It is also null for a
        // console session, which is never bound, so `role` is what tells the two apart. Only a
        // session whose owner is the caller is the caller's to act on.
        "owner": session.bound_to,
        // Whether that owner is a console session or a project session; null with no owner.
        "owner_kind": owner_kind,
        // A lead session: bound to a console session while keeping sessions of its own.
        "lead": session.lead,
        // The sessions bound to this one in any status, archived ones included, as the rules on
        // who can own a session and on transferring one count them.
        "owned_sessions": owned.get(&session.id).copied().unwrap_or(0),
        "yours": session.bound_to.as_deref() == Some(caller.id.as_str()),
        "started_at": session.started_at,
        "ended_at": session.ended_at,
        // Messages sent to this session that are waiting to be written into it, including what is
        // kept for an interrupted session to take at its resume.
        "queued_messages": state.queued_count(&session.id)
            + state.kept_unqueued_count(&session.id)?,
        // What keeps them waiting while `status` alone would say the session can take one, so an
        // idle session with messages queued is not left unexplained. Null when nothing holds it.
        "writes_held": write_hold.map(|hold| {
            let mut held = json!({
                "reason": hold.code(),
                "explanation": write_hold_explanation(hold),
            });
            if let Some(dialog) = hold.dialog() {
                held["dialog"] = json!(dialog);
            }
            held
        }),
    });
    // The session's own launch overrides, kept apart from the observed ones: absent follows its
    // project's (else console's) current setting, `null` leaves the choice to the agent, a value
    // pins it.
    if let Some(model) = &session.model {
        description["model"] = json!(model);
    }
    if let Some(effort) = &session.effort {
        description["effort"] = json!(effort);
    }
    Ok(description)
}

/// What an owner is told about a hold on a session's writes, and what, if anything, it can do.
fn write_hold_explanation(hold: WriteHold) -> &'static str {
    match hold {
        WriteHold::Starting => {
            "Its agent has not yet signalled that it is past its startup screens, so nothing is \
             written into it in case one is still up. Messages for it are queued and go in once \
             that signal arrives; if this lasts, the user has to look at the session."
        }
        WriteHold::TrustConfirmation => {
            "Its agent is asking whether to trust its folder. Messages for it are queued until \
             that is answered."
        }
        WriteHold::AgentDialog(_) => {
            "Its agent is showing a dialog that only the user can answer, and a message written \
             into it would answer the dialog instead. Messages for it are queued until the dialog \
             is gone."
        }
        WriteHold::AgentAction => {
            "Its agent is waiting on the user. Messages for it are queued until the user has \
             answered."
        }
        WriteHold::AgentError => {
            "Its agent has just stopped on an error. Messages for it are queued for a moment, \
             until it is clear whether the agent is showing a prompt only the user can answer."
        }
        WriteHold::Handoff => {
            "It is being handed off to another agent. Messages for it are queued and go to the \
             session that takes its place."
        }
        WriteHold::SharingChange => {
            "It is being restarted for a change of its account's sharing of the agent's default \
             configuration, and is resumed on its conversation afterwards. Messages for it are \
             queued and written into the resumed process."
        }
        WriteHold::UnsentMessage => {
            "A message written into it was not sent by its agent and is waiting in its input box; \
             the user has been told. Nothing more is written into it until a prompt is sent \
             there, and messages for it are queued until then. Do not send it again."
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::protocol::{
        now_millis, Account, AccountQuota, Agent, AgentAvailability, Availability, Console, Event,
        Project, ProjectAccount, ProjectSource, QuotaWindow, QuotaWindowKind,
    };
    use crate::store::LOCAL_HOST_ID;
    use crate::test_support::{output_containing, running, ScratchDir};
    use uuid::Uuid;

    fn console() -> Console {
        Console {
            id: "console-1".to_string(),
            name: "Console".to_string(),
            workdir: "/tmp/console-1".to_string(),
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
            background_tasks: Vec::new(),
            switching_account: false,
            default_effort: None,
            orchestration_role: None,
            context: None,
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
                description: None,
                git_workflow: Default::default(),
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

    /// An agent that is not added is refused to an agent's own `new_session` and `request_session`
    /// alike, the latter before the user is asked.
    #[tokio::test]
    async fn an_agent_that_is_not_added_is_refused_to_new_session_and_request_session() {
        let (state, _dir) = shared_console_state("new-session-agent-not-added");
        state
            .store
            .set_added_agents_if_undecided(&[Agent::Codex])
            .unwrap();
        let brief = json!({ "brief": { "goal": "do the thing" }, "agent": "claude" });
        let mut new = arguments(brief.clone());
        new.insert("project".to_string(), json!("project-1"));
        let mut request = arguments(brief);
        request.insert("kind".to_string(), json!("project"));
        request.insert("project".to_string(), json!("project-1"));

        for (tool, args) in [("new_session", new), ("request_session", request)] {
            let err = call(&state, "a", tool, &args).await.expect_err("refused");
            assert!(err.to_string().contains("is not added"), "{tool}: {err}");
            assert!(err.to_string().contains("Agent accounts"), "{tool}: {err}");
            assert!(!state.session_requests.is_waiting_for("a"));
        }
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

    /// `new_session`'s `branches` are the caller's to give as `request_session`'s are: anything
    /// else is refused before a session is started, and what is accepted moves to the new session
    /// once it is running, which the result says.
    #[tokio::test]
    async fn new_session_hands_the_callers_branches_to_the_session_it_starts() {
        use crate::test_support::{stand_in_launch, Launched, StandIn};
        use std::sync::Mutex;

        let (state, _dir) = shared_console_state("new-session-branches");
        let repo = crate::test_support::repo_with("new-session-branches-repo", &[(b"a", b"a")]);
        state
            .store
            .insert_project(&Project {
                id: "repo".to_string(),
                name: "repo".to_string(),
                path: repo.to_string_lossy().into_owned(),
                ..state.store.get_project("project-1").unwrap().unwrap()
            })
            .unwrap();
        state
            .store
            .insert_session(&Session {
                project_id: Some("repo".to_string()),
                ..project_session("dev", "console-1", None)
            })
            .unwrap();
        crate::test_support::branch_made_from(&state, &repo, "mine", "dev", Some("main"));
        crate::test_support::branch_made_by(&state, &repo, "also", "dev");
        crate::test_support::branch_made_by(&state, &repo, "theirs", "a");
        let _caller = running(&state, "dev");
        let caller = state.session_record("dev").unwrap();
        let responsible = |branch: &str| {
            let key = crate::branch_origin::repo_key(
                &crate::branch_origin::repository_of(&repo).unwrap(),
            );
            state
                .store
                .branch_origins(&key)
                .unwrap()
                .into_iter()
                .find(|row| row.branch == branch)
                .unwrap()
                .responsible_session
        };
        let launches = Arc::new(Mutex::new(Vec::<Launched>::new()));
        let started = Arc::new(Mutex::new(Vec::<StandIn>::new()));
        let before = state.store.list_sessions().unwrap().len();

        let input = arguments(json!({ "brief": { "goal": "x" }, "branches": ["theirs"] }));
        let err = new_session(
            &state,
            &caller,
            &input,
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .expect_err("refused");
        assert!(
            err.to_string()
                .contains("`theirs` is not a branch you are responsible for"),
            "{err}"
        );
        assert!(launches.lock().unwrap().is_empty());
        assert_eq!(state.store.list_sessions().unwrap().len(), before);
        assert_eq!(responsible("mine"), "dev");

        let input = arguments(json!({
            "brief": { "goal": "x" },
            "branches": ["mine", "also", "mine"],
        }));
        let answer = new_session(
            &state,
            &caller,
            &input,
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .unwrap();
        let id = answer["session"].as_str().unwrap();
        assert_eq!(answer["branches"], json!(["mine", "also"]));
        let opening = launches.lock().unwrap()[0].2.clone().unwrap();
        assert!(
            opening
                .contains("handed the branches `mine` (target branch `main`), `also` over to you")
                && opening.contains("you are now the session responsible for them"),
            "{opening}"
        );
        assert!(answer["note"]
            .as_str()
            .unwrap()
            .contains("Handed over to it"));
        assert_eq!(responsible("mine"), id);
        assert_eq!(responsible("also"), id);
        assert_eq!(responsible("theirs"), "a");
        assert_eq!(
            state
                .store
                .get_session(id)
                .unwrap()
                .unwrap()
                .bound_to
                .as_deref(),
            Some("dev")
        );

        // A console session is responsible for no branch and is told so, not that it is not
        // responsible for this one.
        let console = state.session_record("a").unwrap();
        let input = arguments(json!({
            "project": "repo",
            "brief": { "goal": "x" },
            "branches": ["mine"],
        }));
        let err = new_session(
            &state,
            &console,
            &input,
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .expect_err("refused");
        assert!(
            err.to_string()
                .contains("`branches` is for a project session"),
            "{err}"
        );

        // Without `branches` the answer is what it was.
        let input = arguments(json!({ "brief": { "goal": "x" } }));
        let answer = new_session(
            &state,
            &caller,
            &input,
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .unwrap();
        assert!(answer.get("branches").is_none());
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
        crate::test_support::branch_made_by(&state, &repo, "one", "loose");
        crate::test_support::branch_made_from(&state, &repo, "two", "loose", Some("main"));
        crate::test_support::branch_made_by(&state, &repo, "three", "loose");
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
                " It also handed the branches `two` (target branch `main`), `three` over to you, \
                 with the same approval: you are now the session responsible for them, so carry \
                 on the work on them. This is the brief it wrote:",
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

    /// The opening prompt says the user approved the request, or that Octoboard did under the
    /// user's setting.
    #[test]
    fn the_opening_prompt_words_the_approval_by_who_gave_it() {
        let caller = project_session("loose", "console-1", None);
        let asked = request_opening_prompt(&caller, None, false, "the brief");
        assert!(
            asked.contains("), with the user's approval. Nobody owns you"),
            "{asked}"
        );
        assert!(
            asked.ends_with("This is the brief it wrote:\n\nthe brief"),
            "{asked}"
        );
        let waived = request_opening_prompt(&caller, None, true, "the brief");
        assert!(
            waived.contains(
                "), automatically, under the user's setting to approve session requests without \
                 asking. Nobody owns you"
            ),
            "{waived}"
        );
        assert!(!waived.contains("with the user's approval"), "{waived}");
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
        session_request::answer_with_resume(state, &request_id, true, launch, |_| async {
            bail!("nothing is resumed here")
        })
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
    pub(in crate::mcp) async fn wait_for_request(state: &Arc<AppState>, caller: &str) -> Event {
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

    pub(in crate::mcp) fn project_session(
        id: &str,
        console_id: &str,
        bound_to: Option<&str>,
    ) -> Session {
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
    pub(in crate::mcp) fn shared_console_state(name: &str) -> (Arc<AppState>, ScratchDir) {
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
                    description: None,
                    git_workflow: Default::default(),
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

    /// `shared_console_state` plus a project, `repo`, in a real repository, and sessions in it:
    /// `kid` of the console session `a`, `lead` unbound, and `lead-kid` bound to `lead`.
    pub(in crate::mcp) struct RepoState {
        pub state: Arc<AppState>,
        pub repo: ScratchDir,
        _dir: ScratchDir,
    }

    impl RepoState {
        pub fn new(name: &str) -> Self {
            let (state, dir) = shared_console_state(name);
            let repo =
                crate::test_support::repo_with(&format!("mcp-exec-{name}-repo"), &[(b"a", b"a")]);
            state
                .store
                .insert_project(&Project {
                    id: "repo".to_string(),
                    name: "repo".to_string(),
                    path: repo.to_string_lossy().into_owned(),
                    ..state.store.get_project("project-1").unwrap().unwrap()
                })
                .unwrap();
            for (id, owner) in [
                ("kid", Some("a")),
                ("lead", None),
                ("lead-kid", Some("lead")),
            ] {
                state
                    .store
                    .insert_session(&Session {
                        project_id: Some("repo".to_string()),
                        ..project_session(id, "console-1", owner)
                    })
                    .unwrap();
            }
            Self {
                state,
                repo,
                _dir: dir,
            }
        }

        /// Makes `branch`, recorded as `session`'s.
        pub fn branch(&self, branch: &str, session: &str) {
            crate::test_support::branch_made_by(&self.state, &self.repo, branch, session);
        }
    }

    /// Stores a Claude account `claude-a`, whose directory no reader may see, and a session
    /// `rich` in `repo` with every field of the record set: on that account, pinned, locked, with
    /// an observed model and effort and a default effort, and responsible for two branches.
    fn rich_session(fixture: &RepoState) {
        let state = &fixture.state;
        state
            .store
            .insert_account(&crate::protocol::Account {
                id: "claude-a".to_string(),
                agent: Agent::Claude,
                name: "Work account".to_string(),
                config_dir: "/tmp/octoboard-secret-account-dir".to_string(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
            })
            .unwrap();
        state
            .store
            .insert_session(&Session {
                project_id: Some("repo".to_string()),
                account_id: Some("claude-a".to_string()),
                config_dir: Some("/tmp/octoboard-secret-account-dir".to_string()),
                pinned: true,
                locked: true,
                observed_model: Some("claude-opus-5-5".to_string()),
                observed_effort: Some("high".to_string()),
                ..project_session("rich", "console-1", Some("a"))
            })
            .unwrap();
        state.set_default_effort("rich", Some("medium".to_string()));
        fixture.branch("feature", "rich");
        fixture.branch("feat/nested", "rich");
    }

    /// What a record says of the session, as each kind of caller reads it: a console session and
    /// an unbound project session through `get_session`, a bound project session through
    /// `list_sessions`.
    #[tokio::test]
    async fn every_caller_reads_the_account_model_pin_lock_and_branches_of_a_session() {
        let fixture = RepoState::new("record-fields");
        rich_session(&fixture);
        let get = |caller: &'static str, session: &'static str| {
            let state = fixture.state.clone();
            async move {
                call(
                    &state,
                    caller,
                    "get_session",
                    &arguments(json!({ "session": session })),
                )
                .await
                .unwrap()
            }
        };
        let listed = call(&fixture.state, "lead-kid", "list_sessions", &Map::new())
            .await
            .unwrap();
        let in_list = |id: &str| {
            listed["sessions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry["session"] == id)
                .unwrap()
                .clone()
        };
        for (caller, record) in [
            ("a", get("a", "rich").await),
            ("lead", get("lead", "rich").await),
            ("lead-kid", in_list("rich")),
        ] {
            assert_eq!(record["account"], "claude-a", "{caller}");
            assert_eq!(record["account_name"], "Work account", "{caller}");
            assert_eq!(record["observed_model"], "claude-opus-5-5", "{caller}");
            assert_eq!(record["observed_effort"], "high", "{caller}");
            assert_eq!(record["default_effort"], "medium", "{caller}");
            assert_eq!(record["pinned"], true, "{caller}");
            assert_eq!(record["locked"], true, "{caller}");
            let mut branches: Vec<&str> = record["branches"]
                .as_array()
                .unwrap()
                .iter()
                .map(|branch| branch.as_str().unwrap())
                .collect();
            branches.sort_unstable();
            assert_eq!(branches, ["feat/nested", "feature"], "{caller}");
            assert!(
                !record.to_string().contains("octoboard-secret-account-dir"),
                "{caller}: {record}"
            );
        }

        // A session on the default account that nothing was observed of.
        let plain = in_list("kid");
        assert_eq!(plain["account"], "default");
        assert_eq!(plain["account_name"], crate::protocol::DEFAULT_ACCOUNT_NAME);
        for field in ["observed_model", "observed_effort", "default_effort"] {
            assert!(plain[field].is_null(), "{field}");
        }
        assert_eq!(plain["pinned"], false);
        assert_eq!(plain["locked"], false);
        assert_eq!(plain["branches"], json!([]));

        // A console session is responsible for no branch.
        assert_eq!(get("lead", "b").await["branches"], json!([]));
    }

    /// The default account is shown under the name the user gave it.
    #[tokio::test]
    async fn the_default_account_is_shown_under_the_name_the_user_gave_it() {
        let fixture = RepoState::new("record-default-name");
        fixture
            .state
            .store
            .set_default_account_name(Agent::Claude, Some("Personal"))
            .unwrap();
        let record = call(
            &fixture.state,
            "a",
            "get_session",
            &arguments(json!({ "session": "kid" })),
        )
        .await
        .unwrap();
        assert_eq!(record["account"], "default");
        assert_eq!(record["account_name"], "Personal");
    }

    /// Reading a record runs no process: with no `git` to run, the branches are still listed.
    #[tokio::test]
    async fn reading_a_record_runs_no_git() {
        let fixture = RepoState::new("record-no-git");
        rich_session(&fixture);
        fixture.state.host_tools.replace(crate::host_tools::Tools {
            git: || Err(anyhow!("git must not run")),
            ..crate::test_support::host_tools()
        });
        let record = call(
            &fixture.state,
            "a",
            "get_session",
            &arguments(json!({ "session": "rich" })),
        )
        .await
        .unwrap();
        assert_eq!(record["branches"].as_array().unwrap().len(), 2);
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

    /// `output_bytes` sets how much of the output is returned, from 1 to 64 KiB; outside that it is
    /// refused, not clamped.
    #[tokio::test]
    async fn the_output_tail_length_is_the_callers_within_bounds() {
        let (state, dir) = shared_console_state("tool-surface-tail-length");
        let output_dir = dir.join("output");
        std::fs::create_dir_all(&output_dir).unwrap();
        let mut kept = b"first words ".to_vec();
        kept.extend(vec![b'a'; 20 * 1024]);
        kept.extend_from_slice(b" last words");
        std::fs::write(output_dir.join("archived-of-a"), kept).unwrap();
        let read = |extra: Value| {
            let state = state.clone();
            async move {
                let mut arguments = arguments(json!({ "session": "archived-of-a" }));
                arguments.extend(extra.as_object().unwrap().clone());
                call(&state, "a", "get_session", &arguments).await
            }
        };

        let short = read(json!({ "output_bytes": 16 })).await.unwrap();
        assert_eq!(
            short["recent_output"].as_str().unwrap().trim_end(),
            "aaaaa last words"
        );
        let default = read(json!({})).await.unwrap();
        assert_eq!(
            default["recent_output"].as_str().unwrap().trim_end().len(),
            8 * 1024
        );
        let long = read(json!({ "output_bytes": 64 * 1024 })).await.unwrap();
        assert!(long["recent_output"]
            .as_str()
            .unwrap()
            .contains("first words"));
        for bad in [json!(0), json!(64 * 1024 + 1), json!(-1), json!("8")] {
            let err = read(json!({ "output_bytes": bad }))
                .await
                .expect_err("out of bounds");
            assert!(err.to_string().contains("output_bytes"), "{err}");
        }
    }

    /// `list_sessions` narrows by console, project, owner and whether the session is the
    /// caller's, every filter given having to hold; a name that matches nothing is refused.
    #[tokio::test]
    async fn list_sessions_filters_combine() {
        let (state, _dir) = peer_state("peer-list-filters");
        let ids = |caller: &'static str, filters: Value| {
            let state = state.clone();
            async move {
                let listed = call(&state, caller, "list_sessions", &arguments(filters))
                    .await
                    .unwrap();
                let mut ids: Vec<String> = listed["sessions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|session| session["session"].as_str().unwrap().to_string())
                    .collect();
                ids.sort();
                ids
            }
        };

        assert_eq!(
            ids("loose", json!({ "console": "console-2" })).await,
            ["foreign"]
        );
        assert_eq!(
            ids("loose", json!({ "project": "project-3" })).await,
            ["other-project"]
        );
        assert_eq!(ids("loose", json!({ "owner": "a" })).await, ["of-a"]);
        assert_eq!(ids("starter", json!({ "yours": true })).await, ["mine"]);
        let not_yours = ids("starter", json!({ "yours": false })).await;
        assert!(!not_yours.contains(&"mine".to_string()) && not_yours.len() > 3);
        assert_eq!(
            ids("starter", json!({ "project": "project-1", "yours": true })).await,
            ["mine"]
        );
        assert_eq!(
            ids("starter", json!({ "project": "project-3", "yours": true })).await,
            Vec::<String>::new()
        );
        assert_eq!(
            ids("a", json!({ "console": "console-1", "owner": "starter" })).await,
            ["mine"]
        );
        // A console session's own sessions, the same filter for any role.
        assert_eq!(ids("a", json!({ "yours": true })).await, ["of-a"]);

        for (filter, refusal) in [
            (
                json!({ "console": "nowhere" }),
                "there is no console `nowhere`",
            ),
            (
                json!({ "project": "nowhere" }),
                "there is no project `nowhere`",
            ),
            (
                json!({ "owner": "nowhere" }),
                "there is no session `nowhere`",
            ),
            (json!({ "yours": "yes" }), "`yours` must be true or false"),
        ] {
            let err = call(&state, "a", "list_sessions", &arguments(filter.clone()))
                .await
                .expect_err(&filter.to_string());
            assert_eq!(err.to_string(), refusal);
        }
    }

    /// `list_sessions` lists the sessions with a process by default, the interrupted ones with
    /// `status: interrupted`, and both with `any`; an archived session is in none of them.
    #[tokio::test]
    async fn list_sessions_can_select_the_interrupted_sessions() {
        let (state, _dir) = peer_state("peer-list-status");
        with_status(&state, "of-a", SessionStatus::Interrupted);
        let ids = |filters: Value| {
            let state = state.clone();
            async move {
                let listed = call(&state, "loose", "list_sessions", &arguments(filters))
                    .await
                    .unwrap();
                let mut ids: Vec<String> = listed["sessions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|session| session["session"].as_str().unwrap().to_string())
                    .collect();
                ids.sort();
                ids
            }
        };

        let live = ids(json!({})).await;
        assert!(!live.contains(&"of-a".to_string()), "{live:?}");
        assert_eq!(ids(json!({ "status": "live" })).await, live);
        assert_eq!(ids(json!({ "status": "interrupted" })).await, ["of-a"]);
        let any = ids(json!({ "status": "any" })).await;
        let mut both = live.clone();
        both.push("of-a".to_string());
        both.sort();
        assert_eq!(any, both);
        assert!(!any.contains(&"archived-of-a".to_string()), "{any:?}");
        // It combines with the other filters, and shows the status the listing selected.
        assert_eq!(
            ids(json!({ "status": "interrupted", "owner": "b" })).await,
            Vec::<String>::new()
        );
        let listed = call(
            &state,
            "loose",
            "list_sessions",
            &arguments(json!({ "status": "interrupted" })),
        )
        .await
        .unwrap();
        assert_eq!(listed["sessions"][0]["status"], "interrupted");

        let err = call(
            &state,
            "loose",
            "list_sessions",
            &arguments(json!({ "status": "archived" })),
        )
        .await
        .expect_err("not a status");
        assert!(err.to_string().contains("`status` must be"), "{err}");
    }

    /// `list_archived` needs no project: with none it lists every console's archive, console
    /// sessions included, and `console`, `project`, `owner` and `kind` narrow it, all having to
    /// hold.
    #[tokio::test]
    async fn the_archive_can_be_listed_without_a_project_and_narrowed() {
        let (state, _dir) = shared_console_state("tool-surface-archive-filters");
        let mut console_archived = console_session("archived-console");
        console_archived.status = SessionStatus::Archived;
        console_archived.bound_to = Some("a".to_string());
        let mut elsewhere = project_session("archived-elsewhere", "console-2", None);
        elsewhere.project_id = Some("project-2".to_string());
        elsewhere.status = SessionStatus::Archived;
        for session in [&console_archived, &elsewhere] {
            state.store.insert_session(session).unwrap();
        }
        let ids = |filters: Value| {
            let state = state.clone();
            async move {
                let listed = call(&state, "loose", "list_archived", &arguments(filters))
                    .await
                    .unwrap();
                let mut ids: Vec<String> = listed["sessions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|session| session["session"].as_str().unwrap().to_string())
                    .collect();
                ids.sort();
                ids
            }
        };

        assert_eq!(
            ids(json!({})).await,
            [
                "archived-console",
                "archived-elsewhere",
                "archived-of-a",
                "archived-of-b"
            ]
        );
        assert_eq!(
            ids(json!({ "kind": "console" })).await,
            ["archived-console"]
        );
        assert_eq!(
            ids(json!({ "kind": "project" })).await,
            ["archived-elsewhere", "archived-of-a", "archived-of-b"]
        );
        assert_eq!(
            ids(json!({ "owner": "a" })).await,
            ["archived-console", "archived-of-a"]
        );
        assert_eq!(
            ids(json!({ "owner": "a", "kind": "project" })).await,
            ["archived-of-a"]
        );
        assert_eq!(
            ids(json!({ "console": "console-2" })).await,
            ["archived-elsewhere"]
        );
        assert_eq!(
            ids(json!({ "project": "project-1", "owner": "b" })).await,
            ["archived-of-b"]
        );

        for (filter, refusal) in [
            (
                json!({ "console": "nowhere" }),
                "there is no console `nowhere`",
            ),
            (
                json!({ "owner": "nowhere" }),
                "there is no session `nowhere`",
            ),
            (
                json!({ "kind": "hub" }),
                "`kind` must be `console` or `project`, not `hub`",
            ),
            (
                json!({ "kind": "console", "project": "project-1" }),
                "`kind: console` and `project` cannot both hold: a console session is in no \
                 project",
            ),
        ] {
            let err = call(&state, "loose", "list_archived", &arguments(filter.clone()))
                .await
                .expect_err(&filter.to_string());
            assert_eq!(err.to_string(), refusal);
        }
    }

    /// A `project` named beside a `console` is looked for in that console, not in the caller's
    /// first: another console's project of the same name as the caller's own is reachable.
    #[tokio::test]
    async fn a_project_name_is_resolved_within_the_console_given() {
        let (state, _dir) = shared_console_state("tool-surface-archive-project-in-console");
        let template = state.store.get_project("project-2").unwrap().unwrap();
        state
            .store
            .insert_project(&Project {
                id: "twin".to_string(),
                name: "project-1".to_string(),
                path: "/tmp/twin".to_string(),
                ..template
            })
            .unwrap();
        let mut archived = project_session("archived-twin", "console-2", None);
        archived.project_id = Some("twin".to_string());
        archived.status = SessionStatus::Archived;
        state.store.insert_session(&archived).unwrap();

        for tool in ["list_archived", "list_sessions"] {
            let filters = if tool == "list_sessions" {
                json!({ "console": "console-2", "project": "project-1", "status": "any" })
            } else {
                json!({ "console": "console-2", "project": "project-1" })
            };
            let listed = call(&state, "loose", tool, &arguments(filters))
                .await
                .unwrap();
            let ids: Vec<&str> = listed["sessions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|session| session["session"].as_str().unwrap())
                .collect();
            let expected: &[&str] = if tool == "list_archived" {
                &["archived-twin"]
            } else {
                &[]
            };
            assert_eq!(ids, expected, "{tool}");
        }
        // Without a console the caller's own console's `project-1` wins.
        let own = call(
            &state,
            "loose",
            "list_archived",
            &arguments(json!({ "project": "project-1" })),
        )
        .await
        .unwrap();
        assert_eq!(own["sessions"].as_array().unwrap().len(), 2);
        // Two projects of the console with one name are ambiguous there, and the refusal names it.
        let template = state.store.get_project("twin").unwrap().unwrap();
        state
            .store
            .insert_project(&Project {
                id: "twin-2".to_string(),
                path: "/tmp/twin-2".to_string(),
                ..template
            })
            .unwrap();
        let err = call(
            &state,
            "loose",
            "list_archived",
            &arguments(json!({ "console": "console-2", "project": "project-1" })),
        )
        .await
        .expect_err("two projects named project-1");
        assert!(err.to_string().contains("in console `console-2`"), "{err}");
        // A project that is in no such console is refused.
        let err = call(
            &state,
            "loose",
            "list_archived",
            &arguments(json!({ "console": "console-1", "project": "twin" })),
        )
        .await
        .expect_err("twin is console-2's");
        assert!(err.to_string().contains("no project `twin`"), "{err}");
    }

    /// The archive is listed newest archived first and at most `limit` long, with `total` saying
    /// how many match.
    #[tokio::test]
    async fn the_archive_is_newest_first_and_capped_by_limit() {
        let (state, _dir) = shared_console_state("tool-surface-archive-limit");
        for (id, ended_at) in [
            ("old", Some(10)),
            ("new", Some(30)),
            ("mid", Some(20)),
            ("never", None),
        ] {
            let mut session = project_session(id, "console-1", None);
            session.status = SessionStatus::Archived;
            session.ended_at = ended_at;
            state.store.insert_session(&session).unwrap();
        }
        let list = |filters: Value| {
            let state = state.clone();
            async move { call(&state, "loose", "list_archived", &arguments(filters)).await }
        };
        let listed = list(json!({ "kind": "project", "limit": 3 }))
            .await
            .unwrap();
        let ids: Vec<&str> = listed["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|session| session["session"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["new", "mid", "old"]);
        assert_eq!(listed["total"], 6);

        let everything = list(json!({})).await.unwrap();
        assert_eq!(everything["sessions"].as_array().unwrap().len(), 6);
        assert_eq!(everything["total"], 6);
        for bad in [
            json!({ "limit": 0 }),
            json!({ "limit": 201 }),
            json!({ "limit": "5" }),
        ] {
            list(bad).await.expect_err("limit out of range");
        }
    }

    /// Both ways of reading a project give its git state as the daemon holds it: `null` before any
    /// has been read, then the branch and counts with how long ago the last check completed, and a
    /// credential in the error `git` reported removed.
    #[tokio::test]
    async fn a_projects_git_status_is_given_as_last_read() {
        let (state, _dir) = shared_console_state("tool-surface-git-status");
        let project_of_a = |listed: &Value| {
            listed["projects"]
                .as_array()
                .unwrap()
                .iter()
                .find(|project| project["project"] == "project-1")
                .unwrap()["git_status"]
                .clone()
        };

        let listed = call(&state, "a", "list_projects", &Map::new())
            .await
            .unwrap();
        assert_eq!(project_of_a(&listed), Value::Null);
        let owned = call(&state, "of-a", "get_project", &Map::new())
            .await
            .unwrap();
        assert_eq!(owned["git_status"], Value::Null);

        state.publish_git_status(crate::protocol::GitStatus {
            project: "project-1".to_string(),
            repository: true,
            branch: Some("main".to_string()),
            detached: false,
            upstream: Some("origin/main".to_string()),
            ahead: 2,
            behind: 1,
            activity: crate::protocol::GitActivity::Idle,
            error: Some("unable to access 'https://user:token@example.com/r.git/'".to_string()),
        });
        let listed = call(&state, "a", "list_projects", &Map::new())
            .await
            .unwrap();
        let status = project_of_a(&listed);
        assert_eq!(status["branch"], "main");
        assert_eq!(status["upstream"], "origin/main");
        assert_eq!(
            (&status["ahead"], &status["behind"]),
            (&json!(2), &json!(1))
        );
        assert_eq!(status["activity"], "idle");
        assert_eq!(
            status["error"],
            "unable to access 'https://example.com/r.git/'"
        );
        // No remote check has completed in this daemon's run.
        assert_eq!(status["checked_seconds_ago"], Value::Null);
        let owned = call(&state, "of-a", "get_project", &Map::new())
            .await
            .unwrap();
        assert_eq!(owned["git_status"], status);

        state.record_git_check_completed("project-1");
        let owned = call(&state, "of-a", "get_project", &Map::new())
            .await
            .unwrap();
        assert_eq!(owned["git_status"]["checked_seconds_ago"], 0);

        // The age counts on the wall clock: the monotonic clock stands still while the machine
        // sleeps, so a check made an hour ago must read as an hour whatever the `Instant` says.
        state.set_git_check_wall_time_for_test(
            "project-1",
            crate::protocol::now_millis() - 3_600_000,
        );
        let owned = call(&state, "of-a", "get_project", &Map::new())
            .await
            .unwrap();
        assert_eq!(owned["git_status"]["checked_seconds_ago"], 3600);
    }

    /// A `model` or `effort` that is not text is refused before anything is reopened.
    #[tokio::test]
    async fn a_reopen_refuses_a_model_that_is_not_text() {
        let (state, _dir) = shared_console_state("tool-surface-reopen-model");
        for key in ["model", "effort"] {
            let err = call(
                &state,
                "a",
                "reopen_session",
                &arguments(json!({ "session": "archived-of-a", key: 5 })),
            )
            .await
            .expect_err("not text");
            assert_eq!(err.to_string(), format!("`{key}` must be a string or null"));
        }
        let after = state.store.get_session("archived-of-a").unwrap().unwrap();
        assert_eq!(after.status, SessionStatus::Archived);
    }

    /// Information for an interrupted session is kept in the store for its resume (which the
    /// coordinator's tests follow); its owner's copy is sent or kept by the owner's own state;
    /// archiving the session drops what was kept, and an archived session is refused outright.
    #[tokio::test]
    async fn information_for_an_interrupted_session_is_kept_for_its_resume() {
        let (state, _dir) = peer_state("peer-share-interrupted");
        let console_a = running(&state, "a");
        with_status(&state, "of-a", SessionStatus::Interrupted);
        let share = |target: &'static str| {
            let state = state.clone();
            async move {
                call(
                    &state,
                    "loose",
                    "send_message",
                    &arguments(json!({ "session": target, "text": "the port moved" })),
                )
                .await
            }
        };

        let result = share("of-a").await.unwrap();
        assert_eq!(result["delivered"], false);
        let note = result["note"].as_str().unwrap();
        assert!(
            note.contains("interrupted") && note.contains("resumed"),
            "{note}"
        );
        assert!(note.contains("sent a copy"), "{note}");
        assert_eq!(state.store.kept_messages("of-a").unwrap().len(), 1);
        assert!(output_containing(&console_a, "the port moved").contains("Copy of information"));

        // `get_session` counts it among the messages waiting for the session.
        let described = call(
            &state,
            "a",
            "get_session",
            &arguments(json!({ "session": "of-a" })),
        )
        .await
        .unwrap();
        assert_eq!(described["queued_messages"], 1);

        // Archived while information is kept for it: nothing is kept for a later reopen.
        with_status(&state, "of-b", SessionStatus::Interrupted);
        share("of-b").await.unwrap();
        assert_eq!(state.store.kept_messages("of-b").unwrap().len(), 1);
        coordinator::archive_session(&state, "of-b").unwrap();
        assert!(state.store.kept_messages("of-b").unwrap().is_empty());

        let err = share("archived-of-a").await.expect_err("archived");
        assert!(err.to_string().contains("archived"), "{err}");
        assert!(state
            .store
            .kept_messages("archived-of-a")
            .unwrap()
            .is_empty());
    }

    /// The copy for an interrupted owner is kept like the information itself.
    #[tokio::test]
    async fn the_copy_for_an_interrupted_owner_is_kept() {
        let (state, _dir) = peer_state("peer-share-interrupted-owner");
        let _of_a = running(&state, "of-a");
        with_status(&state, "a", SessionStatus::Interrupted);
        let result = call(
            &state,
            "loose",
            "send_message",
            &arguments(json!({ "session": "of-a", "text": "heads up" })),
        )
        .await
        .unwrap();
        assert_eq!(result["delivered"], true);
        assert!(result["note"]
            .as_str()
            .unwrap()
            .contains("copy is kept, and will arrive once the owner is resumed"));
        assert_eq!(state.store.kept_messages("a").unwrap().len(), 1);
    }

    /// A resumed session whose kept information is on its queue counts each message once, not
    /// once in the store and once in the queue.
    #[tokio::test]
    async fn kept_information_that_is_queued_is_counted_once() {
        let (state, _dir) = shared_console_state("tool-surface-kept-count");
        let _live = running(&state, "of-a");
        let row = state.store.add_kept_message("of-a", "kept").unwrap();
        state.queue_kept_message("of-a", "kept", row);
        let described = call(
            &state,
            "a",
            "get_session",
            &arguments(json!({ "session": "of-a" })),
        )
        .await
        .unwrap();
        assert_eq!(described["queued_messages"], 1);
    }

    /// A running session that holds what is queued for it says so and why, in the listing and in
    /// `get_session` alike, however idle its status reads; one with no process holds nothing.
    #[tokio::test]
    async fn a_session_holding_its_queue_says_why() {
        let (state, _dir) = shared_console_state("tool-surface-write-hold");
        let held = |listed: &Value| {
            let session = listed["sessions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|session| session["session"] == "of-a")
                .unwrap()
                .clone();
            (
                session["writes_held"].clone(),
                session["queued_messages"].clone(),
            )
        };
        let fetched = || async {
            let fetched = call(
                &state,
                "a",
                "get_session",
                &arguments(json!({ "session": "of-a" })),
            )
            .await
            .unwrap();
            (
                fetched["writes_held"].clone(),
                fetched["queued_messages"].clone(),
            )
        };
        let listed = || async {
            call(&state, "a", "list_sessions", &Map::new())
                .await
                .unwrap()
        };

        assert_eq!(fetched().await, (Value::Null, json!(0)));

        // Freshly launched, before its agent's first hook.
        let live = crate::test_support::watched_live_session(
            "of-a",
            Agent::Claude,
            80,
            24,
            "sleep 30",
            None,
        );
        state.register_live((*live).clone());
        state.queue_message("of-a", "go on");
        let (hold, queued) = fetched().await;
        assert_eq!(hold["reason"], "starting");
        assert!(hold["explanation"].as_str().unwrap().contains("startup"));
        assert_eq!(queued, 1);
        assert_eq!(held(&listed().await), (hold, json!(1)));

        live.trust.end_watch();
        assert_eq!(fetched().await.0, Value::Null);

        live.mark_unsent(live.prompts_sent());
        assert_eq!(fetched().await.0["reason"], "unsent_message");
        assert_eq!(held(&listed().await).0["reason"], "unsent_message");

        // A dialog on its screen outranks the unsent message, and the session reads as waiting
        // for the user here as it does to a client, though its stored status is idle.
        live.on_output(b"Resume this conversation?\r\nEnter to confirm \xc2\xb7 Esc to resume\r\n");
        let fetched = call(
            &state,
            "a",
            "get_session",
            &arguments(json!({ "session": "of-a" })),
        )
        .await
        .unwrap();
        assert_eq!(fetched["status"], "waiting_user");
        assert_eq!(fetched["writes_held"]["reason"], "agent_dialog");
        assert_eq!(fetched["writes_held"]["dialog"], "claude_resume_at_launch");
        assert!(fetched["note"].is_string());
    }

    /// The reads are the same for every role: a bound project session, an unbound one and a console
    /// session read any session, any project's archive in any console, and the projects of their
    /// own console or another, and `yours` is worked out against whoever asks.
    #[tokio::test]
    async fn every_role_reads_the_same_things_with_ownership_relative_to_itself() {
        let (state, _dir) = peer_state("tool-surface-uniform-reads");
        // An archived session in a project of the other console.
        let mut elsewhere = project_session("archived-elsewhere", "console-2", None);
        elsewhere.project_id = Some("project-2".to_string());
        elsewhere.status = SessionStatus::Archived;
        state.store.insert_session(&elsewhere).unwrap();
        for caller in ["a", "starter", "mine"] {
            // A session of another owner, and a record of it.
            let theirs = call(
                &state,
                caller,
                "get_session",
                &arguments(json!({ "session": "of-b" })),
            )
            .await
            .unwrap();
            assert_eq!(theirs["yours"], false, "{caller}");
            let history = call(
                &state,
                caller,
                "read_session_history",
                &arguments(json!({ "session": "of-b" })),
            )
            .await
            .unwrap();
            assert_eq!(history["unavailable"], false, "{caller}");

            // The archive of a project of its own console and of another's.
            for project in ["project-1", "project-2"] {
                let archive = call(
                    &state,
                    caller,
                    "list_archived",
                    &arguments(json!({ "project": project })),
                )
                .await
                .unwrap();
                let ids: Vec<&str> = archive["sessions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|session| session["session"].as_str().unwrap())
                    .collect();
                let expected: &[&str] = if project == "project-1" {
                    &["archived-of-a", "archived-of-b"]
                } else {
                    &["archived-elsewhere"]
                };
                let mut sorted = ids.clone();
                sorted.sort();
                assert_eq!(sorted, expected, "{caller} {project}");
            }
            let err = call(
                &state,
                caller,
                "list_archived",
                &arguments(json!({ "project": "nowhere" })),
            )
            .await
            .expect_err("no such project");
            assert!(
                err.to_string().contains("there is no project"),
                "{caller}: {err}"
            );

            // Its own console in full, another's as the little that targets a project.
            let own = call(&state, caller, "list_projects", &Map::new())
                .await
                .unwrap();
            assert_eq!(own["console"], "console-1", "{caller}");
            let projects = own["projects"].as_array().unwrap();
            assert!(projects
                .iter()
                .all(|project| project["sessions"].is_array()));
            let sessions: Vec<&Value> = projects
                .iter()
                .flat_map(|project| project["sessions"].as_array().unwrap())
                .collect();
            let of_starter = sessions
                .iter()
                .find(|session| session["session"] == "mine")
                .unwrap();
            assert_eq!(of_starter["yours"], caller == "starter", "{caller}");
            let other = call(
                &state,
                caller,
                "list_projects",
                &arguments(json!({ "console": "console-2" })),
            )
            .await
            .unwrap();
            assert_eq!(other["console"], "console-2", "{caller}");
            assert_eq!(other["projects"][0]["project"], "project-2");
            assert!(other["projects"][0]["sessions"].is_null());

            let consoles = call(&state, caller, "list_consoles", &Map::new())
                .await
                .unwrap();
            let yours: Vec<(&str, bool)> = consoles["consoles"]
                .as_array()
                .unwrap()
                .iter()
                .map(|console| {
                    (
                        console["console"].as_str().unwrap(),
                        console["yours"].as_bool().unwrap(),
                    )
                })
                .collect();
            assert_eq!(
                yours,
                [("console-1", true), ("console-2", false)],
                "{caller}"
            );
        }
    }

    /// A project name that one project of the caller's console and one of another console both
    /// carry is the caller's own, while an id reaches either.
    #[tokio::test]
    async fn a_project_name_in_the_callers_console_wins_for_the_archive() {
        let (state, _dir) = shared_console_state("tool-surface-archive-precedence");
        for (id, console, archived) in [
            ("own-twin", "console-1", "archived-own"),
            ("foreign-twin", "console-2", "archived-foreign"),
        ] {
            state
                .store
                .insert_project(&Project {
                    id: id.to_string(),
                    console_id: console.to_string(),
                    name: "twin".to_string(),
                    ..state.store.get_project("project-1").unwrap().unwrap()
                })
                .unwrap();
            let mut session = project_session(archived, console, None);
            session.project_id = Some(id.to_string());
            session.status = SessionStatus::Archived;
            state.store.insert_session(&session).unwrap();
        }
        for (wanted, expected) in [
            ("twin", "archived-own"),
            ("own-twin", "archived-own"),
            ("foreign-twin", "archived-foreign"),
        ] {
            let listed = call(
                &state,
                "loose",
                "list_archived",
                &arguments(json!({ "project": wanted })),
            )
            .await
            .unwrap();
            assert_eq!(listed["sessions"][0]["session"], expected, "{wanted}");
            assert_eq!(listed["sessions"].as_array().unwrap().len(), 1, "{wanted}");
        }
    }

    /// A project name two projects of another console share is refused rather than read as
    /// either of them.
    #[tokio::test]
    async fn an_ambiguous_project_name_is_refused_for_the_archive() {
        let (state, _dir) = shared_console_state("tool-surface-ambiguous-archive");
        for id in ["twin-1", "twin-2"] {
            state
                .store
                .insert_project(&Project {
                    id: id.to_string(),
                    name: "twin".to_string(),
                    ..state.store.get_project("project-2").unwrap().unwrap()
                })
                .unwrap();
        }
        let err = call(
            &state,
            "loose",
            "list_archived",
            &arguments(json!({ "project": "twin" })),
        )
        .await
        .expect_err("ambiguous");
        assert!(err.to_string().contains("names 2 projects"), "{err}");
        call(
            &state,
            "loose",
            "list_archived",
            &arguments(json!({ "project": "twin-1" })),
        )
        .await
        .unwrap();
    }

    /// `list_projects` with `console` shows another console's projects as the little an agent needs
    /// to target one, while its own console, named or not, is listed whole. A console named
    /// ambiguously or not at all is refused.
    #[tokio::test]
    async fn another_consoles_projects_are_listed_by_id_name_description_and_tags() {
        let (state, _dir) = shared_console_state("tool-surface-other-console-projects");
        let mut project = state.store.get_project("project-2").unwrap().unwrap();
        project.description = Some("Billing".to_string());
        project.tags = vec!["rust".to_string()];
        state.store.update_project(&project).unwrap();
        state
            .store
            .update_console(&Console {
                name: "Other".to_string(),
                ..state.store.get_console("console-2").unwrap().unwrap()
            })
            .unwrap();

        for wanted in ["console-2", "Other"] {
            let listed = call(
                &state,
                "a",
                "list_projects",
                &arguments(json!({ "console": wanted })),
            )
            .await
            .unwrap();
            assert_eq!(listed["console"], "console-2", "{wanted}");
            assert_eq!(
                listed["projects"],
                json!([{
                    "project": "project-2",
                    "name": "project-2",
                    "description": "Billing",
                    "tags": ["rust"],
                }]),
                "{wanted}"
            );
        }
        for wanted in ["console-1", "Console"] {
            let own = call(
                &state,
                "a",
                "list_projects",
                &arguments(json!({ "console": wanted })),
            )
            .await
            .unwrap();
            assert_eq!(own["console"], "console-1", "{wanted}");
            assert_eq!(own["projects"][0]["project"], "project-1", "{wanted}");
            assert!(own["projects"][0]["sessions"].is_array(), "{wanted}");
        }
        let plain = call(&state, "a", "list_projects", &Map::new())
            .await
            .unwrap();
        assert_eq!(plain["projects"].as_array().unwrap().len(), 1);

        state
            .store
            .update_console(&Console {
                name: "Console".to_string(),
                ..state.store.get_console("console-2").unwrap().unwrap()
            })
            .unwrap();
        for (wanted, reason) in [
            ("Console", "names 2 consoles"),
            ("nowhere", "there is no console `nowhere`"),
        ] {
            let err = call(
                &state,
                "a",
                "list_projects",
                &arguments(json!({ "console": wanted })),
            )
            .await
            .expect_err("refused");
            assert!(err.to_string().contains(reason), "{wanted}: {err}");
        }
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
    /// checked to be untouched; `instruct_session` has no such visible effect to compare.
    #[tokio::test]
    async fn writes_are_refused_for_sessions_the_caller_does_not_own() {
        let (state, _dir) = shared_console_state("tool-surface-writes");
        let cases = [
            (
                "instruct_session",
                "of-b",
                "bound to another console session, `b`",
            ),
            ("instruct_session", "loose", "not bound to any session"),
            ("instruct_session", "b", "is a console session;"),
            ("instruct_session", "foreign", "not bound to any session"),
            (
                "interrupt_session",
                "of-b",
                "bound to another console session, `b`",
            ),
            ("interrupt_session", "loose", "not bound to any session"),
            ("interrupt_session", "b", "is a console session;"),
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
            if tool != "instruct_session" {
                assert_eq!(before.status, after.status, "{tool} {target}");
                assert_eq!(before.ended_at, after.ended_at, "{tool} {target}");
            }
        }

        // The caller's own is not refused by the ownership check: `interrupt_session` gets as
        // far as the session's own state, here no process to send a key to.
        let err = call(
            &state,
            "a",
            "interrupt_session",
            &arguments(json!({ "session": "of-a" })),
        )
        .await
        .expect_err("refused");
        assert_eq!(
            err.downcast_ref::<CodedError>().map(|err| err.code),
            Some(error_code::SESSION_NOT_RUNNING),
            "{err}"
        );
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

    /// A message for an interrupted session of the caller's is refused as before, and the refusal
    /// points at `reopen_session`, whose `text` delivers it on relaunch.
    #[tokio::test]
    async fn a_message_for_an_interrupted_session_points_at_reopen_session() {
        let (state, _dir) = shared_console_state("send-to-interrupted");
        with_status(&state, "of-a", SessionStatus::Interrupted);
        let err = call(
            &state,
            "a",
            "instruct_session",
            &arguments(json!({ "session": "of-a", "text": "go on" })),
        )
        .await
        .expect_err("refused");
        assert_eq!(
            err.downcast_ref::<CodedError>().map(|err| err.code),
            Some(error_code::SESSION_NOT_RUNNING),
            "{err}"
        );
        let message = err.to_string();
        assert!(message.contains("interrupted"), "{message}");
        assert!(message.contains("`reopen_session`"), "{message}");
    }

    /// The same pointer, said for an archived session.
    #[tokio::test]
    async fn a_message_for_an_archived_session_points_at_reopen_session() {
        let (state, _dir) = shared_console_state("send-to-archived");
        with_status(&state, "of-a", SessionStatus::Archived);
        let err = call(
            &state,
            "a",
            "instruct_session",
            &arguments(json!({ "session": "of-a", "text": "go on" })),
        )
        .await
        .expect_err("refused");
        assert_eq!(
            err.downcast_ref::<CodedError>().map(|err| err.code),
            Some(error_code::SESSION_NOT_RUNNING),
            "{err}"
        );
        let message = err.to_string();
        assert!(message.contains("archived"), "{message}");
        assert!(message.contains("`reopen_session`"), "{message}");
    }

    /// A refusal to drive a session that is not the caller's says why and what the caller can still
    /// do, naming `send_message` as the caller's agent qualifies it; a session of the caller's own
    /// is told to say it to the user instead, since `send_message` is refused for it.
    #[tokio::test]
    async fn a_refusal_to_drive_a_session_that_is_not_the_callers_names_send_message() {
        let (state, _dir) = shared_console_state("tool-refusal-send-message");
        for target in ["of-b", "loose", "b"] {
            let message = refused(
                &state,
                "a",
                "instruct_session",
                json!({ "session": target, "text": "hi" }),
            )
            .await;
            assert!(
                message.contains(
                    "You can still tell it something with `mcp__octoboard__send_message`"
                ),
                "{target}: {message}"
            );
        }
        let message = refused(
            &state,
            "a",
            "instruct_session",
            json!({ "session": "of-b", "text": "hi" }),
        )
        .await;
        assert!(
            message.contains("Only that session instructs or drives it")
                && message.contains("its owner is sent a copy"),
            "{message}"
        );

        let mut grok = state.session_record("a").unwrap();
        grok.agent = Agent::Grok;
        state.store.update_session(&grok).unwrap();
        let message = refused(
            &state,
            "a",
            "instruct_session",
            json!({ "session": "loose", "text": "hi" }),
        )
        .await;
        assert!(message.contains("`octoboard__send_message`"), "{message}");
        assert!(
            !message.contains("mcp__octoboard__send_message"),
            "{message}"
        );

        let message = refused(
            &state,
            "a",
            "instruct_session",
            json!({ "session": "a", "text": "hi" }),
        )
        .await;
        assert!(!message.contains("send_message"), "{message}");
    }

    /// `interrupt_session` on a running session of the caller's answers with the function's
    /// result: here one at its prompt, with nothing to interrupt.
    #[tokio::test]
    async fn interrupt_session_answers_for_a_running_session_of_the_caller_s() {
        let (state, _dir) = shared_console_state("tool-interrupt-idle");
        with_status(&state, "of-a", SessionStatus::Idle);
        let _live = running(&state, "of-a");

        let answer = call(
            &state,
            "a",
            "interrupt_session",
            &arguments(json!({ "session": "of-a" })),
        )
        .await
        .unwrap();

        assert_eq!(answer["interrupted"], false);
        assert_eq!(answer["status"], "idle");
        assert!(
            answer["note"]
                .as_str()
                .is_some_and(|note| note.starts_with("Nothing to interrupt")),
            "{answer}"
        );
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
                description: None,
                git_workflow: Default::default(),
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
            ("mine", "instruct_session"),
            ("of-a", "archive_session"),
            ("starter", "add_project"),
            ("mine", "show_page"),
            ("mine", "list_pages"),
            ("mine", "delete_pages"),
            ("a", "report"),
            ("mine", "transfer_session"),
            ("mine", "release_session"),
            ("mine", "handoff_session"),
            ("mine", "switch_session_account"),
            ("mine", "set_session_model"),
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
    pub(in crate::mcp) fn lead_state(name: &str) -> (Arc<AppState>, ScratchDir) {
        let (state, dir) = peer_state(name);
        state
            .store
            .set_binding("starter", None, "a", true, false, false)
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
        for tool in ["instruct_session", "archive_session"] {
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
        // Straight to the store: locking an archived session is refused, an old lock is not.
        state.store.set_session_locked("mine", true).unwrap();
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
            (
                "instruct_session",
                "of-a",
                "bound to a console session, `a`",
            ),
            ("instruct_session", "loose", "not bound to any session"),
            ("instruct_session", "starter", "your own session"),
            (
                "instruct_session",
                "other-project",
                "not bound to any session",
            ),
            ("instruct_session", "a", "is a console session"),
            (
                "interrupt_session",
                "of-a",
                "bound to a console session, `a`",
            ),
            ("interrupt_session", "starter", "your own session"),
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
                "send_message",
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
            "send_message",
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
                    "send_message",
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
            "send_message",
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
            "send_message",
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
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
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

    /// `update_project` sets a description to the given text, leaves it alone when absent, clears
    /// it with `null`, and refuses a value that is not text.
    #[tokio::test]
    async fn update_project_sets_leaves_and_clears_the_description() {
        let (state, _dir) = settings_state("update-project-description");
        let update = |args: Value| {
            let state = state.clone();
            async move { call(&state, "a", "update_project", &arguments(args)).await }
        };

        let set = update(json!({ "project": "project-1", "description": "Billing." }))
            .await
            .unwrap();
        assert_eq!(set["description"], "Billing.");

        let kept = update(json!({ "project": "project-1", "pinned": true }))
            .await
            .unwrap();
        assert_eq!(kept["description"], "Billing.");

        let message = refused(
            &state,
            "a",
            "update_project",
            json!({ "project": "project-1", "description": 3 }),
        )
        .await;
        assert!(message.contains("string or null"), "{message}");
        let stored = state.store.get_project("project-1").unwrap().unwrap();
        assert_eq!(stored.description.as_deref(), Some("Billing."));

        let answer = update(json!({ "project": "project-1", "description": null }))
            .await
            .unwrap();
        assert!(answer["description"].is_null());
        let stored = state.store.get_project("project-1").unwrap().unwrap();
        assert_eq!(stored.description, None);
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

    /// The git workflow settings are read and changed through the settings tools: the
    /// application-wide ones by `get_settings` / `update_settings`, a project's own by
    /// `update_project`, and `get_project` and `list_projects` report the override beside the
    /// values in force.
    #[tokio::test]
    async fn the_git_workflow_settings_are_read_and_changed_through_the_tools() {
        let (state, _dir) = settings_state("git-workflow-settings-tools");
        let read = call(&state, "a", "get_settings", &Map::new())
            .await
            .unwrap();
        assert_eq!(
            read["git_workflow"],
            json!({
                "development": "worktree",
                "rebase_before_merge": true,
                "delivery": "ask",
                "revert_gate": false,
            })
        );

        let updated = call(
            &state,
            "a",
            "update_settings",
            &arguments(json!({ "git_workflow": { "delivery": "merge", "revert_gate": true } })),
        )
        .await
        .unwrap();
        assert_eq!(updated["git_workflow"]["delivery"], "merge");
        assert_eq!(updated["git_workflow"]["revert_gate"], true);
        assert_eq!(updated["git_workflow"]["development"], "worktree");
        for bad in [
            json!({ "git_workflow": { "delivery": "rebase" } }),
            json!({ "git_workflow": { "development": "yes" } }),
            json!({ "git_workflow": { "typo": true } }),
        ] {
            let message = refused(&state, "a", "update_settings", bad).await;
            assert!(message.contains("git_workflow"), "{message}");
        }
        assert_eq!(
            state.store.git_workflow_settings().unwrap().delivery,
            crate::git_workflow::Delivery::Merge
        );

        let project = call(
            &state,
            "a",
            "update_project",
            &arguments(json!({
                "project": "project-1",
                "git_workflow": { "development": "in_place", "delivery": "pull_request" },
            })),
        )
        .await
        .unwrap();
        assert_eq!(
            project["git_workflow"]["override"],
            json!({
                "development": "in_place",
                "rebase_before_merge": null,
                "delivery": "pull_request",
                "revert_gate": null,
            })
        );
        assert_eq!(
            project["git_workflow"]["effective"],
            json!({
                "development": "in_place",
                "rebase_before_merge": true,
                "delivery": "pull_request",
                "revert_gate": true,
            })
        );

        // An item left out stays; `null` goes back to following the application-wide value.
        let project = call(
            &state,
            "a",
            "update_project",
            &arguments(json!({
                "project": "project-1",
                "git_workflow": { "delivery": null, "revert_gate": false },
            })),
        )
        .await
        .unwrap();
        assert_eq!(
            project["git_workflow"]["override"]["development"],
            "in_place"
        );
        assert!(project["git_workflow"]["override"]["delivery"].is_null());
        assert_eq!(project["git_workflow"]["effective"]["delivery"], "merge");
        assert_eq!(project["git_workflow"]["effective"]["revert_gate"], false);

        let message = refused(
            &state,
            "a",
            "update_project",
            json!({ "project": "project-1", "git_workflow": { "delivery": "rebase" } }),
        )
        .await;
        assert!(message.contains("git_workflow"), "{message}");

        let listed = call(&state, "a", "list_projects", &Map::new())
            .await
            .unwrap();
        assert_eq!(
            listed["projects"][0]["git_workflow"]["effective"]["development"],
            "in_place"
        );
        let own = call(&state, "of-a", "get_project", &Map::new())
            .await
            .unwrap();
        assert_eq!(own["git_workflow"]["override"]["revert_gate"], false);
        assert_eq!(own["git_workflow"]["effective"]["delivery"], "merge");
        // A project session cannot change them.
        assert!(refused(&state, "of-a", "update_settings", json!({}))
            .await
            .contains("not a tool"));
    }

    /// `get_project` resolves what a project leaves unset against its console, since a project
    /// session cannot read the console: the project's own value wins, the console's project
    /// session value fills in the rest, and the raw settings stay as stored.
    #[tokio::test]
    async fn get_project_gives_the_settings_resolved_against_the_console() {
        let (state, _dir) = shared_console_state("get-project-effective");
        let mut console = state.store.get_console("console-1").unwrap().unwrap();
        console.project_session_agent = Some(Agent::Codex);
        console.claude_project_session_model = Some("console-model".to_string());
        console.claude_project_session_effort = Some("console-effort".to_string());
        state.store.update_console(&console).unwrap();
        let mut project = state.store.get_project("project-1").unwrap().unwrap();
        project.default_agent = None;
        *project.launch_mut(Agent::Claude).0 = Some("project-model".to_string());
        state.store.update_project(&project).unwrap();

        let own = call(&state, "of-a", "get_project", &Map::new())
            .await
            .unwrap();
        assert!(own["default_agent"].is_null());
        assert_eq!(own["accounts"]["claude"], "inherit");
        assert_eq!(own["effective"]["default_agent"], "codex");
        assert_eq!(own["effective"]["accounts"]["claude"], "default");
        assert_eq!(
            own["effective"]["launch"]["claude"]["model"],
            "project-model"
        );
        assert_eq!(
            own["effective"]["launch"]["claude"]["effort"],
            "console-effort"
        );
        assert!(own["effective"]["launch"]["codex"]["model"].is_null());
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
                    message.contains("send_message") || message.contains("your own"),
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

    /// The automatic approvals of session requests, resume requests, account switches and handoffs,
    /// the switch asking for questions as choices, and the interface language, are the user's alone:
    /// an agent can neither read them nor change them through the settings tools, whatever it
    /// passes.
    #[tokio::test]
    async fn the_users_own_switches_are_out_of_the_agents_reach() {
        let (state, _dir) = settings_state("app-settings-auto-approve");
        let listed = call(&state, "a", "get_settings", &arguments(json!({})))
            .await
            .unwrap();
        assert!(listed.get("auto_approve_session_requests").is_none());
        assert!(listed.get("auto_approve_resume_requests").is_none());
        assert!(listed.get("auto_approve_account_switches").is_none());
        assert!(listed.get("auto_approve_handoffs").is_none());
        assert!(listed.get("ask_questions_as_choices").is_none());
        assert!(listed.get("interface_language").is_none());

        for (stored, passed) in [(false, true), (true, false)] {
            state
                .store
                .set_auto_approve_session_requests(stored)
                .unwrap();
            state
                .store
                .set_auto_approve_resume_requests(stored)
                .unwrap();
            state
                .store
                .set_auto_approve_account_switches(stored)
                .unwrap();
            state.store.set_auto_approve_handoffs(stored).unwrap();
            state.store.set_ask_questions_as_choices(stored).unwrap();
            state.store.set_interface_language("ja").unwrap();
            let updated = call(
                &state,
                "a",
                "update_settings",
                &arguments(json!({
                    "auto_approve_session_requests": passed,
                    "auto_approve_resume_requests": passed,
                    "auto_approve_account_switches": passed,
                    "auto_approve_handoffs": passed,
                    "ask_questions_as_choices": passed,
                    "interface_language": "de",
                    "default_clone_dir": "/tmp/clones",
                })),
            )
            .await
            .unwrap();
            assert!(updated.get("auto_approve_session_requests").is_none());
            assert!(updated.get("auto_approve_resume_requests").is_none());
            assert!(updated.get("auto_approve_account_switches").is_none());
            assert!(updated.get("auto_approve_handoffs").is_none());
            assert!(updated.get("ask_questions_as_choices").is_none());
            assert!(updated.get("interface_language").is_none());
            assert_eq!(updated["default_clone_dir"], "/tmp/clones");
            assert_eq!(state.store.auto_approve_session_requests().unwrap(), stored);
            assert_eq!(state.store.auto_approve_resume_requests().unwrap(), stored);
            assert_eq!(state.store.auto_approve_account_switches().unwrap(), stored);
            assert_eq!(state.store.auto_approve_handoffs().unwrap(), stored);
            assert_eq!(state.store.ask_questions_as_choices().unwrap(), stored);
            assert_eq!(
                state.store.interface_language().unwrap().as_deref(),
                Some("ja")
            );
        }
    }

    /// `get_settings` marks each account the user turned off, the default account of an agent
    /// included, so an agent can tell which ones a new session cannot take.
    #[tokio::test]
    async fn get_settings_marks_the_accounts_that_are_turned_off() {
        let (state, _dir) = settings_state("app-settings-disabled-accounts");
        let disabled_of = |listed: &Value| -> Vec<(String, String, bool)> {
            listed["accounts"]
                .as_array()
                .unwrap()
                .iter()
                .map(|account| {
                    (
                        account["agent"].as_str().unwrap().to_string(),
                        account["id"].as_str().unwrap().to_string(),
                        account["disabled"].as_bool().unwrap(),
                    )
                })
                .collect()
        };
        let listed = call(&state, "a", "get_settings", &arguments(json!({})))
            .await
            .unwrap();
        assert!(disabled_of(&listed)
            .iter()
            .all(|(_, _, disabled)| !disabled));

        state
            .store
            .set_default_account_disabled(Agent::Codex, true)
            .unwrap();
        let first = state.store.list_accounts().unwrap().remove(0);
        state.store.set_account_disabled(&first.id, true).unwrap();
        let listed = call(&state, "a", "get_settings", &arguments(json!({})))
            .await
            .unwrap();
        let on: Vec<_> = disabled_of(&listed)
            .into_iter()
            .filter(|(_, _, disabled)| *disabled)
            .map(|(agent, id, _)| (agent, id))
            .collect();
        assert_eq!(on.len(), 2, "{on:?}");
        assert!(on.contains(&("codex".to_string(), "default".to_string())));
        assert!(on.contains(&(first.agent.wire_name().to_string(), first.id)));
    }

    /// Starting quota windows spends the user's quota, so an agent can neither read the setting
    /// nor change it through the settings tools, whatever it passes.
    #[tokio::test]
    async fn the_quota_window_setting_is_out_of_the_agents_reach() {
        let (state, _dir) = settings_state("app-settings-quota-windows");
        let listed = call(&state, "a", "get_settings", &arguments(json!({})))
            .await
            .unwrap();
        assert!(listed.get("auto_start_quota_windows").is_none());

        let updated = call(
            &state,
            "a",
            "update_settings",
            &arguments(json!({
                "auto_start_quota_windows": true,
                "default_clone_dir": "/tmp/clones",
            })),
        )
        .await
        .unwrap();
        assert!(updated.get("auto_start_quota_windows").is_none());
        assert_eq!(updated["default_clone_dir"], "/tmp/clones");
        assert!(!state.store.auto_start_quota_windows().unwrap());
    }

    /// The settings listing names every agent's default account, never an account's directory, and
    /// the only writes are to the five settings a console session may change.
    #[tokio::test]
    async fn settings_are_listed_without_directories_and_five_of_them_are_writable() {
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
        assert_eq!(updated["auto_delete_unresumable_archives"], true);
        let updated = call(
            &state,
            "a",
            "update_settings",
            &arguments(json!({ "auto_delete_unresumable_archives": false })),
        )
        .await
        .unwrap();
        assert_eq!(updated["auto_delete_unresumable_archives"], false);
        assert!(!state.store.auto_delete_unresumable_archives().unwrap());
        let refused_value = refused(
            &state,
            "a",
            "update_settings",
            json!({ "auto_delete_unresumable_archives": "no" }),
        )
        .await;
        assert!(refused_value.contains("true or false"), "{refused_value}");
        let read = call(&state, "a", "get_settings", &Map::new())
            .await
            .unwrap();
        assert_eq!(read["auto_delete_unresumable_archives"], false);
        assert_eq!(read["auto_select_account"], false);
        assert_eq!(read["sidebar_order_by_status"], true);
        let selected = call(
            &state,
            "a",
            "update_settings",
            &arguments(json!({ "auto_select_account": true })),
        )
        .await
        .unwrap();
        assert_eq!(selected["auto_select_account"], true);
        assert!(state.store.auto_select_account().unwrap());
        // Octoboard's own switching of sessions is the user's alone to turn on: it is neither
        // listed nor taken.
        let ignored = call(
            &state,
            "a",
            "update_settings",
            &arguments(json!({ "auto_switch_on_usage_limit": true })),
        )
        .await
        .unwrap();
        assert!(ignored.get("auto_switch_on_usage_limit").is_none());
        assert!(!state.store.auto_switch_on_usage_limit().unwrap());
        let ordered = call(
            &state,
            "a",
            "update_settings",
            &arguments(json!({ "sidebar_order_by_status": false })),
        )
        .await
        .unwrap();
        assert_eq!(ordered["sidebar_order_by_status"], false);
        assert!(!state.store.sidebar_order_by_status().unwrap());
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

        let message = refused(
            &state,
            "a",
            "add_project",
            json!({ "source": "local", "path": "/tmp", "description": 3 }),
        )
        .await;
        assert!(message.contains("description"), "{message}");
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
                "description": "  A billing service.  ",
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
        assert_eq!(project["description"], "A billing service.");
        assert_eq!(project["accounts"]["codex"], "acct-codex");
        assert_eq!(project["launch"]["claude"]["effort"], "low");

        let bare = dir.join("bare");
        std::fs::create_dir(&bare).unwrap();
        let added = call(
            &state,
            "a",
            "add_project",
            &arguments(json!({
                "source": "local",
                "path": bare.to_string_lossy(),
                "description": null,
            })),
        )
        .await
        .unwrap();
        assert!(added["projects"][0]["description"].is_null());
    }

    /// `add_project` takes the project's own git workflow values as `update_project` does, except
    /// that an item left out or `null` follows the application-wide value; a parent source gives
    /// them to every project it adds, and a bad value is refused before anything is stored.
    #[tokio::test]
    async fn add_project_takes_the_projects_own_git_workflow_values() {
        let (state, dir) = settings_state("add-project-git-workflow");
        let before = state.store.list_projects().unwrap().len();
        for bad in [
            json!({ "delivery": "rebase" }),
            json!({ "development": "yes" }),
            json!({ "typo": true }),
        ] {
            let args = json!({ "source": "local", "path": "/tmp", "git_workflow": bad });
            let message = refused(&state, "a", "add_project", args).await;
            assert!(message.contains("git_workflow"), "{message}");
        }
        assert_eq!(state.store.list_projects().unwrap().len(), before);

        call(
            &state,
            "a",
            "update_settings",
            &arguments(json!({ "git_workflow": { "revert_gate": true } })),
        )
        .await
        .unwrap();
        let repo = dir.join("repo");
        std::fs::create_dir(&repo).unwrap();
        let added = call(
            &state,
            "a",
            "add_project",
            &arguments(json!({
                "source": "local",
                "path": repo.to_string_lossy(),
                "git_workflow": { "development": "in_place", "delivery": "merge", "revert_gate": null },
            })),
        )
        .await
        .unwrap();
        assert_eq!(
            added["projects"][0]["git_workflow"]["override"],
            json!({
                "development": "in_place",
                "rebase_before_merge": null,
                "delivery": "merge",
                "revert_gate": null,
            })
        );
        assert_eq!(
            added["projects"][0]["git_workflow"]["effective"]["revert_gate"],
            true
        );

        let parent = dir.join("parent");
        for name in ["one", "two"] {
            std::fs::create_dir_all(parent.join(name).join(".git")).unwrap();
        }
        let added = call(
            &state,
            "a",
            "add_project",
            &arguments(json!({
                "source": "parent",
                "path": parent.to_string_lossy(),
                "git_workflow": { "rebase_before_merge": false },
            })),
        )
        .await
        .unwrap();
        let projects = added["projects"].as_array().unwrap();
        assert_eq!(projects.len(), 2);
        assert!(projects
            .iter()
            .all(|project| project["git_workflow"]["override"]["rebase_before_merge"] == false));

        let bare = dir.join("bare");
        std::fs::create_dir(&bare).unwrap();
        let added = call(
            &state,
            "a",
            "add_project",
            &arguments(json!({ "source": "local", "path": bare.to_string_lossy() })),
        )
        .await
        .unwrap();
        assert!(added["projects"][0]["git_workflow"]["override"]["delivery"].is_null());
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

    /// What an idle session has left running is part of its description, and only while it is
    /// idle: any other status describes none, whatever the daemon still remembers.
    #[test]
    fn a_description_lists_the_background_tasks_of_an_idle_session_only() {
        let (state, _dir) = crate::test_support::app_state("mcp-exec-describe-background");
        state.store.insert_console(&console()).unwrap();
        let caller = console_session("hub");
        let session = crate::test_support::project_session("s", Agent::Claude, SessionStatus::Idle);
        let describe = |session: &Session| {
            describe_session(&state, session, &caller, &HashMap::new(), &HashMap::new()).unwrap()
                ["background_tasks"]
                .clone()
        };

        assert_eq!(describe(&session), json!([]));

        state.apply_background_tasks_change(
            "s",
            crate::hooks::BackgroundTasksChange::Replace(vec![crate::protocol::BackgroundTask {
                id: "b1".to_string(),
                kind: Some("shell".to_string()),
                description: None,
            }]),
        );
        assert_eq!(describe(&session), json!([{"id": "b1", "type": "shell"}]));

        let working = Session {
            status: SessionStatus::Working,
            ..session
        };
        assert_eq!(describe(&working), json!([]));
    }

    /// An owner choosing a session for a task reads how full its context is from the description:
    /// the figures the UI shows, or null before any was reported.
    #[test]
    fn a_description_carries_the_sessions_context() {
        let (state, _dir) = crate::test_support::app_state("mcp-exec-describe-context");
        state.store.insert_console(&console()).unwrap();
        let caller = console_session("hub");
        let session = crate::test_support::project_session("s", Agent::Claude, SessionStatus::Idle);
        assert!(
            describe_session(&state, &session, &caller, &HashMap::new(), &HashMap::new()).unwrap()
                ["context"]
                .is_null()
        );

        let used = Session {
            context: Some(crate::protocol::SessionContext {
                tokens: 120_000,
                window: Some(200_000),
                used_percent: Some(60),
            }),
            ..session
        };
        assert_eq!(
            describe_session(&state, &used, &caller, &HashMap::new(), &HashMap::new()).unwrap()
                ["context"],
            json!({"tokens": 120_000, "window": 200_000, "used_percent": 60})
        );
    }

    /// A Codex project session `a` on the account `codex-work`, in a console whose sessions
    /// otherwise default to Claude Code on the default account.
    fn codex_caller_state(name: &str) -> (Arc<AppState>, ScratchDir, Session) {
        let (state, dir) = shared_console_state(name);
        state
            .store
            .insert_account(&crate::protocol::Account {
                id: "codex-work".to_string(),
                agent: Agent::Codex,
                name: "Work".to_string(),
                config_dir: "/tmp/codex-work".to_string(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
            })
            .unwrap();
        let caller = Session {
            agent: Agent::Codex,
            role: Role::Project,
            project_id: Some("project-1".to_string()),
            account_id: Some("codex-work".to_string()),
            config_dir: Some("/tmp/codex-work".to_string()),
            ..state.session_record("a").unwrap()
        };
        (state, dir, caller)
    }

    /// The agent and account `session_in` reads for `caller` from `args`, starting a session in
    /// `project` of `console`.
    fn agent_and_account(
        state: &Arc<AppState>,
        caller: &Session,
        console: &str,
        project: &str,
        args: Value,
    ) -> (Option<Agent>, Option<Option<String>>) {
        let console = state.store.get_console(console).unwrap().unwrap();
        let project = state.store.get_project(project).unwrap();
        let mut args = arguments(args);
        args.insert("brief".to_string(), json!({ "goal": "Fix the build" }));
        let open = session_in(state, caller, &console, project, &args).unwrap();
        (open.agent, open.account)
    }

    /// A session a session starts runs on the caller's own agent and account when the call names
    /// neither, whatever the console and project set; an agent or account the call names wins.
    #[test]
    fn a_started_session_defaults_to_the_callers_agent_and_account() {
        let (state, _dir, caller) = codex_caller_state("defaults-to-caller");
        let codex_work = Some(Some("codex-work".to_string()));

        let omitted = agent_and_account(&state, &caller, "console-1", "project-1", json!({}));
        assert_eq!(omitted, (Some(Agent::Codex), codex_work.clone()));
        // Naming the caller's own agent keeps the caller's account too.
        let same_agent = json!({ "agent": "codex" });
        assert_eq!(
            agent_and_account(&state, &caller, "console-1", "project-1", same_agent),
            (Some(Agent::Codex), codex_work)
        );
        // Another agent takes what the project and console set for it.
        let other_agent = json!({ "agent": "claude" });
        assert_eq!(
            agent_and_account(&state, &caller, "console-1", "project-1", other_agent),
            (Some(Agent::Claude), None)
        );
        let named = json!({ "account": "default" });
        assert_eq!(
            agent_and_account(&state, &caller, "console-1", "project-1", named),
            (Some(Agent::Codex), Some(None))
        );
    }

    /// A console session does not carry its own agent and account into a project: a session it
    /// starts there runs on the project's default agent unless the call names one, and on the
    /// caller's account only when that is the caller's own agent too.
    #[test]
    fn a_console_session_starts_sessions_on_the_projects_default_agent() {
        let (state, _dir, project_caller) = codex_caller_state("console-caller-default-agent");
        let caller = Session {
            role: Role::Console,
            project_id: None,
            ..project_caller.clone()
        };
        let project = state.store.get_project("project-1").unwrap().unwrap();
        let console = state.store.get_console("console-1").unwrap().unwrap();
        let expected = coordinator::session_agent(None, Some(&project), &console);
        assert_ne!(expected, Agent::Codex);

        // An omitted agent resolves later, from the project: the request names none.
        assert_eq!(
            agent_and_account(&state, &caller, "console-1", "project-1", json!({})),
            (None, None)
        );
        // An explicit agent wins, and the caller's account goes with its own agent only.
        let codex = json!({ "agent": "codex" });
        assert_eq!(
            agent_and_account(&state, &caller, "console-1", "project-1", codex),
            (Some(Agent::Codex), Some(Some("codex-work".to_string())))
        );
        let claude = json!({ "agent": "claude" });
        assert_eq!(
            agent_and_account(&state, &caller, "console-1", "project-1", claude),
            (Some(Agent::Claude), None)
        );
        // A project session keeps following its own agent.
        assert_eq!(
            agent_and_account(&state, &project_caller, "console-1", "project-1", json!({})).0,
            Some(Agent::Codex)
        );
        // A project whose default agent is the caller's own brings the caller's account along.
        state
            .store
            .update_project(&Project {
                default_agent: Some(Agent::Codex),
                ..project
            })
            .unwrap();
        assert_eq!(
            agent_and_account(&state, &caller, "console-1", "project-1", json!({})),
            (None, Some(Some("codex-work".to_string())))
        );
    }

    /// The caller's account is left for what the project and console set when it could not run
    /// the session: turned off, its quota used up, or the account removed. The caller's agent is
    /// left for theirs when it could not either.
    #[test]
    fn the_callers_account_or_agent_is_passed_over_when_it_could_not_open_a_session() {
        use crate::protocol::{AccountQuota, QuotaWindow, QuotaWindowKind};

        let (state, _dir, caller) = codex_caller_state("defaults-passed-over");
        state.account_quotas.hold_in_dir_for_test(
            &AccountQuota {
                agent: Agent::Codex,
                account: Some("codex-work".to_string()),
                fetched_at: 1,
                plan: None,
                windows: vec![QuotaWindow {
                    kind: QuotaWindowKind::Weekly,
                    label: None,
                    used_percent: 100.0,
                    resets_at: None,
                    window_minutes: None,
                }],
                unavailable: None,
            },
            "/tmp/codex-work",
        );
        assert_eq!(
            agent_and_account(&state, &caller, "console-1", "project-1", json!({})),
            (Some(Agent::Codex), None)
        );

        let (state, _dir, caller) = codex_caller_state("defaults-account-disabled");
        state
            .store
            .set_account_disabled("codex-work", true)
            .unwrap();
        assert_eq!(
            agent_and_account(&state, &caller, "console-1", "project-1", json!({})),
            (Some(Agent::Codex), None)
        );
        state
            .store
            .set_account_disabled("codex-work", false)
            .unwrap();
        assert_eq!(
            agent_and_account(&state, &caller, "console-1", "project-1", json!({})),
            (Some(Agent::Codex), Some(Some("codex-work".to_string())))
        );

        let (state, _dir, caller) = codex_caller_state("defaults-account-removed");
        state.store.delete_account("codex-work").unwrap();
        assert_eq!(
            agent_and_account(&state, &caller, "console-1", "project-1", json!({})),
            (Some(Agent::Codex), None)
        );

        let (state, _dir, caller) = codex_caller_state("defaults-agent-not-added");
        state
            .store
            .set_added_agents_if_undecided(&[Agent::Claude, Agent::Grok])
            .unwrap();
        assert_eq!(
            agent_and_account(&state, &caller, "console-1", "project-1", json!({})),
            (None, None)
        );
    }

    /// A session asked for in another console runs on what that console and its project set: its
    /// accounts may be another job's.
    #[test]
    fn a_session_in_another_console_does_not_follow_the_caller() {
        let (state, _dir, caller) = codex_caller_state("defaults-other-console");
        assert_eq!(
            agent_and_account(&state, &caller, "console-2", "project-2", json!({})),
            (None, None)
        );
    }

    fn quota(agent: Agent, account: Option<&str>, windows: Vec<QuotaWindow>) -> AccountQuota {
        AccountQuota {
            agent,
            account: account.map(str::to_string),
            fetched_at: 1,
            plan: None,
            windows,
            unavailable: None,
        }
    }

    fn hold(
        state: &Arc<AppState>,
        agent: Agent,
        account: Option<&str>,
        config_dir: Option<&str>,
        windows: Vec<QuotaWindow>,
    ) {
        let quota = quota(agent, account, windows);
        match config_dir {
            Some(dir) => state.account_quotas.hold_in_dir_for_test(&quota, dir),
            None => state.account_quotas.hold_for_test(&quota),
        }
    }

    fn weekly(used_percent: f64, resets_at: i64) -> QuotaWindow {
        QuotaWindow {
            kind: QuotaWindowKind::Weekly,
            label: None,
            used_percent,
            resets_at: Some(resets_at),
            window_minutes: None,
        }
    }

    fn insert_codex(state: &Arc<AppState>, id: &str, dir: &str) {
        state
            .store
            .insert_account(&Account {
                id: id.to_string(),
                agent: Agent::Codex,
                name: id.to_string(),
                config_dir: dir.to_string(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
            })
            .unwrap();
    }

    fn own_console_account(
        state: &Arc<AppState>,
        caller: &Session,
        args: Value,
    ) -> Option<Option<String>> {
        agent_and_account(state, caller, "console-1", "project-1", args).1
    }

    #[test]
    fn auto_select_account_skips_a_sharing_caller_only_when_enabled() {
        let (state, _dir, caller) = codex_caller_state("auto-select-sharing-caller-scope");
        let account = state.store.get_account("codex-work").unwrap().unwrap();
        let (_, _lock) = state.lock_account_sharing(&account, |_, _| Ok(())).unwrap();
        assert_eq!(
            own_console_account(&state, &caller, json!({})),
            Some(Some("codex-work".to_string())),
        );
        state.store.set_auto_select_account(true).unwrap();
        assert_eq!(own_console_account(&state, &caller, json!({})), Some(None));
    }

    #[test]
    fn auto_select_account_uses_the_project_pin_when_the_callers_sharing_change_runs() {
        let (state, _dir, caller) = codex_caller_state("auto-select-sharing-caller-project-pin");
        insert_codex(&state, "project-pin", "/tmp/codex-project-pin");
        let mut project = state.store.get_project("project-1").unwrap().unwrap();
        project.codex_account_id = ProjectAccount::Account {
            id: "project-pin".to_string(),
        };
        state.store.update_project(&project).unwrap();
        let account = state.store.get_account("codex-work").unwrap().unwrap();
        let (_, _lock) = state.lock_account_sharing(&account, |_, _| Ok(())).unwrap();
        assert_eq!(
            own_console_account(&state, &caller, json!({})),
            Some(Some("codex-work".to_string())),
        );
        state.store.set_auto_select_account(true).unwrap();
        assert_eq!(
            own_console_account(&state, &caller, json!({})),
            Some(Some("project-pin".to_string())),
        );
    }

    #[test]
    fn auto_select_account_falls_back_across_agents_only_for_unnamed_choices() {
        let (state, _dir, caller) = codex_caller_state("auto-select-agent-fallback");
        state.store.set_auto_select_account(true).unwrap();
        state
            .store
            .set_added_agents_if_undecided(&[Agent::Codex, Agent::Grok, Agent::Claude])
            .unwrap();
        state
            .store
            .set_default_account_disabled(Agent::Codex, true)
            .unwrap();
        state
            .store
            .set_account_disabled("codex-work", true)
            .unwrap();
        assert_eq!(
            agent_and_account(&state, &caller, "console-1", "project-1", json!({})),
            (Some(Agent::Grok), Some(None)),
        );
        assert_eq!(
            agent_and_account(
                &state,
                &caller,
                "console-1",
                "project-1",
                json!({"agent": "codex"})
            )
            .0,
            Some(Agent::Codex),
        );
        assert_eq!(
            agent_and_account(
                &state,
                &caller,
                "console-1",
                "project-1",
                json!({"account": "Work"})
            ),
            (Some(Agent::Codex), Some(Some("codex-work".to_string()))),
        );
        // The shared builder also resolves requests made in another console.
        state
            .store
            .set_default_account_disabled(Agent::Claude, true)
            .unwrap();
        assert_eq!(
            agent_and_account(&state, &caller, "console-2", "project-2", json!({})),
            (Some(Agent::Grok), Some(None)),
        );
    }

    #[test]
    fn auto_select_account_fallback_to_callers_agent_keeps_its_usable_account() {
        let (state, _dir, mut caller) = codex_caller_state("auto-select-caller-fallback");
        caller.role = Role::Console;
        caller.project_id = None;
        state.store.set_auto_select_account(true).unwrap();
        state
            .store
            .set_added_agents_if_undecided(&[Agent::Claude, Agent::Codex])
            .unwrap();
        state
            .store
            .set_default_account_disabled(Agent::Claude, true)
            .unwrap();
        assert_eq!(
            agent_and_account(&state, &caller, "console-1", "project-1", json!({})),
            (Some(Agent::Codex), Some(Some("codex-work".to_string()))),
        );
        assert_eq!(
            agent_and_account(&state, &caller, "console-2", "project-2", json!({})),
            (Some(Agent::Codex), Some(None)),
        );
    }

    /// With the setting off, a left-out account stays the caller's even when another account's
    /// weekly quota resets sooner.
    #[test]
    fn auto_select_account_off_keeps_the_callers_account() {
        let (state, _dir, caller) = codex_caller_state("auto-select-off");
        let soon = now_millis() + 60_000;
        insert_codex(&state, "sooner", "/tmp/codex-sooner");
        hold(
            &state,
            Agent::Codex,
            Some("sooner"),
            Some("/tmp/codex-sooner"),
            vec![weekly(80.0, soon)],
        );
        hold(
            &state,
            Agent::Codex,
            Some("codex-work"),
            Some("/tmp/codex-work"),
            vec![weekly(10.0, soon + 3_600_000)],
        );
        assert_eq!(
            own_console_account(&state, &caller, json!({})),
            Some(Some("codex-work".to_string()))
        );
    }

    /// With the setting on, the account whose weekly quota resets soonest is chosen, including
    /// when that account is the default.
    #[test]
    fn auto_select_account_on_chooses_the_soonest_weekly_reset_including_the_default() {
        let soon = now_millis() + 60_000;
        let later = soon + 3_600_000;

        let (state, _dir, caller) = codex_caller_state("auto-select-soonest-stored");
        state.store.set_auto_select_account(true).unwrap();
        insert_codex(&state, "sooner", "/tmp/codex-sooner");
        hold(&state, Agent::Codex, None, None, vec![weekly(10.0, later)]);
        hold(
            &state,
            Agent::Codex,
            Some("sooner"),
            Some("/tmp/codex-sooner"),
            vec![weekly(80.0, soon)],
        );
        assert_eq!(
            own_console_account(&state, &caller, json!({})),
            Some(Some("sooner".to_string()))
        );

        let (state, _dir, caller) = codex_caller_state("auto-select-soonest-default");
        state.store.set_auto_select_account(true).unwrap();
        hold(&state, Agent::Codex, None, None, vec![weekly(80.0, soon)]);
        hold(
            &state,
            Agent::Codex,
            Some("codex-work"),
            Some("/tmp/codex-work"),
            vec![weekly(10.0, later)],
        );
        assert_eq!(own_console_account(&state, &caller, json!({})), Some(None));
    }

    /// A named account is the one the call asked for, whatever resets sooner.
    #[test]
    fn auto_select_account_on_a_named_account_still_wins() {
        let (state, _dir, caller) = codex_caller_state("auto-select-named");
        state.store.set_auto_select_account(true).unwrap();
        let soon = now_millis() + 60_000;
        insert_codex(&state, "sooner", "/tmp/codex-sooner");
        hold(
            &state,
            Agent::Codex,
            Some("sooner"),
            Some("/tmp/codex-sooner"),
            vec![weekly(10.0, soon)],
        );
        assert_eq!(
            own_console_account(&state, &caller, json!({ "account": "Work" })),
            Some(Some("codex-work".to_string()))
        );
    }

    /// Nothing rankable leaves today's resolution: the caller's account, in the caller's own
    /// console.
    #[test]
    fn auto_select_account_on_without_a_rankable_weekly_window_keeps_the_callers_account() {
        let soon = now_millis() + 60_000;
        let cases: &[(&str, Option<Vec<QuotaWindow>>)] = &[
            ("unread", None),
            (
                "session",
                Some(vec![QuotaWindow {
                    kind: QuotaWindowKind::Session,
                    label: None,
                    used_percent: 10.0,
                    resets_at: Some(soon),
                    window_minutes: Some(5 * 60),
                }]),
            ),
        ];
        for (label, windows) in cases {
            let (state, _dir, caller) = codex_caller_state(&format!("auto-select-none-{label}"));
            state.store.set_auto_select_account(true).unwrap();
            insert_codex(&state, "other", "/tmp/codex-other");
            if let Some(windows) = windows {
                hold(
                    &state,
                    Agent::Codex,
                    Some("other"),
                    Some("/tmp/codex-other"),
                    windows.clone(),
                );
            }
            assert_eq!(
                own_console_account(&state, &caller, json!({})),
                Some(Some("codex-work".to_string())),
                "{label}"
            );
        }
    }

    /// Nothing rankable still inherits when the session is not the caller's agent, and when the
    /// caller's own account can no longer open a session. It does not keep the caller's account.
    #[test]
    fn auto_select_account_on_without_a_rankable_window_inherits_otherwise() {
        let (state, _dir, caller) = codex_caller_state("auto-select-other-agent");
        state.store.set_auto_select_account(true).unwrap();
        assert_eq!(
            own_console_account(&state, &caller, json!({ "agent": "claude" })),
            Some(None)
        );

        let (state, _dir, caller) = codex_caller_state("auto-select-caller-used-up");
        state.store.set_auto_select_account(true).unwrap();
        hold(
            &state,
            Agent::Codex,
            Some("codex-work"),
            Some("/tmp/codex-work"),
            vec![QuotaWindow {
                kind: QuotaWindowKind::Session,
                label: None,
                used_percent: 100.0,
                resets_at: Some(now_millis() + 60_000),
                window_minutes: Some(5 * 60),
            }],
        );
        assert_eq!(own_console_account(&state, &caller, json!({})), Some(None));
    }

    /// Another console still takes a rankable account, and leaves the account unset when none
    /// ranks — not the caller's.
    #[test]
    fn auto_select_account_on_in_another_console_picks_or_inherits_the_account() {
        let soon = now_millis() + 60_000;
        let (state, _dir, caller) = codex_caller_state("auto-select-other-console");
        state.store.set_auto_select_account(true).unwrap();
        state
            .store
            .insert_account(&Account {
                id: "claude-soon".to_string(),
                agent: Agent::Claude,
                name: "Soon".to_string(),
                config_dir: "/tmp/claude-soon".to_string(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
            })
            .unwrap();
        hold(
            &state,
            Agent::Claude,
            Some("claude-soon"),
            Some("/tmp/claude-soon"),
            vec![weekly(20.0, soon)],
        );
        assert_eq!(
            agent_and_account(&state, &caller, "console-2", "project-2", json!({})),
            (Some(Agent::Claude), Some(Some("claude-soon".to_string())))
        );

        let (state, _dir, caller) = codex_caller_state("auto-select-other-console-none");
        state.store.set_auto_select_account(true).unwrap();
        hold(
            &state,
            Agent::Codex,
            Some("codex-work"),
            Some("/tmp/codex-work"),
            vec![weekly(10.0, soon)],
        );
        assert_eq!(
            agent_and_account(&state, &caller, "console-2", "project-2", json!({})),
            (Some(Agent::Claude), Some(None))
        );
    }

    /// A reading stored under a directory the account no longer points at does not choose it.
    #[test]
    fn auto_select_account_ignores_a_reading_from_another_config_dir() {
        let (state, _dir, caller) = codex_caller_state("auto-select-other-dir");
        state.store.set_auto_select_account(true).unwrap();
        let soon = now_millis() + 60_000;
        insert_codex(&state, "sooner", "/tmp/codex-sooner");
        insert_codex(&state, "later", "/tmp/codex-later");
        hold(
            &state,
            Agent::Codex,
            Some("sooner"),
            Some("/tmp/codex-elsewhere"),
            vec![weekly(10.0, soon)],
        );
        hold(
            &state,
            Agent::Codex,
            Some("later"),
            Some("/tmp/codex-later"),
            vec![weekly(80.0, soon + 3_600_000)],
        );
        assert_eq!(
            own_console_account(&state, &caller, json!({})),
            Some(Some("later".to_string()))
        );
    }

    /// A used-up 5-hour window keeps the account out, however soon its weekly window resets.
    #[test]
    fn auto_select_account_skips_an_account_whose_five_hour_window_is_used_up() {
        let (state, _dir, caller) = codex_caller_state("auto-select-five-hour");
        state.store.set_auto_select_account(true).unwrap();
        let soon = now_millis() + 60_000;
        insert_codex(&state, "sooner", "/tmp/codex-sooner");
        insert_codex(&state, "later", "/tmp/codex-later");
        hold(
            &state,
            Agent::Codex,
            Some("sooner"),
            Some("/tmp/codex-sooner"),
            vec![
                QuotaWindow {
                    kind: QuotaWindowKind::Session,
                    label: None,
                    used_percent: 100.0,
                    resets_at: Some(soon + 3_600_000),
                    window_minutes: None,
                },
                weekly(10.0, soon),
            ],
        );
        hold(
            &state,
            Agent::Codex,
            Some("later"),
            Some("/tmp/codex-later"),
            vec![weekly(80.0, soon + 3_600_000)],
        );
        assert_eq!(
            own_console_account(&state, &caller, json!({})),
            Some(Some("later".to_string()))
        );
    }

    /// An account whose sharing change is running would refuse the launch, so the pick passes
    /// over it for the next one.
    #[test]
    fn auto_select_account_skips_an_account_whose_sharing_change_is_running() {
        let (state, _dir, caller) = codex_caller_state("auto-select-sharing-locked");
        state.store.set_auto_select_account(true).unwrap();
        let soon = now_millis() + 60_000;
        insert_codex(&state, "sooner", "/tmp/codex-sooner");
        hold(
            &state,
            Agent::Codex,
            Some("sooner"),
            Some("/tmp/codex-sooner"),
            vec![weekly(10.0, soon)],
        );
        hold(
            &state,
            Agent::Codex,
            Some("codex-work"),
            Some("/tmp/codex-work"),
            vec![weekly(80.0, soon + 3_600_000)],
        );
        let account = state.store.get_account("sooner").unwrap().unwrap();
        let (_, lock) = state.lock_account_sharing(&account, |_, _| Ok(())).unwrap();
        assert_eq!(
            own_console_account(&state, &caller, json!({})),
            Some(Some("codex-work".to_string()))
        );
        drop(lock);
        assert_eq!(
            own_console_account(&state, &caller, json!({})),
            Some(Some("sooner".to_string()))
        );
    }

    /// The same reset time prefers more remaining, and a tie keeps the earlier candidate.
    #[test]
    fn auto_select_account_breaks_a_reset_tie_by_remaining_then_by_candidate_order() {
        let reset = now_millis() + 60_000;

        let (state, _dir, caller) = codex_caller_state("auto-select-more-remaining");
        state.store.set_auto_select_account(true).unwrap();
        hold(&state, Agent::Codex, None, None, vec![weekly(80.0, reset)]);
        hold(
            &state,
            Agent::Codex,
            Some("codex-work"),
            Some("/tmp/codex-work"),
            vec![weekly(10.0, reset)],
        );
        assert_eq!(
            own_console_account(&state, &caller, json!({})),
            Some(Some("codex-work".to_string()))
        );

        let (state, _dir, caller) = codex_caller_state("auto-select-equal-remaining");
        state.store.set_auto_select_account(true).unwrap();
        hold(&state, Agent::Codex, None, None, vec![weekly(40.0, reset)]);
        hold(
            &state,
            Agent::Codex,
            Some("codex-work"),
            Some("/tmp/codex-work"),
            vec![weekly(40.0, reset)],
        );
        assert_eq!(own_console_account(&state, &caller, json!({})), Some(None));
    }

    /// The caller names the session; without a name it is named from the goal's first line.
    #[test]
    fn a_started_session_takes_the_title_given_else_its_goals() {
        let (state, _dir) = shared_console_state("started-session-title");
        let caller = state.session_record("a").unwrap();
        let console = state.store.get_console("console-1").unwrap().unwrap();
        let title = |args: Value| {
            session_in(&state, &caller, &console, None, &arguments(args))
                .unwrap()
                .title
        };
        let brief = json!({ "goal": "Fix the build\nIt fails on CI." });
        assert_eq!(
            title(json!({ "brief": brief, "title": "  修复构建  " })).as_deref(),
            Some("修复构建")
        );
        assert_eq!(
            title(json!({ "brief": brief, "title": " " })).as_deref(),
            Some("Fix the build")
        );
        assert_eq!(
            title(json!({ "brief": brief })).as_deref(),
            Some("Fix the build")
        );
    }

    /// A session reads itself by leaving `session` out, which is how it learns its own id.
    #[tokio::test]
    async fn get_session_without_a_session_reads_the_caller() {
        let fixture = RepoState::new("get-session-self");
        for caller in ["a", "lead", "lead-kid"] {
            let record = call(&fixture.state, caller, "get_session", &Map::new())
                .await
                .unwrap();
            assert_eq!(record["session"], caller);
        }
        // Naming another session is unchanged.
        let record = call(
            &fixture.state,
            "lead",
            "get_session",
            &arguments(json!({ "session": "kid" })),
        )
        .await
        .unwrap();
        assert_eq!(record["session"], "kid");
    }

    #[test]
    fn session_roles_are_derived_independently_of_parent_and_child_count() {
        use crate::protocol::OrchestrationRole::{Coordinator, Worker};

        let (state, _dir) = peer_state("derived-session-role");
        let caller = state.session_record("a").unwrap();
        for (mut session, expected, name) in [
            (console_session("a"), Coordinator, "console"),
            (
                project_session("loose", "console-1", None),
                Coordinator,
                "independent",
            ),
            (
                project_session("of-a", "console-1", Some("a")),
                Worker,
                "worker",
            ),
            (
                Session {
                    lead: true,
                    ..project_session("empty-coordinator", "console-1", Some("a"))
                },
                Coordinator,
                "reporting coordinator without children",
            ),
        ] {
            // A wire value must never become authoritative for a later derivation.
            session.orchestration_role = Some(if expected == Worker {
                Coordinator
            } else {
                Worker
            });
            assert_eq!(session.orchestration_role(), expected, "{name}");
            assert_eq!(session.is_worker(), expected == Worker, "{name}");
            let wire = state.wire_session(session.clone());
            assert_eq!(wire.orchestration_role, Some(expected), "{name}");
            let record =
                describe_session(&state, &session, &caller, &HashMap::new(), &HashMap::new())
                    .unwrap();
            assert_eq!(record["orchestration_role"], json!(expected), "{name}");
            assert_eq!(record["owner"], json!(session.bound_to), "{name}");
            assert_eq!(record["owned_sessions"], 0);
        }
    }

    /// A record carries what an owner needs to tell sessions apart and to decide on a transfer.
    #[tokio::test]
    async fn a_session_record_carries_its_identity_launch_choices_and_owned_count() {
        let fixture = RepoState::new("record-identity");
        let state = &fixture.state;
        state
            .store
            .insert_session(&Session {
                project_id: Some("repo".to_string()),
                agent_session_id: Some("0a1b2c3d-0000-4000-8000-000000000001".to_string()),
                has_conversation: true,
                model: Some(Some("opus".to_string())),
                effort: Some(None),
                lead: true,
                ..project_session("lead-two", "console-1", Some("a"))
            })
            .unwrap();
        // One live and one archived session under it: both count.
        for (id, status) in [
            ("under-live", SessionStatus::Idle),
            ("under-archived", SessionStatus::Archived),
        ] {
            let mut under = project_session(id, "console-1", Some("lead-two"));
            under.status = status;
            state.store.insert_session(&under).unwrap();
        }
        let record = call(
            state,
            "a",
            "get_session",
            &arguments(json!({ "session": "lead-two" })),
        )
        .await
        .unwrap();
        assert_eq!(record["owned_sessions"], 2);
        assert_eq!(
            record["agent_session_id"],
            "0a1b2c3d-0000-4000-8000-000000000001"
        );
        assert_eq!(record["origin"], "user");
        assert_eq!(record["has_conversation"], true);
        assert_eq!(record["model"], "opus");
        // An explicit choice of the agent's own default is a `null`, not an absent key.
        assert!(record.get("effort").is_some_and(Value::is_null));
        assert_eq!(record["lead"], true);

        // A session that inherits its launch settings has neither key, and owns nothing.
        let plain = call(
            state,
            "a",
            "get_session",
            &arguments(json!({ "session": "kid" })),
        )
        .await
        .unwrap();
        assert!(plain.get("model").is_none() && plain.get("effort").is_none());
        assert_eq!(plain["owned_sessions"], 0);
        assert_eq!(plain["lead"], false);
        assert_eq!(plain["agent_session_id"], Value::Null);
        assert_eq!(plain["display_title"], plain["title"]);
    }

    /// `list_archived` pages with `before`, and `yours` narrows it as it does `list_sessions`.
    #[tokio::test]
    async fn the_archive_is_paged_with_before_and_narrowed_to_yours() {
        let (state, _dir) = shared_console_state("archive-before");
        for (id, ended) in [("old", 10), ("mid", 20), ("new", 30), ("tie", 30)] {
            let mut archived = project_session(id, "console-1", Some("a"));
            archived.status = SessionStatus::Archived;
            archived.ended_at = Some(ended);
            state.store.insert_session(&archived).unwrap();
        }
        let list = |filters: Value| {
            let state = state.clone();
            async move {
                call(&state, "a", "list_archived", &arguments(filters))
                    .await
                    .unwrap()
            }
        };
        let ids = |listed: &Value| -> Vec<String> {
            listed["sessions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|session| session["session"].as_str().unwrap().to_string())
                .collect()
        };
        // Newest first, the id breaking the tie; `archived-of-a` has no end time and comes last.
        let first = list(json!({ "owner": "a", "limit": 2 })).await;
        assert_eq!(ids(&first), ["new", "tie"]);
        assert_eq!(
            (first["total"].clone(), first["older"].clone()),
            (json!(5), json!(3))
        );
        // The cursor is a position, so the session it came from may leave the archive between
        // pages without moving the next one.
        let mut reopened = state.store.get_session("tie").unwrap().unwrap();
        reopened.status = SessionStatus::Idle;
        state.store.update_session(&reopened).unwrap();
        let second =
            list(json!({ "owner": "a", "limit": 2, "before": first["next"].clone() })).await;
        assert_eq!(ids(&second), ["mid", "old"]);
        assert_eq!(second["older"], 1);
        let third =
            list(json!({ "owner": "a", "limit": 2, "before": second["next"].clone() })).await;
        assert_eq!(ids(&third), ["archived-of-a"]);
        assert_eq!(third["older"], 0);
        assert_eq!(third["next"], Value::Null);

        let yours = ids(&list(json!({ "yours": true })).await);
        assert!(
            yours.contains(&"mid".to_string()) && !yours.contains(&"archived-of-b".to_string())
        );
        let others = ids(&list(json!({ "yours": false })).await);
        assert!(
            others.contains(&"archived-of-b".to_string()) && !others.contains(&"mid".to_string())
        );

        let err = call(
            &state,
            "a",
            "list_archived",
            &arguments(json!({ "before": "tie" })),
        )
        .await
        .expect_err("a session id is not a cursor");
        assert!(err.to_string().contains("is not a cursor"), "{err}");
    }

    /// A console session reads one project with `project`; `refresh` checks the git status first
    /// and says it finished, and is refused for another console, whose entries have no status.
    #[tokio::test]
    async fn list_projects_names_one_project_and_refreshes_its_git_status() {
        let fixture = RepoState::new("list-projects-refresh");
        let state = &fixture.state;
        let listed = call(
            state,
            "a",
            "list_projects",
            &arguments(json!({ "project": "repo" })),
        )
        .await
        .unwrap();
        let projects = listed["projects"].as_array().unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0]["project"], "repo");
        assert!(listed.get("git_refresh").is_none());

        let refreshed = call(
            state,
            "a",
            "list_projects",
            &arguments(json!({ "project": "repo", "refresh": true })),
        )
        .await
        .unwrap();
        assert_eq!(refreshed["git_refresh"], json!({ "finished": true }));
        let status = &refreshed["projects"][0]["git_status"];
        assert_eq!(status["repository"], true);
        assert!(status["checked_seconds_ago"].is_u64(), "{status}");

        // A project of another console is not among this one's.
        let err = call(
            state,
            "a",
            "list_projects",
            &arguments(json!({ "project": "project-2" })),
        )
        .await
        .expect_err("another console's project");
        assert!(err.to_string().contains("has no project"), "{err}");
        // Named beside its own console it is found, without the settings.
        let other = call(
            state,
            "a",
            "list_projects",
            &arguments(json!({ "console": "console-2", "project": "project-2" })),
        )
        .await
        .unwrap();
        assert_eq!(other["projects"][0]["project"], "project-2");
        let err = call(
            state,
            "a",
            "list_projects",
            &arguments(json!({ "console": "console-2", "refresh": true })),
        )
        .await
        .expect_err("no status to refresh");
        assert!(err.to_string().contains("`refresh`"), "{err}");
    }

    /// `get_project` with `refresh` checks the project's git status before answering.
    #[tokio::test]
    async fn get_project_can_refresh_the_git_status_first() {
        let fixture = RepoState::new("get-project-refresh");
        let plain = call(&fixture.state, "lead", "get_project", &Map::new())
            .await
            .unwrap();
        assert!(plain.get("git_refresh").is_none());
        let refreshed = call(
            &fixture.state,
            "lead",
            "get_project",
            &arguments(json!({ "refresh": true })),
        )
        .await
        .unwrap();
        assert_eq!(refreshed["git_refresh"], json!({ "finished": true }));
        assert!(refreshed["git_status"].is_object(), "{refreshed}");
    }

    /// `new_session` says which account the session opened on.
    #[tokio::test]
    async fn new_session_returns_the_account_it_opened_on() {
        use crate::test_support::{stand_in_launch, Launched, StandIn};
        use std::sync::Mutex;

        let (state, _dir) = shared_console_state("new-session-account");
        state
            .store
            .insert_account(&Account {
                id: "claude-b".to_string(),
                agent: Agent::Claude,
                name: "Second".to_string(),
                config_dir: "/tmp/octoboard-new-session-account".to_string(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
            })
            .unwrap();
        let caller = state.session_record("a").unwrap();
        let launches = Arc::new(Mutex::new(Vec::<Launched>::new()));
        let started = Arc::new(Mutex::new(Vec::<StandIn>::new()));
        for (input, account, name) in [
            (json!({}), "default", crate::protocol::DEFAULT_ACCOUNT_NAME),
            (json!({ "account": "Second" }), "claude-b", "Second"),
        ] {
            let mut input = input;
            input["project"] = json!("project-1");
            input["brief"] = json!({ "goal": "x" });
            let answer = new_session(
                &state,
                &caller,
                &arguments(input),
                stand_in_launch(&started, &launches, || {}),
            )
            .await
            .unwrap();
            assert_eq!(answer["account"], account);
            assert_eq!(answer["account_name"], name);
        }
    }

    /// `send_message` is offered as the way to tell a session that is not the caller's only while
    /// it can take one: not for an archived session, which it refuses.
    #[test]
    fn the_refusal_to_act_on_a_session_offers_send_message_only_while_it_can_take_it() {
        let (state, _dir) = shared_console_state("refusal-hint");
        let caller = state.session_record("a").unwrap();
        let refusal = |id: &str| {
            resolve_owned_session(&state, &caller, id)
                .expect_err("not the caller's")
                .to_string()
        };
        let mut archived_loose = state.session_record("loose").unwrap();
        archived_loose.id = "archived-loose".to_string();
        archived_loose.status = SessionStatus::Archived;
        state.store.insert_session(&archived_loose).unwrap();

        for (id, offered) in [
            ("of-b", true),
            ("archived-of-b", false),
            ("loose", true),
            ("archived-loose", false),
        ] {
            let message = refusal(id);
            assert_eq!(message.contains("send_message"), offered, "{id}: {message}");
            assert!(message.contains("not yours to"), "{id}: {message}");
        }
    }

    /// A refresh that is not over when the wait is says so, and names the projects still being
    /// checked; one the one-minute floor skips counts as over.
    #[tokio::test]
    async fn a_git_refresh_reports_the_checks_that_were_not_over_in_time() {
        use crate::mcp::settings::refresh_git_status_within;
        use std::time::Duration;

        let fixture = RepoState::new("git-refresh-unfinished");
        let state = &fixture.state;
        let project = state.store.get_project("repo").unwrap().unwrap();

        let late = refresh_git_status_within(state, vec![project.clone()], Duration::ZERO).await;
        assert_eq!(late["finished"], false);
        assert_eq!(late["unfinished"], json!(["repo"]));
        assert!(
            late["note"].as_str().unwrap().contains("had not finished"),
            "{late}"
        );

        // A check completed within the floor is not started again, and not waited for.
        state.record_git_check_completed("repo");
        let skipped = refresh_git_status_within(state, vec![project], Duration::ZERO).await;
        assert_eq!(skipped, json!({ "finished": true }));
    }

    /// `reopen_session` answers with the account, model and effort the relaunch used.
    #[tokio::test]
    async fn reopen_session_returns_the_account_model_and_effort_it_launched_with() {
        use crate::test_support::{stand_in_launch, Launched, StandIn};
        use std::sync::Mutex;

        let fixture = RepoState::new("reopen-returns-launch");
        let state = &fixture.state;
        let mut kid = state.store.get_session("kid").unwrap().unwrap();
        kid.status = SessionStatus::Archived;
        state.store.update_session(&kid).unwrap();
        state
            .set_session_launch_overrides("kid", &Some(Some("sonnet".to_string())), &Some(None))
            .unwrap();
        let caller = state.session_record("a").unwrap();
        let launches = Arc::new(Mutex::new(Vec::<Launched>::new()));
        let started = Arc::new(Mutex::new(Vec::<StandIn>::new()));

        let answer = reopen_session(
            state,
            &caller,
            &arguments(json!({ "session": "kid" })),
            || Ok(HashMap::new()),
            {
                let (started, launches) = (started.clone(), launches.clone());
                move |request| stand_in_launch(&started, &launches, || {})(request)
            },
        )
        .await
        .unwrap();

        assert_eq!(launches.lock().unwrap().len(), 1);
        assert_eq!(answer["session"], "kid");
        assert_eq!(answer["account"], "default");
        assert_eq!(
            answer["account_name"],
            crate::protocol::DEFAULT_ACCOUNT_NAME
        );
        assert_eq!(answer["model"], "sonnet");
        assert_eq!(answer["effort"], Value::Null);
    }

    /// While accounts are selected automatically, `new_session` names the account that was
    /// picked, not the caller's, and the model and effort it launched with.
    #[tokio::test]
    async fn new_session_returns_the_account_automatic_selection_picked() {
        use crate::protocol::{AccountQuota, QuotaWindow, QuotaWindowKind};
        use crate::test_support::{stand_in_launch, Launched, StandIn};
        use std::sync::Mutex;

        let (state, dir) = shared_console_state("new-session-picked-account");
        let config_dir = dir.join("claude-b").to_string_lossy().into_owned();
        state
            .store
            .insert_account(&Account {
                id: "claude-b".to_string(),
                agent: Agent::Claude,
                name: "Second".to_string(),
                config_dir: config_dir.clone(),
                share_default_config: false,
                share_default_config_incomplete: false,
                disabled: false,
            })
            .unwrap();
        state.store.set_auto_select_account(true).unwrap();
        let mut project = state.store.get_project("project-1").unwrap().unwrap();
        project.claude_session_model = Some("sonnet".to_string());
        project.claude_session_effort = Some("high".to_string());
        state.store.update_project(&project).unwrap();
        let hold = |account: Option<&str>, used_percent: f64| {
            let quota = AccountQuota {
                agent: Agent::Claude,
                account: account.map(str::to_string),
                fetched_at: 1,
                plan: None,
                windows: vec![QuotaWindow {
                    kind: QuotaWindowKind::Weekly,
                    label: None,
                    used_percent,
                    resets_at: Some(crate::protocol::now_millis() + 86_400_000),
                    window_minutes: None,
                }],
                unavailable: None,
            };
            match account {
                None => state.account_quotas.hold_for_test(&quota),
                Some(_) => state
                    .account_quotas
                    .hold_in_dir_for_test(&quota, &config_dir),
            }
        };
        // The caller's own account, the default one, is used up.
        hold(None, 100.0);
        hold(Some("claude-b"), 20.0);
        let caller = state.session_record("a").unwrap();
        let launches = Arc::new(Mutex::new(Vec::<Launched>::new()));
        let started = Arc::new(Mutex::new(Vec::<StandIn>::new()));

        let answer = new_session(
            &state,
            &caller,
            &arguments(json!({ "project": "project-1", "brief": { "goal": "x" } })),
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .unwrap();

        assert_eq!(answer["account"], "claude-b");
        assert_eq!(answer["account_name"], "Second");
        assert_eq!(answer["model"], "sonnet");
        assert_eq!(answer["effort"], "high");
    }

    #[tokio::test]
    async fn new_session_automatic_agent_fallback_inherits_the_final_agents_launch_settings() {
        use crate::test_support::{stand_in_launch, Launched, StandIn};
        use std::sync::Mutex;

        let (state, _dir) = shared_console_state("new-session-agent-fallback-launch");
        state.store.set_auto_select_account(true).unwrap();
        state
            .store
            .set_default_account_disabled(Agent::Claude, true)
            .unwrap();
        crate::test_support::offer_models(&state, Agent::Codex, &[("fallback-model", &["high"])]);
        let mut project = state.store.get_project("project-1").unwrap().unwrap();
        project.claude_session_model = Some("sonnet".to_string());
        project.codex_session_model = Some("fallback-model".to_string());
        project.codex_session_effort = Some("high".to_string());
        state.store.update_project(&project).unwrap();
        let caller = state.session_record("a").unwrap();
        let launches = Arc::new(Mutex::new(Vec::<Launched>::new()));
        let started = Arc::new(Mutex::new(Vec::<StandIn>::new()));
        let answer = new_session(
            &state,
            &caller,
            &arguments(json!({"project": "project-1", "brief": {"goal": "x"}})),
            stand_in_launch(&started, &launches, || {}),
        )
        .await
        .unwrap();
        assert_eq!(answer["agent"], "codex");
        assert_eq!(answer["account"], "default");
        assert_eq!(answer["model"], "fallback-model");
        assert_eq!(answer["effort"], "high");
        let session = state
            .session_record(answer["session"].as_str().unwrap())
            .unwrap();
        assert_eq!(session.agent, Agent::Codex);
    }

    /// `archive_session` with `keep` removes no branch or worktree, where without it a merged
    /// branch goes with its session.
    #[tokio::test]
    async fn archive_session_can_keep_the_branches() {
        let fixture = RepoState::new("archive-keep");
        let exists = |branch: &str| {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(fixture.repo.to_string_lossy().as_ref())
                .args(["branch", "--list", branch])
                .output()
                .unwrap();
            !String::from_utf8_lossy(&out.stdout).trim().is_empty()
        };
        fixture.branch("kept", "kid");
        fixture.branch("gone", "lead-kid");
        let archived = call(
            &fixture.state,
            "a",
            "archive_session",
            &arguments(json!({ "session": "kid", "keep": true })),
        )
        .await
        .unwrap();
        assert_eq!(archived["branches_kept"], true);
        assert!(archived["note"]
            .as_str()
            .unwrap()
            .contains("handover_branches"));
        assert!(exists("kept"));

        let archived = call(
            &fixture.state,
            "lead",
            "archive_session",
            &arguments(json!({ "session": "lead-kid" })),
        )
        .await
        .unwrap();
        assert!(archived.get("branches_kept").is_none());
        assert!(!exists("gone"));
    }

    /// `keep` on a lead session leaves its branches and those of the sessions under it, which
    /// are archived with it, and the note says which are not the caller's to name as `from`.
    #[tokio::test]
    async fn keeping_the_branches_of_an_archived_lead_names_those_of_its_sessions() {
        let fixture = RepoState::new("archive-keep-lead");
        for (id, owner) in [("mid", "a"), ("mid-kid", "mid"), ("mid-kid-2", "mid")] {
            fixture
                .state
                .store
                .insert_session(&Session {
                    project_id: Some("repo".to_string()),
                    lead: id == "mid",
                    ..project_session(id, "console-1", Some(owner))
                })
                .unwrap();
        }
        fixture.branch("mid-work", "mid");
        fixture.branch("kid-work", "mid-kid");
        fixture.branch("kid-2-work", "mid-kid-2");

        let archived = call(
            &fixture.state,
            "a",
            "archive_session",
            &arguments(json!({ "session": "mid", "keep": true })),
        )
        .await
        .unwrap();

        assert_eq!(archived["branches_kept"], true);
        let note = archived["note"].as_str().unwrap();
        assert!(
            note.contains("`handover_branches` with `from` `mid`"),
            "{note}"
        );
        assert!(note.contains("come back with those sessions"), "{note}");
        assert!(
            note.contains("`reopen_session` `mid` with a `text`")
                && note.contains("from each of `mid-kid`, `mid-kid-2` to itself"),
            "{note}"
        );
        assert!(
            note.contains("`mid-kid` (kid-work)") && note.contains("`mid-kid-2` (kid-2-work)"),
            "{note}"
        );
        assert!(note.contains("not yours to name as `from`"), "{note}");
        assert!(!note.contains("`mid` (mid-work)"), "{note}");
        for id in ["mid", "mid-kid", "mid-kid-2"] {
            assert_eq!(
                fixture.state.session_record(id).unwrap().status,
                SessionStatus::Archived
            );
        }
    }
}
