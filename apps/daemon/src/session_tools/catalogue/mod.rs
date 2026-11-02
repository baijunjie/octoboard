//! The catalogue of the Octoboard MCP tools. The server that announces the tools, the one that runs
//! a call and the role descriptions ([`super::role`]) that name them all read this one list, so
//! they cannot drift apart. The console session's instruction file is held to it by a test in
//! [`super::console_instructions`], which fails when a tool it names is not in the catalogue. The
//! names an agent actually shows the model are prefixed per agent — `mcp__octoboard__x` on Claude
//! Code and Codex, `octoboard__x` on Grok (see "Across the three agents" in
//! `docs/agent-cli-reference.md`), which is why [`qualified_tool_name`] exists rather than the role
//! text spelling a prefix out.
//!
//! Each tool family's definitions sit in a file of their own; which tools a session is offered,
//! and in what order, is set here.

mod messages;
mod moves;
mod pages;
mod reads;
mod repository;
mod requests;
mod schemas;
mod sessions;
mod settings;
#[cfg(test)]
mod tests;

use serde_json::Value;

use crate::protocol::{Agent, Role};

use messages::{REPORT, SEND_MESSAGE};
use moves::{
    BIND_TO_SESSION, HANDOFF_SESSION, HANDOVER_BRANCHES, HANDOVER_PROCESSES, RELEASE_SESSION,
    SET_SESSION_MODEL, SWITCH_SESSION_ACCOUNT, TRANSFER_SESSION,
};
use pages::{DELETE_PAGES, LIST_PAGES, SHOW_PAGE};
use reads::{
    GET_PROJECT, GET_SESSION, LIST_AGENTS, LIST_AGENT_MODELS, LIST_ARCHIVED, LIST_CONSOLES,
    LIST_PROJECTS, LIST_SESSIONS, READ_SESSION_HISTORY,
};
use repository::{
    GET_SESSION_BRANCHES, LIST_PROJECT_BRANCHES, LIST_PROJECT_CHANGES, READ_PROJECT_DIFF,
};
use requests::{REQUEST_RESUME_SESSION, REQUEST_SESSION};
use sessions::{
    ARCHIVE_SELF, ARCHIVE_SESSION, CONSOLE_NEW_SESSION, INSTRUCT_SESSION, INTERRUPT_SESSION,
    PROJECT_NEW_SESSION, RENAME_SESSION, REOPEN_SESSION, SET_SESSION_PINNED,
};
use settings::{
    ADD_PROJECT, GET_CONSOLE, GET_SETTINGS, UPDATE_CONSOLE, UPDATE_PROJECT, UPDATE_SETTINGS,
};

/// The tools whose call the daemon has to learn the agent cancelled: a session request, or a switch
/// or a handoff the user is asked to approve, waits in front of the user and is withdrawn when its
/// call's connection to the daemon goes. A switch or a handoff that runs without asking finishes
/// on a task of its own, so the connection going leaves it alone.
pub(crate) const WITHDRAWN_ON_CANCEL: &[&str] = &[
    "request_session",
    "request_resume_session",
    "switch_session_account",
    "handoff_session",
];

/// Whether `tool`'s call runs on the task serving its connection, and so stops wherever it is when
/// that connection goes: the calls in [`WITHDRAWN_ON_CANCEL`], and `set_session_model`, which runs
/// its operation on a task of its own and tells its caller the outcome when the call is gone. Every
/// other call runs to its end.
pub(crate) fn runs_on_the_call(tool: &str) -> bool {
    WITHDRAWN_ON_CANCEL.contains(&tool) || tool == "set_session_model"
}

/// The key the adapters register the server under. It must not collide with a server the project
/// defines: on Claude Code a colliding key means the *project's* definition is silently never
/// spawned.
pub const SERVER_KEY: &str = "octoboard";

/// One tool, as both sides of the stdio bridge see it.
#[derive(Clone, Copy)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    /// The tool's JSON Schema, built on demand rather than stored: `rmcp` wants an owned object
    /// and the catalogue is walked once per `tools/list`.
    pub schema: fn() -> Value,
}

/// The tools a session may call, by session kind and orchestration capability. Nothing else is
/// announced to it: workers cannot manage child sessions, and console sessions cannot report.
///
/// `worker` is `Session::is_worker`, not the presence of a parent: a project coordinator
/// reporting to a console still has the coordinator set. The daemon checks the current capability
/// on every call, and the MCP child is told when its catalogue changes.
pub fn tools_for(role: Role, worker: bool) -> &'static [ToolDef] {
    match (role, worker) {
        (Role::Console, _) => CONSOLE_SESSION_TOOLS,
        (Role::Project, true) => WORKER_PROJECT_SESSION_TOOLS,
        (Role::Project, false) => COORDINATOR_PROJECT_SESSION_TOOLS,
    }
}

