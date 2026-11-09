//! Reading a Claude Code account's MCP servers and connectors: `claude auth status` for whether
//! anyone is signed in, then `claude mcp list`, which has no JSON form and prints one
//! `<name>: <transport> - <marker> <status>` line per server after connecting to each of them.
//! The default account is read with `CLAUDE_CONFIG_DIR` as the shell has it, a non-default one with
//! its own directory, as `account_cli` runs every query. The listing has nothing to refetch, so a
//! `refresh` changes nothing.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use super::login::{logged_in, STATUS_OUTPUT_LIMIT, STATUS_TIMEOUT};
use crate::accounts::mcp_inventory::{
    query_unavailable, stderr_detail, Listing, McpInventoryReader,
};
use crate::adapter::account_cli;
use crate::protocol::{
    AgentId, ConnectorState, LocalMcpServer, McpInventoryFailure, McpInventoryUnavailable,
    McpRuntimeState, McpSignInState, PlatformConnector,
};

/// The most `claude mcp list` may print: a line per server.
const LIST_OUTPUT_LIMIT: usize = 1024 * 1024;

/// The name prefix of a connector the claude.ai account hosts, as `claude mcp list` shows it.
const CONNECTOR_PREFIX: &str = "claude.ai ";

pub(super) struct ClaudeMcpInventory;

impl McpInventoryReader for ClaudeMcpInventory {
    fn read(
        &self,
        shell_env: &HashMap<String, String>,
        config_dir: Option<&Path>,
        _refresh: bool,
        timeout: Duration,
        cancel: &AtomicBool,
    ) -> Result<Listing, McpInventoryUnavailable> {
        let run = |args: &[&str], timeout, limit| {
            account_cli::query(
                AgentId::CLAUDE,
                shell_env,
                config_dir,
                args,
                timeout,
                limit,
                Some(cancel),
            )
            .map_err(query_unavailable)
        };
        // `auth status` exits 1 when signed out, with its answer on stdout all the same.
        let status = run(
            &["auth", "status"],
            STATUS_TIMEOUT.min(timeout),
            STATUS_OUTPUT_LIMIT,
        )?;
        match logged_in(&status.stdout) {
            Some(false) => {
                return Err(unavailable(McpInventoryFailure::NotSignedIn, None));
            }
            Some(true) => {}
            None => {
                return Err(unavailable(
                    McpInventoryFailure::Failed,
                    Some(stderr_detail(
                        &status,
                        "`claude auth status` gave no answer",
                    )),
                ));
            }
        }
        let list = run(&["mcp", "list"], timeout, LIST_OUTPUT_LIMIT)?;
        if !list.status.success() {
            return Err(unavailable(
                McpInventoryFailure::Failed,
                Some(stderr_detail(&list, "`claude mcp list` failed")),
            ));
        }
        Ok(parse_list(&String::from_utf8_lossy(&list.stdout)))
    }
}

fn unavailable(reason: McpInventoryFailure, detail: Option<String>) -> McpInventoryUnavailable {
    McpInventoryUnavailable::new(reason, detail)
}

/// What a line of `claude mcp list` says of its server.
#[derive(Debug, PartialEq, Eq)]
enum Status {
    Connected,
    NeedsAuthentication,
    /// With the reason the line gives after the dash, if it does.
    Failed(Option<String>),
    /// A status this reading does not know.
    Other,
}

/// The status in `text`, the part of a line after the last ` - ` that names one: a marker
/// character and a space, optionally, then the words.
fn status_of(text: &str) -> Option<Status> {
    let text = text.trim();
    let words = match text.split_once(' ') {
        Some((marker, rest))
            if marker.chars().count() == 1 && !marker.chars().all(char::is_alphanumeric) =>
        {
            rest.trim_start()
        }
        _ => text,
    };
    if words.starts_with("Connected") {
        Some(Status::Connected)
    } else if words.starts_with("Needs authentication") {
        Some(Status::NeedsAuthentication)
    } else {
        let after = words.strip_prefix("Failed to connect")?;
        let reason = after
            .trim()
            .strip_prefix('\u{2014}')
            .map(str::trim)
            .filter(|reason| !reason.is_empty());
        Some(Status::Failed(reason.map(str::to_string)))
    }
}

