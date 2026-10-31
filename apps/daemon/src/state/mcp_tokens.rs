//! The per-session tokens the MCP surface is reached with.

use super::AppState;

impl AppState {
    /// Issues the token one session's MCP server authenticates with. A fresh one per launch: the
    /// previous process is gone, and a token that outlived it would let a stale child act on the
    /// session the id was reused for.
    pub fn issue_mcp_token(&self, session_id: &str) -> String {
        let token = uuid::Uuid::new_v4().to_string();
        let mut tokens = self.mcp_tokens.write().expect("mcp token lock poisoned");
        tokens.retain(|_, owner| owner != session_id);
        tokens.insert(token.clone(), session_id.to_string());
        token
    }

    /// Retires this session's token without waiting for its process to go away. Used where no
    /// process ever started, so nothing will ever observe the exit that would otherwise retire it.
    pub fn revoke_mcp_tokens(&self, session_id: &str) {
        self.mcp_tokens
            .write()
            .expect("mcp token lock poisoned")
            .retain(|_, owner| owner != session_id);
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
