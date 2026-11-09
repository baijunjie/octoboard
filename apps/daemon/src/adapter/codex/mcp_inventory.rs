//! Reading a Codex account's MCP servers and Apps on `codex app-server`: `mcpServerStatus/list`
//! for the servers and `app/installed` for the Apps the account has, after `account/read` says
//! whether anyone is signed in, then `app/read` for those Apps' names and the plugins that bring
//! them. The methods are experimental, so every field is read defensively: an `app/installed` the
//! installed Codex does not know leaves the connectors empty, and an `app/read` it does not know or
//! cannot answer leaves the Apps their runtime names, rather than failing the reading. Run the way
//! a reset credit is: the account's directory as `CODEX_HOME`, in two short conversations whose
//! requests are all sent up front, as `app/read` needs the ids `app/installed` gives. `codex mcp
//! list --json` is asked first which servers are turned off, as `mcpServerStatus/list` lists those
//! too and gives no sign of it.
//!
//! `app/list` is not asked: it answers only once Codex has fetched the whole public App
//! catalogue, page by page, which every new app server does and which takes half a minute or more.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::app_server::{self, CODEX_INVENTORY_APP_SERVER_ARGS};
use crate::accounts::mcp_inventory::{
    enabled_by_default, not_run_unavailable, Listing, McpInventoryReader,
};
use crate::adapter::account_cli;
use crate::agents::json_rpc;
use crate::processes::subprocess::{run_exchange, Bounds, ExchangeFailure};
use crate::protocol::{
    AgentId, ConnectorState, LocalMcpServer, McpInventoryFailure, McpInventoryUnavailable,
    McpRuntimeState, McpSignInState, PlatformConnector,
};

/// The most either conversation may print. `mcpServerStatus/list` carries the schema of every tool
/// of every server, the `codex_apps` server's tools for the Apps included, which runs past a
/// megabyte for an account with a few Apps (1.1 MB measured for 168 App tools); this leaves room
/// for many times that.
const OUTPUT_LIMIT: usize = 16 * 1024 * 1024;

/// The most servers `mcpServerStatus/list` is asked for: the conversation sends no follow-up
/// request, so there is no paging.
const LIST_LIMIT: u32 = 1000;

/// The most App ids one `app/read` takes.
const READ_BATCH: usize = 100;

/// The request ids of the first conversation, after `initialize`'s 1.
const ACCOUNT: i64 = 2;
const APP_INSTALLED: i64 = 3;
const SERVER_STATUS: i64 = 4;

/// The request id of the first `app/read` of the second conversation; each further batch takes
/// the next.
const APP_READ: i64 = 10;

/// How long `codex mcp list` may take, at most a quarter of the reading's time so that a listing
/// that hangs leaves the rest of it: it only reads configuration.
const CONFIGURED_TIMEOUT: Duration = Duration::from_secs(15);

/// The most `codex mcp list --json` may print: one short entry per configured server.
const CONFIGURED_OUTPUT_LIMIT: usize = 1024 * 1024;

/// The server Codex serves its Apps through; they are listed as connectors, not as a server.
const APPS_SERVER: &str = "codex_apps";

pub(super) struct CodexMcpInventory;

impl McpInventoryReader for CodexMcpInventory {
    fn read(
        &self,
        shell_env: &HashMap<String, String>,
        config_dir: Option<&Path>,
        refresh: bool,
        timeout: Duration,
        cancel: &AtomicBool,
    ) -> Result<Listing, McpInventoryUnavailable> {
        let deadline = Instant::now() + timeout;
        let left = || deadline.saturating_duration_since(Instant::now());
        let disabled = disabled_servers(shell_env, config_dir, timeout, cancel);
        let first = [
            request(ACCOUNT, "account/read", json!({})),
            request(
                APP_INSTALLED,
                "app/installed",
                json!({"forceRefresh": refresh}),
            ),
            request(
                SERVER_STATUS,
                "mcpServerStatus/list",
                json!({"detail": "toolsAndAuthOnly", "limit": LIST_LIMIT}),
            ),
        ];
        let replies = converse(shell_env, config_dir, &first, left(), cancel)?;
        let (installed, servers) = interpret(&replies, &disabled)?;
        let ids: Vec<&str> = installed.iter().map(|app| app.id.as_str()).collect();
        let details = if ids.is_empty() {
            Replies::new()
        } else {
            match converse(shell_env, config_dir, &reads(&ids), left(), cancel) {
                Ok(details) => details,
                // The names and plugins are an extra: Apps without them keep their runtime names.
                Err(_) if !cancel.load(Ordering::Relaxed) => Replies::new(),
                Err(unavailable) => return Err(unavailable),
            }
        };
        Ok((connectors(&installed, &details), servers))
    }
}

