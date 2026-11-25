//! Pi's account-level MCP inventory. The CLI's diagnostic list starts the servers and initializes
//! their OAuth store, so the inventory reads the configuration without executing those servers.

use std::collections::{BTreeMap, HashMap};
use std::io::ErrorKind;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Deserialize;

use crate::accounts::mcp_inventory::{Listing, McpInventoryReader};
use crate::adapter::account_cli::account_dir;
use crate::adapter::files::{read_regular_text, SMALL_FILE_LIMIT};
use crate::protocol::{
    AgentId, LocalMcpServer, McpInventoryFailure, McpInventoryUnavailable, McpRuntimeState,
    McpSignInState,
};

pub(super) struct PiMcpInventory;

#[derive(Deserialize)]
struct Configuration {
    #[serde(default, rename = "mcpServers")]
    servers: BTreeMap<String, Configured>,
}

#[derive(Deserialize)]
struct Configured {
    command: Option<String>,
    url: Option<String>,
}

impl McpInventoryReader for PiMcpInventory {
    fn read(
        &self,
        shell_env: &HashMap<String, String>,
        config_dir: Option<&Path>,
        _refresh: bool,
        _timeout: Duration,
        cancel: &AtomicBool,
    ) -> Result<Listing, McpInventoryUnavailable> {
        if cancel.load(Ordering::Acquire) {
            return Err(McpInventoryUnavailable::new(
                McpInventoryFailure::TimedOut,
                None,
            ));
        }
        let file = account_dir(AgentId::PI, config_dir, shell_env).join("mcp.json");
        let text = match read_regular_text(&file, SMALL_FILE_LIMIT) {
            Ok(text) => text,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok((vec![], vec![])),
            Err(_) => return Err(unreadable()),
        };
        let config: Configuration =
            serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|_| unreadable())?;
        let servers = config
            .servers
            .into_iter()
            .filter_map(|(name, config)| {
                if config.url.is_none() && config.command.is_none() {
                    return None;
                }
                // HTTP paths, queries and user info may hold credentials. Only show the origin.
                let origin = config
                    .url
                    .as_deref()
                    .and_then(|url| url.parse::<axum::http::Uri>().ok())
                    .and_then(|uri| {
                        let scheme = uri.scheme_str()?;
                        if !matches!(scheme, "http" | "https") {
                            return None;
                        }
                        let authority = uri.authority()?.as_str().rsplit('@').next()?;
                        Some(format!("{scheme}://{authority}"))
                    });
                Some(LocalMcpServer {
                    name,
                    origin,
                    plugin: None,
                    sign_in: if config.url.is_some() {
                        McpSignInState::Unknown
                    } else {
                        McpSignInState::NotNeeded
                    },
                    runtime: McpRuntimeState::Unknown,
                    runtime_message: None,
                    tool_count: None,
                    oauth_sign_in: false,
                })
            })
            .collect();
        Ok((vec![], servers))
    }
}

fn unreadable() -> McpInventoryUnavailable {
    McpInventoryUnavailable::new(
        McpInventoryFailure::Failed,
        Some("Pi's mcp.json could not be read".to_owned()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    #[test]
    fn configured_servers_are_listed_without_starting_their_commands() {
        let dir = ScratchDir::new("pi-mcp-inventory");
        std::fs::write(
            dir.join("mcp.json"),
            r#"{"mcpServers":{"local":{"command":"/nonexistent/pi-probe"},"remote":{"url":"http://127.0.0.1:1/mcp"}}}"#,
        )
        .unwrap();
        let (connectors, servers) = PiMcpInventory
            .read(
                &HashMap::new(),
                Some(&dir),
                true,
                Duration::from_secs(1),
                &AtomicBool::new(false),
            )
            .unwrap();
        assert!(connectors.is_empty());
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].name, "local");
        assert_eq!(servers[0].sign_in, McpSignInState::NotNeeded);
        assert_eq!(servers[1].name, "remote");
        assert_eq!(servers[1].origin.as_deref(), Some("http://127.0.0.1:1"));
        assert!(servers
            .iter()
            .all(|server| server.runtime == McpRuntimeState::Unknown));
        assert!(!dir.join("mcp-auth.json").exists());
    }
}
