//! Starting and steering sessions: `new_session`, `request_session`, the rename, pin, reopen and
//! interrupt tools, and the choices a session started on a caller's behalf is made from.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use crate::branch_origin::handover::{branch_name, move_branches, Handed, Handover};
use crate::coordinator::{self, launch::LaunchEdit, open::OpenRequest};
use crate::mcp::args::{
    find_console, find_project, optional_agent, optional_string, optional_string_list, own_console,
    own_project, present_text, raw_text, required_str, resolve_project,
};
use crate::mcp::handover::{branch_names, held_by};
use crate::mcp::settings::resolve_account;
use crate::messaging::texts::{self, branches_opening_prompt, request_opening_prompt};
use crate::protocol::{Agent, Console, Origin, Project, Role, Session};
use crate::session::term::{Launch, LaunchRequest};
use crate::session_request;
use crate::state::AppState;

use super::resolve::{branch_names_of, describe_session, owned_count_of, resolve_owned_session};

/// Starts the session through `launch`: `term::launch` outside the tests, which cannot start an
/// agent.
pub(super) async fn new_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
    launch: impl FnOnce(crate::session::term::LaunchRequest) -> Result<crate::session::term::Launch>
        + Send
        + 'static,
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
    let session = coordinator::open::open_session_with(
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
    let (model, effort) = coordinator::launch::launch_settings(state, &session)?;
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
    let task = texts::render_brief(
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
    // Read once: the agent and the account are both settled on it.
    let automatic = state.store.auto_select_account()?;
    let mut agent = match chosen_agent {
        Some(agent) => Some(agent),
        None if follows_callers_agent
            && (automatic || coordinator::account_choice::agent_can_open(state, caller.agent)?) =>
        {
            Some(caller.agent)
        }
        None => None,
    };
    // An account is one agent's, so it is read for the agent the session will be of, as
    // `coordinator::account_choice::session_agent` settles it.
    let session_agent =
        coordinator::account_choice::session_agent(agent, project.as_ref(), console);
    let account = match optional_string(arguments, "account") {
        Some(wanted) => Some(resolve_account(state, session_agent, &wanted)?),
        None if automatic => {
            match automatic_choice(
                state,
                caller,
                session_agent,
                console,
                project.as_ref(),
                chosen_agent.is_none(),
            )? {
                Some((picked_agent, picked_account)) => {
                    agent = Some(picked_agent);
                    Some(picked_account)
                }
                None => None,
            }
        }
        None if in_own_console => callers_account(state, caller, session_agent)?,
        // Inherit the project, else the console.
        None => None,
    };
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
            optional_string(arguments, "title").unwrap_or_else(|| texts::title_from_goal(goal)),
        ),
        origin: Origin::Console,
        bound_to: None,
        resume_agent_session_id: None,
        session_id: None,
        terminal_appearance: None,
    })
}

/// The agent and account of a session `caller` starts, or hands one off to, in `project` of
/// `console` (none for a console session), when the call names no account and automatic account
/// selection is on: what [`coordinator::account_choice::automatic_open_choice`] picks, as for a
/// session the user opens without choosing one, of `agent` alone unless `other_agents`. `caller`'s
/// own account (`callers_account`) is what the session inherits in `caller`'s own console. `None`
/// when nothing is picked: the session then takes the project's account for its agent, else the
/// console's.
pub(in crate::mcp) fn automatic_choice(
    state: &Arc<AppState>,
    caller: &Session,
    agent: Agent,
    console: &Console,
    project: Option<&Project>,
    other_agents: bool,
) -> Result<Option<(Agent, Option<String>)>> {
    let inherited = if caller.console_id == console.id {
        callers_account(state, caller, caller.agent)?.map(|account| (caller.agent, account))
    } else {
        None
    };
    coordinator::account_choice::automatic_open_choice(
        state,
        agent,
        console,
        project,
        inherited,
        other_agents,
    )
}

/// The account a session of `agent` that `caller` starts runs on when the call names none:
/// `caller`'s own, for a session of `caller`'s own agent, so that work a session hands out goes on
/// in the same agent and account unless the user asked otherwise. `None` — the project's account
/// for the agent, else the console's — for another agent, and when `caller`'s account could not
/// open a session now: removed since, turned off, logged out, or with its quota used up.
pub(in crate::mcp) fn callers_account(
    state: &Arc<AppState>,
    caller: &Session,
    agent: Agent,
) -> Result<Option<Option<String>>> {
    if agent != caller.agent
        || !coordinator::account_choice::account_can_open(
            state,
            agent,
            caller.account_id.as_deref(),
        )?
    {
        return Ok(None);
    }
    Ok(Some(caller.account_id.clone()))
}

/// Stops the turn a session of the caller's is running, through the same function the control
/// request runs.
pub(super) async fn interrupt_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let target = resolve_owned_session(state, caller, required_str(arguments, "session")?)?;
    let crate::session::interrupt::Interrupted {
        interrupted,
        status,
        note,
    } = crate::session::interrupt::interrupt_session(state, &target.id).await?;
    Ok(json!({ "interrupted": interrupted, "status": status, "note": note }))
}

pub(super) async fn rename_session(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let target = resolve_owned_session(state, caller, required_str(arguments, "session")?)?;
    let title = raw_text(arguments, "title")?.ok_or_else(|| anyhow!("`title` is required"))?;
    let renamed = coordinator::lifecycle::rename_session(state, &target.id, &title)?;
    let branches = branch_names_of(state, vec![renamed.clone()]).await?;
    describe_session(
        state,
        &renamed,
        caller,
        &branches,
        &owned_count_of(state, &renamed)?,
    )
}

pub(super) async fn set_session_pinned(
    state: &Arc<AppState>,
    caller: &Session,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let target = resolve_owned_session(state, caller, required_str(arguments, "session")?)?;
    let pinned = arguments
        .get("pinned")
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow!("`pinned` is required and must be true or false"))?;
    let updated = coordinator::lifecycle::set_session_pinned(state, &target.id, pinned)?;
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
pub(super) async fn reopen_session(
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
    coordinator::resume::resume_session_edited_with(
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
    let (model, effort) = coordinator::launch::launch_settings(state, &reopened)?;
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

/// Asks the user for a session with no owner, and waits for the answer; see
/// `crate::session_request`. Read and checked as `new_session` is, in whichever project or
/// console the call names.
pub(super) async fn request_session(
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

#[cfg(test)]
mod tests;