fn unavailable(reason: McpInventoryFailure, detail: Option<String>) -> McpInventoryUnavailable {
    McpInventoryUnavailable::new(reason, detail)
}

/// The arguments the app server is started with: the quiet ones that leave Apps and plugins on,
/// and, for a non-default account, its own store for MCP sign-ins, as a session of it is launched
/// with (`super::MCP_FILE_CREDENTIALS_STORE`); without it the reading would report the default
/// account's keychain sign-ins.
fn server_args(config_dir: Option<&Path>) -> Vec<&'static str> {
    let mut args = CODEX_INVENTORY_APP_SERVER_ARGS.to_vec();
    if config_dir.is_some() {
        args.extend(["-c", super::MCP_FILE_CREDENTIALS_STORE]);
    }
    args
}

/// The servers `codex mcp list --json` says are turned off, whether in the account's
/// configuration or by the plugin that brings them. Its entries also carry each server's command,
/// environment and headers, which are not read. A listing that cannot be had or read turns nothing
/// off, so the reading still lists every server `mcpServerStatus/list` gives.
fn disabled_servers(
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    timeout: Duration,
    cancel: &AtomicBool,
) -> HashSet<String> {
    #[derive(serde::Deserialize)]
    struct Configured {
        name: String,
        #[serde(default = "enabled_by_default")]
        enabled: bool,
    }
    let mut args = vec!["mcp", "list", "--json"];
    if config_dir.is_some() {
        args.extend(["-c", super::MCP_FILE_CREDENTIALS_STORE]);
    }
    account_cli::query(
        AgentId::CODEX,
        shell_env,
        config_dir,
        &args,
        CONFIGURED_TIMEOUT.min(timeout / 4),
        CONFIGURED_OUTPUT_LIMIT,
        Some(cancel),
    )
    .ok()
    .filter(|output| output.status.success())
    .and_then(|output| serde_json::from_slice::<Vec<Configured>>(&output.stdout).ok())
    .into_iter()
    .flatten()
    .filter(|server| !server.enabled)
    .map(|server| server.name)
    .collect()
}

/// The replies of a conversation, by request id.
type Replies = HashMap<i64, Value>;

/// A JSON-RPC request of the app server's protocol.
fn request(id: i64, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

/// The `app/read` requests for `ids`, in batches of at most [`READ_BATCH`], without the Apps'
/// tools.
fn reads(ids: &[&str]) -> Vec<Value> {
    ids.chunks(READ_BATCH)
        .zip(APP_READ..)
        .map(|(batch, id)| {
            request(
                id,
                "app/read",
                json!({"appIds": batch, "includeTools": false}),
            )
        })
        .collect()
}

/// Runs an app server for the account, sends it `requests` after the handshake, and returns the
/// reply to each once all have come.
fn converse(
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    requests: &[Value],
    timeout: Duration,
    cancel: &AtomicBool,
) -> Result<Replies, McpInventoryUnavailable> {
    let mut run = account_cli::admitted_command(
        AgentId::CODEX,
        shell_env,
        config_dir,
        &server_args(config_dir),
    )
    .map_err(|refused| not_run_unavailable(&refused))?;
    let ids: Vec<i64> = requests
        .iter()
        .filter_map(|request| request["id"].as_i64())
        .collect();
    let input: Vec<String> = [app_server::initialize(), app_server::initialized()]
        .iter()
        .chain(requests)
        .map(Value::to_string)
        .collect();
    let replies: RefCell<Replies> = RefCell::new(HashMap::new());
    let bounds = Bounds::agent_query(timeout, OUTPUT_LIMIT);
    run_exchange(&mut run.command, &input, &bounds, cancel, |line| {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return false;
        };
        let mut replies = replies.borrow_mut();
        if let Some(id) = ids.iter().find(|id| json_rpc::is_reply_to(&value, **id)) {
            replies.insert(*id, value);
        }
        replies.len() == ids.len()
    })
    .map_err(|failure| match failure {
        ExchangeFailure::TimedOut(_) => unavailable(McpInventoryFailure::TimedOut, None),
        other => unavailable(McpInventoryFailure::Failed, Some(other.to_string())),
    })?;
    Ok(replies.into_inner())
}

