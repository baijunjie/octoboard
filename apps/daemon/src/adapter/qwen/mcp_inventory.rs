//! Account-level MCP settings, in ascending Qwen Code layer order. Reading the files avoids
//! starting servers or migrating the settings as `qwen mcp list` can.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use serde_json::Value;

use super::settings::{system_defaults_path, system_settings_path, SYSTEM_FILE_LIMIT};
use super::settings_file::{self, SettingsFile};
use crate::accounts::mcp_inventory::{Listing, McpInventoryReader};
use crate::accounts::shared_config::admitted_reading;
use crate::adapter::account_dir;
use crate::adapter::files::LARGE_FILE_LIMIT;
use crate::protocol::{
    AgentId, LocalMcpServer, McpInventoryFailure, McpInventoryUnavailable, McpRuntimeState,
    McpSignInState,
};
use crate::providers::qwen::QWEN_SETTINGS_FILE;

pub(super) struct QwenMcpInventory;

impl McpInventoryReader for QwenMcpInventory {
    fn read(
        &self,
        shell_env: &HashMap<String, String>,
        config_dir: Option<&Path>,
        _refresh: bool,
        _timeout: Duration,
        _cancel: &AtomicBool,
    ) -> Result<Listing, McpInventoryUnavailable> {
        let _reading = admitted_reading(config_dir)
            .map_err(|_| unavailable("The account directory is unavailable."))?;
        let account = account_dir(AgentId::QWEN, config_dir, shell_env);
        let layers = [
            (system_defaults_path(shell_env), SYSTEM_FILE_LIMIT),
            (account.join(QWEN_SETTINGS_FILE), LARGE_FILE_LIMIT),
            (system_settings_path(shell_env), SYSTEM_FILE_LIMIT),
        ];
        let mut configured = BTreeMap::new();
        for (file, limit) in layers {
            match settings_file::read(&file, limit) {
                SettingsFile::Missing => {}
                SettingsFile::Unusable(_) => {
                    return Err(unavailable("Qwen Code settings could not be read."))
                }
                SettingsFile::Settings(mut layer) => {
                    if let Some(Value::Object(servers)) = layer.remove("mcpServers") {
                        configured.extend(servers);
                    }
                }
            }
        }
        let servers = configured
            .into_iter()
            .filter_map(|(name, server)| server_of(name, &server))
            .collect();
        Ok((vec![], servers))
    }
}

fn unavailable(detail: &str) -> McpInventoryUnavailable {
    McpInventoryUnavailable::new(McpInventoryFailure::Failed, Some(detail.into()))
}

fn server_of(name: String, server: &Value) -> Option<LocalMcpServer> {
    let endpoint = server["httpUrl"]
        .as_str()
        .or_else(|| server["url"].as_str());
    let local = endpoint.is_none() && server["command"].as_str().is_some();
    if !local && endpoint.is_none() {
        return None;
    }
    Some(LocalMcpServer {
        name,
        origin: endpoint.and_then(remote_origin),
        plugin: None,
        sign_in: if local {
            McpSignInState::NotNeeded
        } else {
            McpSignInState::Unknown
        },
        runtime: McpRuntimeState::Unknown,
        runtime_message: None,
        tool_count: None,
        oauth_sign_in: false,
    })
}

fn remote_origin(endpoint: &str) -> Option<String> {
    let uri = endpoint.parse::<hyper::Uri>().ok()?;
    let scheme = uri.scheme_str()?;
    let authority = uri.authority()?.as_str();
    (matches!(scheme, "http" | "https") && !authority.contains('@'))
        .then(|| format!("{scheme}://{authority}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::ScratchDir;

    #[test]
    fn the_inventory_uses_each_accounts_settings_between_the_two_system_layers() {
        use super::super::settings::{SYSTEM_DEFAULTS_VAR, SYSTEM_SETTINGS_VAR};

        let account = ScratchDir::new("qwen-mcp-inventory");
        let defaults = account.join("system-defaults.json");
        let system = account.join("system-settings.json");
        for (file, contents) in [
            (
                &defaults,
                r#"{"mcpServers":{"defaults":{"command":"false"},"priority":{"command":"false"}}}"#,
            ),
            (
                &account.join(QWEN_SETTINGS_FILE),
                r#"{"mcpServers":{"user":{"command":"false"},"priority":{"url":"http://127.0.0.1:2"}}}"#,
            ),
            (
                &system,
                r#"{"mcpServers":{"system":{"httpUrl":"http://127.0.0.1:1/mcp"},"priority":{"httpUrl":"http://127.0.0.1:1"}}}"#,
            ),
        ] {
            std::fs::write(file, contents).expect("Qwen settings");
        }
        let env = HashMap::from([
            (
                SYSTEM_DEFAULTS_VAR.into(),
                defaults.to_string_lossy().into_owned(),
            ),
            (
                SYSTEM_SETTINGS_VAR.into(),
                system.to_string_lossy().into_owned(),
            ),
        ]);
        let (connectors, servers) = QwenMcpInventory
            .read(
                &env,
                Some(&account),
                false,
                Duration::ZERO,
                &AtomicBool::new(false),
            )
            .expect("account inventory");
        assert!(connectors.is_empty());
        assert_eq!(
            servers
                .iter()
                .map(|server| server.name.as_str())
                .collect::<Vec<_>>(),
            ["defaults", "priority", "system", "user"]
        );
        assert_eq!(servers[1].origin.as_deref(), Some("http://127.0.0.1:1"));
        assert_eq!(servers[2].origin.as_deref(), Some("http://127.0.0.1:1"));
        assert_eq!(servers[3].sign_in, McpSignInState::NotNeeded);
    }
}