pub fn tool_by_name(role: Role, worker: bool, name: &str) -> Option<&'static ToolDef> {
    tools_for(role, worker)
        .iter()
        .find(|tool| tool.name == name)
}

/// How the agent presents one of these tools to its model. Only the prefix differs between the
/// agents, but a role description that names the wrong one sends the model looking for a tool it
/// cannot see.
pub fn qualified_tool_name(agent: Agent, tool: &str) -> String {
    match agent {
        Agent::Claude | Agent::Codex => format!("mcp__{SERVER_KEY}__{tool}"),
        Agent::Grok => format!("{SERVER_KEY}__{tool}"),
    }
}

const CONSOLE_SESSION_TOOLS: &[ToolDef] = &[
    LIST_PROJECTS,
    ADD_PROJECT,
    UPDATE_PROJECT,
    GET_CONSOLE,
    UPDATE_CONSOLE,
    GET_SETTINGS,
    UPDATE_SETTINGS,
    CONSOLE_NEW_SESSION,
    INSTRUCT_SESSION,
    INTERRUPT_SESSION,
    GET_SESSION,
    RENAME_SESSION,
    SET_SESSION_PINNED,
    ARCHIVE_SESSION,
    ARCHIVE_SELF,
    LIST_ARCHIVED,
    REOPEN_SESSION,
    HANDOFF_SESSION,
    HANDOVER_BRANCHES,
    HANDOVER_PROCESSES,
    SWITCH_SESSION_ACCOUNT,
    SET_SESSION_MODEL,
    TRANSFER_SESSION,
    RELEASE_SESSION,
    REQUEST_SESSION,
    REQUEST_RESUME_SESSION,
    LIST_SESSIONS,
    GET_SESSION_BRANCHES,
    LIST_PROJECT_CHANGES,
    LIST_PROJECT_BRANCHES,
    READ_PROJECT_DIFF,
    READ_SESSION_HISTORY,
    LIST_AGENT_MODELS,
    LIST_CONSOLES,
    SEND_MESSAGE,
    LIST_AGENTS,
    LIST_PAGES,
    SHOW_PAGE,
    DELETE_PAGES,
];

/// Workers report to a parent and cannot manage child sessions. They keep the shared reads,
/// information sharing, the project-settings read and the session requests the user approves.
const WORKER_PROJECT_SESSION_TOOLS: &[ToolDef] = &[
    REPORT,
    GET_SESSION,
    LIST_SESSIONS,
    GET_SESSION_BRANCHES,
    LIST_PROJECT_CHANGES,
    LIST_PROJECT_BRANCHES,
    READ_PROJECT_DIFF,
    READ_SESSION_HISTORY,
    LIST_AGENT_MODELS,
    LIST_CONSOLES,
    LIST_PROJECTS,
    LIST_ARCHIVED,
    SEND_MESSAGE,
    GET_PROJECT,
    LIST_AGENTS,
    REQUEST_SESSION,
    REQUEST_RESUME_SESSION,
];

/// Project coordinators manage child sessions in their own project and have report-panel tools.
/// Their catalogue stays the same with or without a console parent: `report` refuses independent
/// sessions, and `archive_self` refuses a coordinator with a parent, which finishes with a report.
const COORDINATOR_PROJECT_SESSION_TOOLS: &[ToolDef] = &[
    PROJECT_NEW_SESSION,
    INSTRUCT_SESSION,
    INTERRUPT_SESSION,
    GET_SESSION,
    ARCHIVE_SESSION,
    ARCHIVE_SELF,
    LIST_ARCHIVED,
    REOPEN_SESSION,
    RENAME_SESSION,
    SET_SESSION_PINNED,
    HANDOFF_SESSION,
    HANDOVER_BRANCHES,
    HANDOVER_PROCESSES,
    SWITCH_SESSION_ACCOUNT,
    SET_SESSION_MODEL,
    TRANSFER_SESSION,
    RELEASE_SESSION,
    BIND_TO_SESSION,
    REQUEST_SESSION,
    REQUEST_RESUME_SESSION,
    REPORT,
    LIST_SESSIONS,
    GET_SESSION_BRANCHES,
    LIST_PROJECT_CHANGES,
    LIST_PROJECT_BRANCHES,
    READ_PROJECT_DIFF,
    READ_SESSION_HISTORY,
    LIST_AGENT_MODELS,
    LIST_CONSOLES,
    LIST_PROJECTS,
    SEND_MESSAGE,
    GET_PROJECT,
    LIST_AGENTS,
    LIST_PAGES,
    SHOW_PAGE,
    DELETE_PAGES,
];