/// An App `app/installed` gives.
#[derive(Debug, PartialEq)]
struct Installed {
    id: String,
    /// The name the runtime's tool catalogue carries, for when `app/read` gives none.
    runtime_name: Option<String>,
    state: ConnectorState,
}

/// What the first conversation's replies say: the installed Apps and the servers, leaving out the
/// `disabled` servers.
fn interpret(
    replies: &Replies,
    disabled: &HashSet<String>,
) -> Result<(Vec<Installed>, Vec<LocalMcpServer>), McpInventoryUnavailable> {
    if replies.get(&ACCOUNT).is_some_and(signed_out) {
        return Err(unavailable(McpInventoryFailure::NotSignedIn, None));
    }
    let status = replies.get(&SERVER_STATUS);
    if let Some(detail) = status.and_then(json_rpc::error_detail) {
        return Err(unavailable(McpInventoryFailure::Failed, Some(detail)));
    }
    Ok((
        installed(replies.get(&APP_INSTALLED)),
        status
            .map(|reply| servers(reply, disabled))
            .unwrap_or_default(),
    ))
}

/// Whether `account/read` says nobody is signed in: no account, where one is required. An agent
/// configured to need none (`requiresOpenaiAuth` false) is not signed out.
fn signed_out(reply: &Value) -> bool {
    reply.pointer("/result/account").is_some_and(Value::is_null)
        && reply
            .pointer("/result/requiresOpenaiAuth")
            .and_then(Value::as_bool)
            == Some(true)
}

