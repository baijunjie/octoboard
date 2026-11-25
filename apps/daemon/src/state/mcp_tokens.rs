//! The per-session tokens the MCP surface is reached with, and whether the server holding a
//! session's token has answered a listing of its tools: what a launch whose agent offers
//! Octoboard's tools only from then on waits for before its first message
//! (`crate::adapter::AgentProvider::awaits_tool_listing`).

use std::sync::Arc;

use super::AppState;
use crate::session::LiveSession;

impl AppState {
    /// Issues the token one session's MCP server authenticates with. A fresh one per launch: the
    /// previous process is gone, and a token that outlived it would let a stale child act on the
    /// session the id was reused for.
    pub fn issue_mcp_token(&self, session_id: &str) -> String {
        let token = uuid::Uuid::new_v4().to_string();
        let mut tokens = self.mcp_tokens.write().expect("mcp token lock poisoned");
        tokens.retain(|_, owner| owner != session_id);
        tokens.insert(token.clone(), session_id.to_string());
        self.forget_tools_listed(session_id);
        token
    }

    /// Puts back the token an adopted launch's MCP server authenticates with, which the daemon that
    /// started the launch issued and its holder kept bound to the launch's key. The launch's
    /// process is the same one, so the token is as good as it was; any other this session had is
    /// retired, as issuing one does.
    pub fn restore_mcp_token(&self, session_id: &str, token: &str) {
        let mut tokens = self.mcp_tokens.write().expect("mcp token lock poisoned");
        tokens.retain(|_, owner| owner != session_id);
        tokens.insert(token.to_string(), session_id.to_string());
        self.forget_tools_listed(session_id);
    }

    /// Retires this session's token without waiting for its process to go away. Used where no
    /// process ever started, so nothing will ever observe the exit that would otherwise retire it.
    pub fn revoke_mcp_tokens(&self, session_id: &str) {
        self.mcp_tokens
            .write()
            .expect("mcp token lock poisoned")
            .retain(|_, owner| owner != session_id);
        self.forget_tools_listed(session_id);
    }

    /// Notes that the MCP server of the session `session_id` answered a listing of its tools
    /// (`octoboardd mcp` says so under the session's token), and has what was held for it
    /// written: from here the agent can offer Octoboard's tools to its model. The listing can
    /// come before the launch is registered, so it is kept with the token, not with the process.
    pub(crate) fn note_mcp_tools_listed(self: &Arc<Self>, session_id: &str) {
        let first = self
            .mcp_tools_listed
            .write()
            .expect("mcp tools lock poisoned")
            .insert(session_id.to_string());
        if !first {
            return;
        }
        let waited = self
            .live_session(session_id)
            .is_some_and(|live| live.tool_listing_wait_open());
        if waited {
            if let Ok(session) = self.session_record(session_id) {
                self.spawn_flush_outbox(session_id, self.agent_status(&session));
            }
        }
    }

    /// Whether `live`, a launch whose provider has its first message wait for the session's MCP
    /// server to have answered a listing of its tools, still waits for it: the wait's bound has
    /// not passed ([`LiveSession::tool_listing_wait_open`]) and no listing has been answered
    /// under the session's token. Nothing is written into the session meanwhile, as while its
    /// startup screens are not past.
    pub(crate) fn awaits_tool_listing(&self, live: &LiveSession) -> bool {
        live.tool_listing_wait_open()
            && !self
                .mcp_tools_listed
                .read()
                .expect("mcp tools lock poisoned")
                .contains(&live.id)
    }

    fn forget_tools_listed(&self, session_id: &str) {
        self.mcp_tools_listed
            .write()
            .expect("mcp tools lock poisoned")
            .remove(session_id);
    }

    /// The session a token belongs to, which is how an MCP call is attributed. The caller never
    /// names its own session: a child that rewrote the argument would otherwise act on another.
    pub fn session_for_mcp_token(&self, token: &str) -> Option<String> {
        self.mcp_tokens
            .read()
            .expect("mcp token lock poisoned")
            .get(token)
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::app_state;

    /// An adopted launch gets its own token back, and with it no other of the session's.
    #[test]
    fn a_restored_token_replaces_the_sessions_others() {
        let (state, _dir) = app_state("state-tokens-restored");
        let issued = state.issue_mcp_token("s");
        state.restore_mcp_token("s", "kept-by-the-holder");
        assert!(state.session_for_mcp_token(&issued).is_none());
        assert_eq!(
            state.session_for_mcp_token("kept-by-the-holder").as_deref(),
            Some("s")
        );
        assert!(state.session_for_mcp_token("other").is_none());
    }

    /// A token is per launch: the previous process is gone, and a token that outlived it would let
    /// a stale child act on the session.
    #[test]
    fn issuing_a_token_retires_the_sessions_previous_one() {
        let (state, _dir) = app_state("state-tokens");
        let first = state.issue_mcp_token("s");
        assert_eq!(state.session_for_mcp_token(&first).as_deref(), Some("s"));

        let second = state.issue_mcp_token("s");
        assert_ne!(first, second);
        assert!(state.session_for_mcp_token(&first).is_none());
        assert_eq!(state.session_for_mcp_token(&second).as_deref(), Some("s"));
    }
}
