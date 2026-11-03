//! The process trees the daemon lists and counts, as they go on the wire.

use serde::Serialize;

/// A group of one launch's processes as the user sees it: a top process — one whose parent is not
/// the same launch's — and its descendants of that launch.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProcessTree {
    /// The top process, as `<pid>:<start>`: what a stop or a keep names the tree by.
    pub id: String,
    /// The session whose launch the tree belongs to; absent for a tree no session owns: one left
    /// behind, or one the user kept.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub session_id: Option<String>,
    /// The user kept the tree: no session owns it and nothing stops it unless the user asks.
    pub kept: bool,
    /// The top process's arguments, joined; empty when they cannot be read.
    pub command: String,
    pub cwd: Option<String>,
    /// The top process's working directory was removed.
    pub cwd_missing: bool,
    /// When the top process started, in milliseconds since the Unix epoch.
    pub started_at: i64,
    pub process_count: usize,
    /// The resident memory of every process in the tree, summed.
    pub memory_bytes: u64,
    /// The TCP ports the tree's processes listen on, sorted.
    pub ports: Vec<u16>,
}

/// How many trees there are under one project, or for one console session, in the count of
/// every project's and console session's trees. It names a project or a console session, never
/// both.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProcessCount {
    /// The project the trees are listed under; absent for a console session.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub project_id: Option<String>,
    /// The console session whose trees these are; absent for a project.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub session_id: Option<String>,
    /// How many trees, at least one.
    pub trees: usize,
}