/// The Apps of an `app/installed` reply, in its order (Codex's own Apps settings list them in the
/// same order), each with the state its `enabled` and `callable` give.
fn installed(reply: Option<&Value>) -> Vec<Installed> {
    reply
        .and_then(|reply| reply.pointer("/result/apps"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|app| {
            let flag = |key: &str| app.get(key).and_then(Value::as_bool);
            Some(Installed {
                id: app.get("id").and_then(Value::as_str)?.to_string(),
                runtime_name: app
                    .get("runtimeName")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                state: connector_state(flag("enabled"), flag("callable")),
            })
        })
        .collect()
}

/// Codex's `enabled` (whether the App is turned on, after every configuration layer) and
/// `callable` (whether it is on and has a tool the model may call) as a connector state: on and
/// callable is connected, turned off is not connected, and on with nothing callable is unknown, as
/// whether it works cannot be told.
fn connector_state(enabled: Option<bool>, callable: Option<bool>) -> ConnectorState {
    match (enabled, callable) {
        (Some(true), Some(true)) => ConnectorState::Connected,
        (Some(false), _) => ConnectorState::NotConnected,
        _ => ConnectorState::Unknown,
    }
}

/// The installed Apps as connectors, each named, and given the plugins that bring it, as an
/// `app/read` reply among `details` has it; an App no reply describes keeps its runtime name, else
/// its id, and no plugins.
fn connectors(installed: &[Installed], details: &Replies) -> Vec<PlatformConnector> {
    let described: HashMap<&str, &Value> = details
        .values()
        .filter_map(|reply| reply.pointer("/result/apps")?.as_array())
        .flatten()
        .filter_map(|app| Some((app.get("id")?.as_str()?, app)))
        .collect();
    installed
        .iter()
        .map(|app| {
            let detail = described.get(app.id.as_str());
            let name = detail
                .and_then(|detail| detail.get("name"))
                .and_then(Value::as_str)
                .or(app.runtime_name.as_deref())
                .unwrap_or(&app.id);
            let plugins = detail
                .and_then(|detail| detail.get("pluginDisplayNames"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|name| name.as_str().map(str::to_string))
                .collect();
            PlatformConnector {
                id: app.id.clone(),
                name: name.to_string(),
                state: app.state,
                plugins,
            }
        })
        .collect()
}

/// The servers of a `mcpServerStatus/list` reply, `codex_apps`, the `disabled` ones and any Codex
/// itself reports as disabled left out.
fn servers(reply: &Value, disabled: &HashSet<String>) -> Vec<LocalMcpServer> {
    reply
        .pointer("/result/data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|server| {
            let name = server.get("name").and_then(Value::as_str)?;
            let runtime = server.get("runtimeStatus").and_then(Value::as_str);
            if name == APPS_SERVER || disabled.contains(name) || runtime == Some("disabled") {
                return None;
            }
            let text = |key: &str| server.get(key).and_then(Value::as_str).map(str::to_string);
            let tools_error = text("toolsError");
            let tool_count = match (&tools_error, server.get("tools").and_then(Value::as_object)) {
                (None, Some(tools)) => u32::try_from(tools.len()).ok(),
                _ => None,
            };
            let (sign_in, runtime, runtime_message) = server_states(
                server.get("authStatus").and_then(Value::as_str),
                runtime,
                tools_error.as_deref(),
                tool_count.is_some_and(|count| count > 0),
            );
            Some(LocalMcpServer {
                name: name.to_string(),
                origin: text("httpOrigin"),
                plugin: text("pluginId"),
                sign_in,
                runtime,
                runtime_message,
                tool_count,
                oauth_sign_in: oauth_sign_in(server.get("authStatus").and_then(Value::as_str)),
            })
        })
        .collect()
}

/// Whether the account signs in to a server with OAuth through `codex mcp login`, going by Codex's
/// `authStatus`: a server that has a token or can take one. A bearer token is configured, not
/// signed in to; `unsupported` and a value not known here offer no sign-in.
fn oauth_sign_in(auth: Option<&str>) -> bool {
    matches!(auth, Some("oAuth" | "notLoggedIn"))
}

/// Codex's `authStatus`, `runtimeStatus` and `toolsError` of a server as Octoboard's sign-in and
/// runtime states and the failure message: the one place Codex's values are mapped, where a value
/// not known here is `unknown`. Codex gives `runtimeStatus` only for a server a thread has
/// started, which a reading has none of, so it is normally absent: a server is then failed when
/// listing its tools failed, running when Codex `listed_tools` of it, as it must have started to
/// give them, and unknown otherwise.
fn server_states(
    auth: Option<&str>,
    runtime: Option<&str>,
    tools_error: Option<&str>,
    listed_tools: bool,
) -> (McpSignInState, McpRuntimeState, Option<String>) {
    let mut sign_in = match auth {
        Some("oAuth" | "bearerToken") => McpSignInState::SignedIn,
        Some("notLoggedIn") => McpSignInState::NeedsSignIn,
        Some("unsupported") => McpSignInState::NotNeeded,
        _ => McpSignInState::Unknown,
    };
    let failed = || (McpRuntimeState::Failed, tools_error.map(str::to_string));
    let (runtime, message) = match runtime {
        Some("connected") => (McpRuntimeState::Running, None),
        Some("failed" | "cancelled") => failed(),
        Some("authenticationRequired") => {
            sign_in = McpSignInState::NeedsSignIn;
            (McpRuntimeState::Unknown, None)
        }
        Some(_) => (McpRuntimeState::Unknown, None),
        None if tools_error.is_some() => failed(),
        None if listed_tools => (McpRuntimeState::Running, None),
        None => (McpRuntimeState::Unknown, None),
    };
    (sign_in, runtime, message)
}

#[cfg(test)]
mod tests;
