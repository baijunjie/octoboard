//! `agy mcp list` reads the global MCP file alone, with a stricter parser than the session.
//! Read that file as the session does and list active plugins with the CLI's read-only
//! `plugin list --json`. Their MCP files extend the configured servers.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use crate::accounts::mcp_inventory::{query_unavailable, Listing, McpInventoryReader};
use crate::adapter::account_cli::{account_dir, query};
use crate::protocol::{
    AgentId, LocalMcpServer, McpInventoryFailure, McpInventoryUnavailable, McpRuntimeState,
    McpSignInState,
};
use crate::providers::antigravity::{CONFIG_DIR, MCP_CONFIG_FILE};

const PLUGIN_LIST_LIMIT: usize = 1024 * 1024;

pub(super) struct AntigravityInventory;

#[derive(Deserialize)]
struct Plugin {
    name: String,
    enabled: bool,
    path: Option<PathBuf>,
}

impl McpInventoryReader for AntigravityInventory {
    fn read(
        &self,
        shell_env: &HashMap<String, String>,
        config_dir: Option<&Path>,
        _refresh: bool,
        timeout: Duration,
        cancel: &AtomicBool,
    ) -> Result<Listing, McpInventoryUnavailable> {
        if config_dir.is_some() {
            return Err(failed("Antigravity CLI has only its default account"));
        }
        let agent = AgentId::ANTIGRAVITY;
        let account = account_dir(agent, None, shell_env);
        let flag = super::home::directory_argument(&account);
        let output = query(
            agent,
            &super::home::without_update(shell_env),
            None,
            &[flag.as_str(), "plugin", "list", "--json"],
            timeout,
            PLUGIN_LIST_LIMIT,
            Some(cancel),
        )
        .map_err(query_unavailable)?;
        if !output.status.success() {
            return Err(failed("Antigravity CLI could not list its plugins"));
        }
        let plugins: Vec<Plugin> = serde_json::from_slice(&output.stdout)
            .map_err(|_| failed("Antigravity CLI returned an unreadable plugin list"))?;
        let mut servers = servers_in(&account.join(CONFIG_DIR), None)?;
        for plugin in plugins.into_iter().filter(|plugin| plugin.enabled) {
            if let Some(path) = plugin.path.filter(|path| path.is_absolute()) {
                servers.extend(servers_in(&path, Some(&plugin.name))?);
            }
        }
        Ok((Vec::new(), servers))
    }
}

fn servers_in(
    dir: &Path,
    plugin: Option<&str>,
) -> Result<Vec<LocalMcpServer>, McpInventoryUnavailable> {
    let config = super::home::read_mcp_config(&dir.join(MCP_CONFIG_FILE))
        .map_err(|_| failed("Antigravity CLI's MCP configuration could not be read"))?;
    let servers = match config.get("mcpServers") {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::Object(servers)) => servers,
        _ => {
            return Err(failed(
                "Antigravity CLI's MCP configuration has no server map",
            ))
        }
    };
    Ok(servers
        .iter()
        .filter_map(|(name, server)| {
            if server["disabled"].as_bool() == Some(true) {
                return None;
            }
            let remote = server["url"]
                .as_str()
                .or_else(|| server["serverUrl"].as_str());
            if remote.is_none() && !server["command"].is_string() {
                return None;
            }
            Some(LocalMcpServer {
                name: name.clone(),
                origin: remote.and_then(origin_of),
                plugin: plugin.map(str::to_owned),
                sign_in: if remote.is_some() {
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
        .collect())
}

fn origin_of(url: &str) -> Option<String> {
    let uri = url.parse::<hyper::Uri>().ok()?;
    let scheme = uri
        .scheme_str()
        .filter(|scheme| matches!(*scheme, "http" | "https"))?;
    // Only the origin is published. The URL's userinfo, path or query can hold credentials.
    let authority = uri.authority()?.as_str().rsplit('@').next()?;
    Some(format!("{scheme}://{authority}"))
}

fn failed(detail: &str) -> McpInventoryUnavailable {
    McpInventoryUnavailable::new(McpInventoryFailure::Failed, Some(detail.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fake_agent, ScratchDir, PATIENCE};

    #[test]
    fn antigravity_inventory_reads_global_and_enabled_plugin_servers() {
        let scratch = ScratchDir::new("antigravity-inventory");
        let config = scratch.join(".gemini").join(CONFIG_DIR);
        let active = config.join("plugins/on");
        let disabled = config.join("plugins/off");
        for dir in [&active, &disabled] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(
            config.join(MCP_CONFIG_FILE),
            r#"// the CLI's runtime accepts comments
{"mcpServers":{"local":{"command":"stub"}}}"#,
        )
        .unwrap();
        std::fs::write(
            active.join(MCP_CONFIG_FILE),
            r#"{"mcpServers":{"remote":{"serverUrl":"http://127.0.0.1:8981/mcp"}}}"#,
        )
        .unwrap();
        std::fs::write(
            disabled.join(MCP_CONFIG_FILE),
            r#"{"mcpServers":{"hidden":{"command":"stub"}}}"#,
        )
        .unwrap();
        let plugins = serde_json::json!([
            {"name":"on", "enabled":true, "path":active},
            {"name":"off", "enabled":false, "path":disabled}
        ]);
        let listing = scratch.join("plugins.json");
        std::fs::write(&listing, plugins.to_string()).unwrap();
        let mut env = fake_agent(
            &scratch,
            "agy",
            &format!(
                "test \"$2 $3 $4\" = 'plugin list --json' || exit 2\ncat '{}'",
                listing.display(),
            ),
        );
        env.insert("HOME".to_owned(), scratch.to_string_lossy().into_owned());
        let (connectors, servers) = AntigravityInventory
            .read(&env, None, false, PATIENCE, &AtomicBool::new(false))
            .unwrap();
        assert!(connectors.is_empty());
        let [local, remote] = servers.as_slice() else {
            panic!("expected a global and a plugin server");
        };
        assert_eq!(local.name, "local");
        assert_eq!(local.sign_in, McpSignInState::NotNeeded);
        assert_eq!(remote.name, "remote");
        assert_eq!(remote.plugin.as_deref(), Some("on"));
        assert_eq!(remote.origin.as_deref(), Some("http://127.0.0.1:8981"));
    }
}
