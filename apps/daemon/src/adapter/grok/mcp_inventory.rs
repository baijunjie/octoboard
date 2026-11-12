//! Reading a Grok Build account's MCP servers: `grok mcp list --json` for the servers configured
//! in the account's `config.toml`, then `grok mcp doctor --json`, which
//! starts every enabled server and reports how its checks went. Grok has no command that lists
//! platform connectors or signs an account in to a server (that is OAuth inside its TUI's `/mcps`
//! modal), so the reading has neither. Both commands run in the account's `GROK_HOME` (the default
//! account's as the shell has it) and neither joins a leader process, so a non-default account's
//! reading never reaches another account's. The listing carries the servers' headers and
//! environment variables, which are not read, so no secret in them is ever held here. A `refresh`
//! changes nothing, as there is nothing to refetch.
//!
//! Two quirks of the CLI shape what is shown: the listing prints `[]` and exits 0 for a
//! `config.toml` that is not valid TOML, so such a configuration reads as no servers, as Grok
//! itself reports it; and `doctor` also reports the servers Grok takes from `~/.claude.json` and
//! the working directory's `.mcp.json`, which the listing leaves out.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::accounts::mcp_inventory::{
    enabled_by_default, failure_detail, query_unavailable, Listing, McpInventoryReader,
};
use crate::adapter::account_cli;
use crate::protocol::{
    AgentId, LocalMcpServer, McpInventoryFailure, McpInventoryUnavailable, McpRuntimeState,
    McpSignInState,
};

/// The most either command may print.
const OUTPUT_LIMIT: usize = 1024 * 1024;

/// How long `grok mcp list` may take: it only reads configuration.
const LIST_TIMEOUT: Duration = Duration::from_secs(15);

/// The longest failure reason kept, in characters.
const MESSAGE_LIMIT: usize = 300;

pub(super) struct GrokMcpInventory;

impl McpInventoryReader for GrokMcpInventory {
    fn read(
        &self,
        shell_env: &HashMap<String, String>,
        config_dir: Option<&Path>,
        _refresh: bool,
        timeout: Duration,
        cancel: &AtomicBool,
    ) -> Result<Listing, McpInventoryUnavailable> {
        let deadline = Instant::now() + timeout;
        let left = || deadline.saturating_duration_since(Instant::now());
        let run = |args: &[&str], timeout| {
            account_cli::query(
                AgentId::GROK,
                shell_env,
                config_dir,
                args,
                timeout,
                OUTPUT_LIMIT,
                Some(cancel),
            )
            .map_err(query_unavailable)
        };
        let list = run(&["mcp", "list", "--json"], LIST_TIMEOUT.min(left()))?;
        if !list.status.success() {
            return Err(unavailable(
                McpInventoryFailure::Failed,
                Some(failure_detail(&list, "`grok mcp list` failed")),
            ));
        }
        let configured: Vec<Configured> = serde_json::from_slice(&list.stdout).map_err(|err| {
            unavailable(
                McpInventoryFailure::Failed,
                Some(format!("`grok mcp list` gave no listing: {err}")),
            )
        })?;
        // `doctor` exits 1 when a server fails, with its report on stdout all the same; a report
        // that cannot be read leaves every state unknown.
        let doctor = run(&["mcp", "doctor", "--json"], left())?;
        let report = serde_json::from_slice::<Report>(&doctor.stdout).ok();
        Ok((vec![], servers_of(configured, report)))
    }
}

fn unavailable(reason: McpInventoryFailure, detail: Option<String>) -> McpInventoryUnavailable {
    McpInventoryUnavailable::new(reason, detail)
}

/// One entry of `grok mcp list --json`. Only these fields are read: the entry also carries the
/// server's `headers` and `env`.
#[derive(Deserialize)]
struct Configured {
    name: String,
    /// The URL of a remote server.
    url: Option<String>,
    /// A server turned off in the configuration, which `doctor` does not start or report.
    #[serde(default = "enabled_by_default")]
    enabled: bool,
}

/// `grok mcp doctor --json`. An entry that cannot be read is skipped, so one odd entry does not
/// void the rest of the report.
#[derive(Deserialize)]
struct Report {
    #[serde(default, deserialize_with = "readable_entries")]
    servers: Vec<Diagnosed>,
}

fn readable_entries<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Diagnosed>, D::Error> {
    let entries = Vec::<serde_json::Value>::deserialize(deserializer)?;
    Ok(entries
        .into_iter()
        .filter_map(|entry| serde_json::from_value(entry).ok())
        .collect())
}

