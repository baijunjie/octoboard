//! What an account's agent reports of its MCP servers and connectors
//! (`account_mcp_inventory_read`), and how signing an account in to or out of one of them ended
//! (`account_mcp_server_auth_ended`).
//! See "Account MCP inventory" in `apps/daemon/PROTOCOL.md`.

use serde::Serialize;

use super::agents::AgentId;

/// How an agent's account stands with the MCP servers and connectors it has, read through the
/// agent's own CLI at `read_at`. Runtime state of the daemon: never stored.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AccountMcpInventory {
    pub agent: AgentId,
    /// The account's id; `None` for the agent's default account.
    pub account: Option<String>,
    /// When the reading was taken, milliseconds since the epoch.
    pub read_at: i64,
    /// The connectors the agent's platform hosts for the account (Claude's claude.ai connectors,
    /// Codex's Apps). Empty when `unavailable` is set.
    pub connectors: Vec<PlatformConnector>,
    /// The MCP servers configured for the account, whether by the user or by a plugin. Empty when
    /// `unavailable` is set.
    pub servers: Vec<LocalMcpServer>,
    /// Why the account could not be read; the two lists are empty then.
    pub unavailable: Option<McpInventoryUnavailable>,
}

/// Why an account's MCP servers and connectors could not be read. The reason is a closed set the
/// client words itself; `detail` is what the agent or the daemon said, for the cases it helps.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpInventoryUnavailable {
    pub reason: McpInventoryFailure,
    pub detail: Option<String>,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum McpInventoryFailure {
    /// The account is not signed in to the agent.
    NotSignedIn,
    /// The agent's CLI is not installed, or cannot be started.
    CliMissing,
    /// The CLI did not answer in time.
    TimedOut,
    /// The agent has no way of listing its MCP servers.
    UnsupportedAgent,
    /// Anything else; `detail` says what.
    Failed,
}

/// A connector the agent's platform hosts for the account.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlatformConnector {
    /// The agent's own identifier for the connector.
    pub id: String,
    pub name: String,
    pub state: ConnectorState,
    /// The plugins that bring the connector (Codex); empty for Claude Code.
    pub plugins: Vec<String>,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorState {
    Connected,
    NotConnected,
    Unknown,
}

/// An MCP server configured for the account.
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalMcpServer {
    pub name: String,
    /// The URL of a remote server; null for a server that is a local command.
    pub origin: Option<String>,
    /// The plugin that brings the server; null for one the user configured.
    pub plugin: Option<String>,
    pub sign_in: McpSignInState,
    pub runtime: McpRuntimeState,
    /// Why the server failed, when `runtime` is `failed` and the agent says.
    pub runtime_message: Option<String>,
    /// How many tools the server offers, where the agent reports it.
    pub tool_count: Option<u32>,
    /// Whether the account signs in to this server with OAuth through the agent's own login
    /// command, which Octoboard can run (`sign_in_account_mcp_server` and
    /// `sign_out_account_mcp_server`).
    pub oauth_sign_in: bool,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum McpSignInState {
    SignedIn,
    NeedsSignIn,
    /// The server takes no sign-in (a local command, a server that has none).
    NotNeeded,
    Unknown,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum McpRuntimeState {
    Running,
    Failed,
    Unknown,
}

/// How a sign-in to or sign-out of an MCP server ended (`account_mcp_server_auth_ended`).
#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpServerAuthOutcome {
    pub status: McpServerAuthStatus,
    /// What the agent's CLI said, for `failed`; null otherwise.
    pub detail: Option<String>,
}

#[cfg_attr(test, derive(ts_rs::TS))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum McpServerAuthStatus {
    Succeeded,
    /// The user ended a sign-in with `cancel_account_mcp_sign_in`, or the account's removal did.
    Cancelled,
    /// The CLI did not end in time and was killed.
    TimedOut,
    Failed,
}

impl McpServerAuthOutcome {
    pub fn of(status: McpServerAuthStatus) -> Self {
        Self {
            status,
            detail: None,
        }
    }

    pub fn failed(detail: String) -> Self {
        Self {
            status: McpServerAuthStatus::Failed,
            detail: Some(detail),
        }
    }
}

impl McpInventoryUnavailable {
    pub fn new(reason: McpInventoryFailure, detail: Option<String>) -> Self {
        Self { reason, detail }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_auth_outcome_has_the_documented_shape() {
        assert_eq!(
            serde_json::to_value(McpServerAuthOutcome::of(McpServerAuthStatus::TimedOut)).unwrap(),
            serde_json::json!({"status": "timed_out", "detail": null})
        );
        assert_eq!(
            serde_json::to_value(McpServerAuthOutcome::failed("no".to_string())).unwrap(),
            serde_json::json!({"status": "failed", "detail": "no"})
        );
    }

    #[test]
    fn an_unavailable_inventory_has_the_documented_shape() {
        let inventory = AccountMcpInventory {
            agent: AgentId::CODEX,
            account: None,
            read_at: 1_792_258_799_899,
            connectors: vec![PlatformConnector {
                id: "gmail".to_string(),
                name: "Gmail".to_string(),
                state: ConnectorState::NotConnected,
                plugins: vec![],
            }],
            servers: vec![],
            unavailable: Some(McpInventoryUnavailable::new(
                McpInventoryFailure::NotSignedIn,
                None,
            )),
        };
        assert_eq!(
            serde_json::to_value(inventory).unwrap(),
            serde_json::json!({
                "agent": "codex", "account": null, "read_at": 1_792_258_799_899_i64,
                "connectors": [{"id": "gmail", "name": "Gmail", "state": "not_connected",
                    "plugins": []}],
                "servers": [],
                "unavailable": {"reason": "not_signed_in", "detail": null}
            })
        );
    }
}
