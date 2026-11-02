//! The daemon side of the MCP tools: running one call against the real consoles, projects and
//! sessions.
//!
//! The calling session is resolved from the token the call arrived with, never from an argument,
//! so a child that rewrote its own arguments still cannot act on another session. Which tools it
//! may call follows from that session's role and whether it has the role of a worker,
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

use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use serde_json::{Map, Value};

use crate::session_tools::catalogue::{runs_on_the_call, tool_by_name};
use crate::state::AppState;

use super::tools::archive::{archive_self, archive_session};
use super::tools::messaging::{instruct_session, report, send_message};
use super::tools::sessions::{
    interrupt_session, new_session, rename_session, reopen_session, request_session,
    set_session_pinned,
};
use super::tools::settings::add_project;
use super::tools::{
    account_switch, agents, bindings, handoff, handover, history, models, pages, project_branches,
    project_changes, reads, resume_request, session_branches, session_model, settings,
};

/// Runs one tool call. The `Err` case is what the model is shown as the reason the call failed, so
/// the messages are written for it rather than for a log.
///
/// The call is cancelled by dropping it, as the route serving it does when the caller's connection
/// goes, and with it its future at whichever `.await` it had reached. So every tool but those meant
/// to stop with the call, or that see to their own end ([`runs_on_the_call`]), runs on a
/// task of its own that the drop leaves to finish: a session opened and its branches not yet
/// moved, or a report delivered and its reporter's clean-up not yet started, would otherwise be
/// left half done.
pub(crate) async fn call(
    state: &Arc<AppState>,
    session_id: &str,
    tool: &str,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    if runs_on_the_call(tool) {
        return dispatch(state, session_id, tool, arguments).await;
    }
    let (state, session_id, tool, arguments) = (
        state.clone(),
        session_id.to_string(),
        tool.to_string(),
        arguments.clone(),
    );
    tokio::spawn(async move { dispatch(&state, &session_id, &tool, &arguments).await })
        .await
        .map_err(|err| anyhow!("the call did not finish: {err}"))?
}

async fn dispatch(
    state: &Arc<AppState>,
    session_id: &str,
    tool: &str,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let session = state.session_record(session_id)?;
    if tool_by_name(session.role, session.is_worker(), tool).is_none() {
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
        "new_session" => {
            new_session(state, &session, arguments, crate::session::term::launch).await
        }
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
                crate::session::term::launch,
            )
            .await
        }
        "handover_branches" => handover::handover_branches(state, &session, arguments).await,
        "handover_processes" => handover::handover_processes(state, &session, arguments).await,
        "handoff_session" => {
            handoff::handoff_session(state, &session, arguments, crate::session::term::launch).await
        }
        "switch_session_account" => {
            account_switch::switch_session_account(
                state,
                &session,
                arguments,
                crate::env_shell::launch_snapshot,
                crate::session::term::launch,
            )
            .await
        }
        "set_session_model" => {
            session_model::set_session_model(
                state,
                &session,
                arguments,
                crate::env_shell::launch_snapshot,
                crate::session::term::launch,
            )
            .await
        }
        "show_page" => pages::show_page(state, &session, arguments),
        "list_pages" => pages::list_pages(state, &session, arguments),
        "delete_pages" => pages::delete_pages(state, &session, arguments),
        "report" => report(state, &session, arguments).await,
        "list_sessions" => reads::list_sessions(state, &session, arguments).await,
        "get_session_branches" => {
            session_branches::get_session_branches(state, &session, arguments).await
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
        "list_agents" => agents::list_agents(state, arguments).await,
        "send_message" => send_message(state, &session, arguments).await,
        "transfer_session" => bindings::transfer_session(state, &session, arguments).await,
        "release_session" => bindings::release_session(state, &session, arguments).await,
        "bind_to_session" => bindings::bind_to_session(state, &session, arguments).await,
        "request_session" => request_session(state, &session, arguments).await,
        "request_resume_session" => {
            resume_request::request_resume_session(state, &session, arguments).await
        }
        // Unreachable while the catalogue and this dispatch agree; a tool added to one and not the
        // other should say so rather than look like a refusal.
        _ => bail!("`{tool}` is announced but not implemented"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::test_support::{arguments, lead_state, peer_state, shared_console_state};
    use crate::protocol::{Agent, AgentAvailability, Availability};
    use crate::test_support::{output_containing, running};
    use serde_json::json;

    /// A call whose connection goes while it runs is not stopped part-way: a `report` cut off
    /// while it waits for its session's hook events to be applied still reaches the owner.
    #[tokio::test]
    async fn a_call_cut_off_part_way_still_runs_to_its_end() {
        let (state, _dir) = shared_console_state("cut-off-call");
        let owner = running(&state, "a");
        // An event still being applied holds the report back until it is done.
        state
            .hook_queues
            .push("of-a", bytes::Bytes::from_static(b"{}"));
        let report = arguments(json!({
            "summary": "finished-although-cut-off",
            "status": "needs_decision",
        }));

        let cut_off = tokio::time::timeout(
            std::time::Duration::from_millis(200),
            call(&state, "of-a", "report", &report),
        )
        .await;
        assert!(cut_off.is_err(), "the report waited for the hook event");
        state.hook_queues.complete("of-a");

        let heard = tokio::task::spawn_blocking(move || {
            output_containing(&owner, "finished-although-cut-off")
        })
        .await
        .unwrap();
        assert!(heard.contains("finished-although-cut-off"), "{heard}");
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

    /// A coordinator with a parent keeps the unbound project session's tools, so its `new_session`
    /// gets as far as the launch (an unavailable agent is refused there), binding the new session
    /// to it, and its `bind_to_session` is refused in prose; a session bound to it still has no
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
}
