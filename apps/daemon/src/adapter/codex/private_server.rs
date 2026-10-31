//! The private Codex app server's launch plan and the release that supports it.

pub struct PrivateServerPlan {
    pub args: Vec<String>,
    pub tui_args: Vec<String>,
    pub hook_command: String,
    pub resume_thread: Option<String>,
    pub resume_reviewer: Option<crate::protocol::ApprovalReviewer>,
}

pub(super) fn private_server_supported(version: Option<crate::adapter::AgentVersion>) -> bool {
    version == Some((0, 163, 0))
}