#[derive(Deserialize)]
struct Diagnosed {
    name: String,
    /// `stdio`, `http` or `sse`; a server whose entry gives none is not taken for a remote one.
    #[serde(default)]
    transport: String,
    /// A server's command line, or its URL.
    #[serde(default)]
    target: String,
    #[serde(default)]
    healthy: bool,
    #[serde(default)]
    checks: Vec<Check>,
}

#[derive(Deserialize)]
struct Check {
    label: String,
    #[serde(default)]
    passed: bool,
    detail: Option<String>,
}

impl Diagnosed {
    fn is_remote(&self) -> bool {
        !self.transport.is_empty() && self.transport != "stdio"
    }

    /// How many tools the server offers, from the check that says `<n> tools discovered`.
    fn tool_count(&self) -> Option<u32> {
        self.checks.iter().find_map(|check| {
            let mut words = check.label.split_whitespace();
            let count = words.next()?.parse().ok()?;
            (words.next()?.starts_with("tool") && words.next()? == "discovered").then_some(count)
        })
    }

    /// Whether a check failed because the server asked for authentication.
    fn needs_sign_in(&self) -> bool {
        self.checks.iter().any(|check| {
            !check.passed
                && check
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.to_lowercase().contains("auth required"))
        })
    }

    /// Why the server failed: the first failed check's label and what it says, with the
    /// transport library's prefix left off.
    fn failure(&self) -> Option<String> {
        let check = self.checks.iter().find(|check| !check.passed)?;
        let detail = check.detail.as_deref().map(str::trim).unwrap_or_default();
        let detail = detail
            .split_once("] error: ")
            .map_or(detail, |(_, rest)| rest);
        let message = if detail.is_empty() {
            check.label.clone()
        } else {
            format!("{}: {detail}", check.label)
        };
        Some(message.chars().take(MESSAGE_LIMIT).collect())
    }
}

/// The servers the listing and the report give, in the listing's order and then the report's
/// servers the listing lacks, which Grok takes from other tools' configuration (Claude Code's
/// `~/.claude.json`, a `.mcp.json` in the directory the CLI runs from, the home directory). A
/// server turned off in the configuration is left out, as the protocol has no disabled state. A
/// listed server whose state the report does not give (no report, or no entry for it) is of
/// unknown state.
fn servers_of(configured: Vec<Configured>, report: Option<Report>) -> Vec<LocalMcpServer> {
    let mut diagnosed: Vec<Diagnosed> = report.map(|report| report.servers).unwrap_or_default();
    let mut servers: Vec<LocalMcpServer> = configured
        .into_iter()
        .filter(|entry| entry.enabled)
        .map(|entry| {
            let found = diagnosed.iter().position(|d| d.name == entry.name);
            let found = found.map(|at| diagnosed.remove(at));
            let remote = entry.url.is_some();
            server_of(entry.name, entry.url, remote, found.as_ref())
        })
        .collect();
    for rest in &diagnosed {
        let origin = rest.is_remote().then(|| rest.target.clone());
        servers.push(server_of(
            rest.name.clone(),
            origin,
            rest.is_remote(),
            Some(rest),
        ));
    }
    servers
}

/// A server's `doctor` report, if any, as a [`LocalMcpServer`]: the one place Grok's values are
/// mapped.
fn server_of(
    name: String,
    origin: Option<String>,
    remote: bool,
    diagnosed: Option<&Diagnosed>,
) -> LocalMcpServer {
    let not_needed = if remote {
        McpSignInState::Unknown
    } else {
        McpSignInState::NotNeeded
    };
    let (sign_in, runtime, runtime_message, tool_count) = match diagnosed {
        Some(d) if d.healthy => (not_needed, McpRuntimeState::Running, None, d.tool_count()),
        Some(d) if d.needs_sign_in() => (
            McpSignInState::NeedsSignIn,
            McpRuntimeState::Unknown,
            None,
            None,
        ),
        Some(d) => (not_needed, McpRuntimeState::Failed, d.failure(), None),
        None => (not_needed, McpRuntimeState::Unknown, None, None),
    };
    LocalMcpServer {
        name,
        origin,
        plugin: None,
        sign_in,
        runtime,
        runtime_message,
        tool_count,
        // Grok's sign-in to a server is its TUI's `/mcps` modal, which Octoboard cannot run.
        oauth_sign_in: false,
    }
}

#[cfg(test)]
mod tests;
