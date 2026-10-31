//! What `probe_path_links` reports for each path it is asked about.

use serde::Serialize;

/// What a path asked about in `probe_path_links` names.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PathLinkKind {
    File,
    Directory,
    None,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Serialize)]
pub struct PathLink {
    /// The path as asked.
    pub path: String,
    pub kind: PathLinkKind,
    /// Where the client reads the file with `read_project_file`, for a file inside the project or
    /// one of its worktrees, or for a directory there, its README or index file when it has one;
    /// absent for anything else. Whether the viewer can show it is for the client to find out by
    /// reading it.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub view: Option<PathLinkView>,
    /// A file the system could launch: it is for the viewer to show, and `open_path_link` refuses it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(test, ts(optional = nullable))]
    pub viewer_only: bool,
}

/// A file in the project, as `read_project_file` names it: the file a path link names, or the one
/// a directory link shows.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PathLinkView {
    /// The worktree it is in; absent for the project's own directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub worktree: Option<String>,
    /// Its wire path below that directory.
    pub path: String,
}
