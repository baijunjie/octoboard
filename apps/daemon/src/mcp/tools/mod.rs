//! The tools of the Octoboard MCP server, one module per family: the ones that read, the ones that
//! start, message, archive and hand over sessions, the settings and project tools, and the
//! requests to the user; `relaunch` is no family but the result wording the relaunching tools
//! share. Each tool runs against the real state with the caller already resolved by
//! [`super::dispatch`].

pub(super) mod account_switch;
pub(super) mod agents;
pub(super) mod archive;
pub(super) mod bindings;
pub(super) mod handoff;
pub(super) mod handover;
pub(super) mod history;
pub(super) mod messaging;
pub(super) mod models;
pub(super) mod pages;
pub(super) mod project_branches;
pub(super) mod project_browse;
pub(super) mod project_changes;
pub(super) mod reads;
pub(super) mod relaunch;
pub(super) mod resume_request;
pub(super) mod session_branches;
pub(super) mod session_model;
pub(super) mod sessions;
pub(super) mod settings;
