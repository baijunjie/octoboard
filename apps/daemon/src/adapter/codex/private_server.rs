//! The private Codex app server's launch plan and the release that supports it.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::Result;

use super::transport::CodexTransport;
use crate::session::term::{MessageTransport, PrivateServer};

pub struct PrivateServerPlan {
    pub args: Vec<String>,
    pub tui_args: Vec<String>,
    pub hook_command: String,
    pub resume_thread: Option<String>,
    pub resume_reviewer: Option<super::ApprovalReviewer>,
}

impl PrivateServer for PrivateServerPlan {
    /// Starts the server, and the terminal is then a client of it.
    fn start(
        &self,
        binary: &str,
        environment: &HashMap<String, String>,
        cwd: &Path,
    ) -> Result<(Vec<String>, Arc<dyn MessageTransport>)> {
        let transport = CodexTransport::start(binary, self, environment, cwd)?;
        let mut args = self.tui_args.clone();
        args.extend(["--remote".to_string(), transport.endpoint()]);
        Ok((args, Arc::new(transport)))
    }

    #[cfg(test)]
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub(super) fn private_server_supported(version: Option<crate::adapter::AgentVersion>) -> bool {
    version == Some((0, 163, 0))
}
