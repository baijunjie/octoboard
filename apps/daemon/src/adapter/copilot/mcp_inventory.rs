//! The account's configured MCP servers, including Copilot's own servers and those a plugin brings.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use serde_json::Value;

use super::{accounts, models};
use crate::accounts::mcp_inventory::{
    not_run_unavailable, query_unavailable, Listing, McpInventoryReader,
};
use crate::adapter::account_cli::{self, QueryFailure};
use crate::adapter::user_json;
use crate::protocol::{
    AgentId, LocalMcpServer, McpInventoryFailure, McpInventoryUnavailable, McpRuntimeState,
    McpSignInState,
};

const OUTPUT_LIMIT: usize = 4 * 1024 * 1024;
const SETTINGS: &str = "settings.json";
const MCP_CONFIG: &str = "mcp-config.json";

pub(super) struct CopilotMcpInventory;

impl McpInventoryReader for CopilotMcpInventory {
    fn read(
        &self,
        shell_env: &HashMap<String, String>,
        config_dir: Option<&Path>,
        _refresh: bool,
        timeout: Duration,
        cancel: &AtomicBool,
    ) -> Result<Listing, McpInventoryUnavailable> {
        let agent = AgentId::COPILOT;
        let account = account_cli::account_dir(agent, config_dir, shell_env);
        let state = accounts::state_of(&account);
        let own = models::private_query_home(&crate::paths::run_dir(), state.as_ref())
            .map_err(|_| failed("Could not prepare an isolated Copilot MCP listing"))?;
        let mut run = account_cli::admitted_command_in_home(
            agent,
            shell_env,
            config_dir,
            Some(&own),
            &["--no-auto-update"],
        )
        .map_err(|error| not_run_unavailable(&error))?;
        run.command.current_dir(own.path());
        run.command.arg("--log-dir").arg(own.path().join("logs"));
        if let Some(token) = models::token_to_hand(state.as_ref(), shell_env) {
            run.command.env("COPILOT_GITHUB_TOKEN", token);
        }
        let config = account.join(MCP_CONFIG);
        if config.is_file() {
            run.command
                .arg("--additional-mcp-config")
                .arg(format!("@{}", config.display()));
        }
        let settings = read_settings(&account.join(SETTINGS))?;
        for (key, flag) in [
            ("disabledMcpServers", "--disable-mcp-server"),
            ("enabledMcpServers", "--enable-mcp-server"),
        ] {
            for name in settings[key]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                run.command.arg(format!("{flag}={name}"));
            }
        }
        for path in plugin_dirs(state.as_ref(), &settings) {
            run.command.arg("--plugin-dir").arg(path);
        }
        run.command.args(["mcp", "list", "--json"]);
        let result = account_cli::run_query(&mut run.command, timeout, OUTPUT_LIMIT, Some(cancel))
            .map_err(|error| query_unavailable(QueryFailure::Bounded(error)))?;
        if !result.status.success() {
            // The CLI's listing and errors can echo the configured headers and environment.
            return Err(failed("Copilot could not list the account's MCP servers"));
        }
        let result = serde_json::from_slice(&result.stdout)
            .ok()
            .and_then(|value| servers(&value))
            .ok_or_else(|| failed("Copilot returned an unreadable MCP listing"))?;
        Ok((Vec::new(), result))
    }
}

fn read_settings(path: &Path) -> Result<Value, McpInventoryUnavailable> {
    match crate::hostfs::read_regular_text(path, OUTPUT_LIMIT) {
        Ok(text) => user_json::parse(&text, accounts::STATE_FILE_TAKES)
            .map_err(|_| failed("Copilot's MCP listing could not read settings.json")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Value::Null),
        Err(_) => Err(failed("Copilot's MCP listing could not read settings.json")),
    }
}

fn plugin_dirs(state: Option<&Value>, settings: &Value) -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    if let Some(installed) = state.and_then(|state| state["installedPlugins"].as_array()) {
        dirs.extend(installed.iter().filter_map(|plugin| {
            let name = plugin["name"].as_str()?;
            let marketplace = plugin["marketplace"].as_str()?;
            let enabled = settings["enabledPlugins"][format!("{name}@{marketplace}")]
                .as_bool()
                .or_else(|| plugin["enabled"].as_bool())
                .unwrap_or(true);
            enabled.then_some(())?;
            let path = Path::new(plugin["cache_path"].as_str()?);
            (path.is_absolute() && path.is_dir()).then(|| path.to_owned())
        }));
    }
    // A directory marketplace is used live and has no installedPlugins entry or cached copy.
    if let Some(enabled) = settings["enabledPlugins"].as_object() {
        dirs.extend(enabled.iter().filter_map(|(key, value)| {
            if value.as_bool() != Some(true) {
                return None;
            }
            let (name, marketplace) = key.rsplit_once('@')?;
            let source = &settings["extraKnownMarketplaces"][marketplace]["source"];
            if source["source"] != "directory" {
                return None;
            }
            let root = Path::new(source["path"].as_str()?);
            [".claude-plugin", ".github/plugin"]
                .into_iter()
                .find_map(|manifest| {
                    let text = crate::hostfs::read_regular_text(
                        &root.join(manifest).join("marketplace.json"),
                        OUTPUT_LIMIT,
                    )
                    .ok()?;
                    let listing: Value = serde_json::from_str(&text).ok()?;
                    let entry = listing["plugins"]
                        .as_array()?
                        .iter()
                        .find(|plugin| plugin["name"] == name)?;
                    let path = root.join(entry["source"].as_str()?);
                    (path.is_absolute() && path.is_dir()).then_some(path)
                })
        }));
    }
    dirs
}

fn servers(value: &Value) -> Option<Vec<LocalMcpServer>> {
    Some(
        value
            .get("mcpServers")?
            .as_object()?
            .iter()
            .filter_map(|(name, server)| {
                if server["enabled"] == false {
                    return None;
                }
                let origin = server["url"].as_str().and_then(origin_of);
                let local = matches!(server["type"].as_str(), Some("local" | "stdio"));
                Some(LocalMcpServer {
                    name: name.to_owned(),
                    origin,
                    plugin: server["sourcePlugin"].as_str().map(str::to_owned),
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
            })
            .collect(),
    )
}

// User-supplied URLs can hold credentials. Only HTTP(S) scheme and host go to the window.
fn origin_of(url: &str) -> Option<String> {
    let uri: axum::http::Uri = url.parse().ok()?;
    let scheme = uri
        .scheme_str()
        .filter(|scheme| matches!(*scheme, "https" | "http"))?;
    let authority = uri.authority()?.as_str().rsplit('@').next()?;
    Some(format!("{scheme}://{authority}"))
}

fn failed(detail: &str) -> McpInventoryUnavailable {
    McpInventoryUnavailable::new(McpInventoryFailure::Failed, Some(detail.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_listing_includes_configured_and_builtin_servers_without_claiming_runtime_status() {
        let value = serde_json::from_str(include_str!(
            "../../../testdata/copilot_mcp_list_1.0.114.json"
        ))
        .unwrap();
        let found = servers(&value).unwrap();
        let get = |name| found.iter().find(|server| server.name == name).unwrap();
        assert!(!found.iter().any(|server| server.name == "example"));
        assert_eq!(
            get("github-mcp-server").origin.as_deref(),
            Some("https://api.individual.githubcopilot.com")
        );
        assert_eq!(get("computer-use").plugin.as_deref(), Some("computer-use"));
        assert_eq!(get("local").sign_in, McpSignInState::NotNeeded);
        assert_eq!(get("github-mcp-server").runtime, McpRuntimeState::Unknown);
    }
}
