//! The account-level MCP inventory from OpenCode's merged configuration.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crate::accounts::mcp_inventory::{query_unavailable, Listing, McpInventoryReader};
use crate::adapter::account_cli;
use crate::protocol::{
    AgentId, LocalMcpServer, McpInventoryFailure, McpInventoryUnavailable, McpRuntimeState,
    McpSignInState,
};
use crate::terminal::plain_text::strip_escapes;

pub(super) struct OpenCodeMcpInventory;

impl McpInventoryReader for OpenCodeMcpInventory {
    fn read(
        &self,
        shell_env: &HashMap<String, String>,
        config_dir: Option<&Path>,
        _refresh: bool,
        timeout: Duration,
        cancel: &AtomicBool,
    ) -> Result<Listing, McpInventoryUnavailable> {
        let refused = || McpInventoryUnavailable::new(McpInventoryFailure::Failed, None);
        let Some(v2) = super::version::background_v2(shell_env) else {
            return Err(refused());
        };
        let args = if v2 {
            &["debug", "config", "--standalone"][..]
        } else {
            &["mcp", "list"][..]
        };
        let output = account_cli::query(
            AgentId::OPENCODE,
            shell_env,
            config_dir,
            args,
            timeout,
            512 * 1024,
            Some(cancel),
        )
        .map_err(query_unavailable)?;
        if !output.status.success() {
            return Err(refused());
        }
        let servers = if v2 {
            configured_v2_servers(&output.stdout)
        } else {
            parse_list(&strip_escapes(&String::from_utf8_lossy(&output.stdout)))
        }
        .ok_or_else(refused)?;
        Ok((Vec::new(), servers))
    }
}

// The v2 CLI can list zero running servers before it loads the config's MCP plugin. Its
// config projection has the configured names. Only the names and local/remote kind leave here.
fn configured_v2_servers(stdout: &[u8]) -> Option<Vec<LocalMcpServer>> {
    let documents: Vec<serde_json::Value> = serde_json::from_slice(stdout).ok()?;
    let mut servers = BTreeMap::new();
    for document in documents
        .iter()
        .filter(|document| document["type"] == "document")
    {
        if let Some(entries) = document["info"]["mcp"]["servers"].as_object() {
            for (name, entry) in entries {
                let sign_in = if entry["type"] == "local" {
                    McpSignInState::NotNeeded
                } else {
                    McpSignInState::Unknown
                };
                servers.insert(
                    name.clone(),
                    LocalMcpServer {
                        name: name.clone(),
                        origin: None,
                        plugin: None,
                        sign_in,
                        runtime: McpRuntimeState::Unknown,
                        runtime_message: None,
                        tool_count: None,
                        oauth_sign_in: false,
                    },
                );
            }
        }
    }
    Some(servers.into_values().collect())
}

fn parse_list(text: &str) -> Option<Vec<LocalMcpServer>> {
    if text.contains("No MCP servers configured") {
        return Some(Vec::new());
    }
    let count: usize = text.lines().find_map(|line| {
        line.strip_prefix('└')?
            .trim()
            .strip_suffix(" server(s)")?
            .parse()
            .ok()
    })?;
    let servers: Vec<_> = text
        .lines()
        .filter_map(|line| {
            let (_, label) = line.strip_prefix('●')?.trim().split_once(' ')?;
            let (name, status) = [
                "connected (OAuth)",
                "connected",
                "disabled",
                "failed",
                "not initialized",
                "needs authentication",
                "needs client registration",
            ]
            .into_iter()
            .find_map(|status| {
                Some((
                    label.strip_suffix(status)?.strip_suffix(' ')?.to_owned(),
                    status,
                ))
            })?;
            let runtime = match status {
                "connected (OAuth)" | "connected" => McpRuntimeState::Running,
                "failed" => McpRuntimeState::Failed,
                _ => McpRuntimeState::Unknown,
            };
            let sign_in = match status {
                "connected (OAuth)" => McpSignInState::SignedIn,
                "needs authentication" | "needs client registration" => McpSignInState::NeedsSignIn,
                _ => McpSignInState::Unknown,
            };
            // The following detail lines can include a remote URL or the local command and its
            // arguments, so only the name and state of an entry are carried over.
            Some(LocalMcpServer {
                name,
                origin: None,
                plugin: None,
                sign_in,
                runtime,
                runtime_message: None,
                tool_count: None,
                oauth_sign_in: false,
            })
        })
        .collect();
    (servers.len() == count).then_some(servers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_configured_servers_from_the_cli_capture() {
        let text = include_str!("../../../testdata/opencode_mcp_list_1.19.6.txt");
        let servers = parse_list(&strip_escapes(text)).expect("MCP inventory");
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].name, "disabled_example");
        assert_eq!(servers[0].runtime, McpRuntimeState::Unknown);
        assert_eq!(servers[1].name, "failed_example");
        assert_eq!(servers[1].runtime, McpRuntimeState::Failed);
    }
    #[test]
    fn v2_lists_configured_servers_from_its_config_projection() {
        let capture = include_bytes!("../../../testdata/opencode_mcp_config_2.4.1.json");
        let servers = configured_v2_servers(capture).unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "probe");
        assert_eq!(servers[0].runtime, McpRuntimeState::Unknown);
        assert_eq!(servers[0].sign_in, McpSignInState::NotNeeded);
    }
}