/// One server line: its name, the transport it is run or reached by, and its status.
struct Line<'a> {
    name: &'a str,
    transport: &'a str,
    status: Status,
}

/// Reads `<name>: <transport> - <status>`; `None` for a line that is not one (the header, a blank
/// line, the notice that none is configured). The status is taken from the last ` - ` that is
/// followed by a known one, since a command or a reason may hold a ` - ` itself; a line with no
/// such dash that still has one is a server of an unknown status.
fn parse_line(line: &str) -> Option<Line<'_>> {
    let (name, rest) = line.split_once(": ")?;
    let dashes: Vec<usize> = rest.match_indices(" - ").map(|(at, _)| at).collect();
    let last = *dashes.last()?;
    let (at, status) = dashes
        .iter()
        .rev()
        .find_map(|&at| Some((at, status_of(&rest[at + 3..])?)))
        .unwrap_or((last, Status::Other));
    Some(Line {
        name: name.trim(),
        transport: rest[..at].trim(),
        status,
    })
}

/// The URL of a remote server, from a transport of the form `<url> (<KIND>)`; `None` for a
/// command.
fn origin_of(transport: &str) -> Option<String> {
    let inner = transport.strip_suffix(')')?;
    let (url, kind) = inner.rsplit_once(" (")?;
    let is_kind = !kind.is_empty()
        && kind
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-');
    (is_kind && !url.trim().is_empty()).then(|| url.trim().to_string())
}

/// The connectors and servers of a `claude mcp list` output.
fn parse_list(output: &str) -> Listing {
    let mut connectors = Vec::new();
    let mut servers = Vec::new();
    for line in output.lines().filter_map(parse_line) {
        match line.name.strip_prefix(CONNECTOR_PREFIX) {
            Some(name) => connectors.push(PlatformConnector {
                id: line.name.to_string(),
                name: name.to_string(),
                state: connector_state(&line.status),
                plugins: vec![],
            }),
            None => servers.push(server_of(&line)),
        }
    }
    (connectors, servers)
}

/// A claude.ai connector's status as a connector state: the one place Claude Code's values are
/// mapped for connectors, where a value not known here is `unknown`.
fn connector_state(status: &Status) -> ConnectorState {
    match status {
        Status::Connected => ConnectorState::Connected,
        Status::NeedsAuthentication => ConnectorState::NotConnected,
        Status::Failed(_) | Status::Other => ConnectorState::Unknown,
    }
}

/// A server line as a server: the one place Claude Code's values are mapped for servers.
fn server_of(line: &Line) -> LocalMcpServer {
    let origin = origin_of(line.transport);
    let plugin = line
        .name
        .strip_prefix("plugin:")
        .and_then(|rest| rest.split_once(':'))
        .map(|(plugin, _)| plugin.to_string())
        .filter(|plugin| !plugin.is_empty());
    let sign_in = match (&line.status, &origin) {
        (Status::NeedsAuthentication, _) => McpSignInState::NeedsSignIn,
        (Status::Connected | Status::Failed(_), None) => McpSignInState::NotNeeded,
        _ => McpSignInState::Unknown,
    };
    let (runtime, runtime_message) = match &line.status {
        Status::Connected => (McpRuntimeState::Running, None),
        Status::Failed(reason) => (McpRuntimeState::Failed, reason.clone()),
        Status::NeedsAuthentication | Status::Other => (McpRuntimeState::Unknown, None),
    };
    LocalMcpServer {
        name: line.name.to_string(),
        origin,
        plugin,
        sign_in,
        runtime,
        runtime_message,
        tool_count: None,
        // Octoboard has no way to run Claude Code's sign-in to a server, which is its own prompt.
        oauth_sign_in: false,
    }
}

#[cfg(test)]
mod tests;
