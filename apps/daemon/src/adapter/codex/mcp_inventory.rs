//! Reading a Codex account's MCP servers and Apps: `mcpServerStatus/list`, `app/list` and
//! `app/installed` on `codex app-server`, after `account/read` says whether anyone is signed in.
//! The methods are experimental, so every field is read defensively and an `app/list` or
//! `app/installed` the installed Codex does not know leaves its part empty rather than failing the
//! reading. Run the way a reset credit is: the account's directory as `CODEX_HOME`, one short
//! conversation whose requests are all sent up front.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use serde_json::{json, Value};

use super::app_server::{self, CODEX_INVENTORY_APP_SERVER_ARGS};
use crate::accounts::mcp_inventory::{Listing, McpInventoryReader};
use crate::accounts::quotas::probe::OUTPUT_LIMIT;
use crate::adapter::account_cli::{self, NotRun};
use crate::agents::json_rpc;
use crate::processes::subprocess::{run_exchange, Bounds, ExchangeFailure};
use crate::protocol::{
    AgentId, ConnectorState, LocalMcpServer, McpInventoryFailure, McpInventoryUnavailable,
    McpRuntimeState, McpSignInState, PlatformConnector,
};

/// The most each list is asked for: the conversation sends no follow-up request, so there is no
/// paging.
const LIST_LIMIT: u32 = 1000;

/// The request ids of the reading, after `initialize`'s 1.
const ACCOUNT: i64 = 2;
const APP_LIST: i64 = 3;
const APP_INSTALLED: i64 = 4;
const SERVER_STATUS: i64 = 5;

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
        let replies = converse(shell_env, config_dir, refresh, timeout, cancel)?;
        interpret(&replies)
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

/// The replies to requests 2 to 5, by id.
type Replies = HashMap<i64, Value>;

/// A JSON-RPC request of the app server's protocol.
fn request(id: i64, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

fn converse(
    shell_env: &HashMap<String, String>,
    config_dir: Option<&Path>,
    refresh: bool,
    timeout: Duration,
    cancel: &AtomicBool,
) -> Result<Replies, McpInventoryUnavailable> {
    let mut run = account_cli::admitted_command(
        AgentId::CODEX,
        shell_env,
        config_dir,
        &server_args(config_dir),
    )
    .map_err(|refused| match refused {
        NotRun::BinaryNotFound(_) | NotRun::BinaryRefused(_) => {
            unavailable(McpInventoryFailure::CliMissing, Some(refused.detail()))
        }
        NotRun::DirMissing(_) => unavailable(McpInventoryFailure::Failed, Some(refused.detail())),
    })?;
    let input: Vec<String> = [
        app_server::initialize(),
        app_server::initialized(),
        request(ACCOUNT, "account/read", json!({})),
        request(
            APP_LIST,
            "app/list",
            json!({"limit": LIST_LIMIT, "forceRefetch": refresh}),
        ),
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
    ]
    .iter()
    .map(Value::to_string)
    .collect();
    let replies: RefCell<Replies> = RefCell::new(HashMap::new());
    let bounds = Bounds::agent_query(timeout, OUTPUT_LIMIT);
    run_exchange(&mut run.command, &input, &bounds, cancel, |line| {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return false;
        };
        let mut replies = replies.borrow_mut();
        for id in [ACCOUNT, APP_LIST, APP_INSTALLED, SERVER_STATUS] {
            if json_rpc::is_reply_to(&value, id) {
                replies.insert(id, value);
                break;
            }
        }
        replies.len() == 4
    })
    .map_err(|failure| match failure {
        ExchangeFailure::TimedOut(_) => unavailable(McpInventoryFailure::TimedOut, None),
        other => unavailable(McpInventoryFailure::Failed, Some(other.to_string())),
    })?;
    Ok(replies.into_inner())
}

/// What the replies say, once the conversation is over.
fn interpret(replies: &Replies) -> Result<Listing, McpInventoryUnavailable> {
    if replies.get(&ACCOUNT).is_some_and(signed_out) {
        return Err(unavailable(McpInventoryFailure::NotSignedIn, None));
    }
    let status = replies.get(&SERVER_STATUS);
    if let Some(detail) = status.and_then(json_rpc::error_detail) {
        return Err(unavailable(McpInventoryFailure::Failed, Some(detail)));
    }
    let apps = replies.get(&APP_LIST).filter(|reply| succeeded(reply));
    let installed = replies.get(&APP_INSTALLED).filter(|reply| succeeded(reply));
    Ok((
        connectors(apps, installed),
        status.map(servers).unwrap_or_default(),
    ))
}

fn succeeded(reply: &Value) -> bool {
    json_rpc::error_detail(reply).is_none()
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

/// The Apps `app/list` names, each with the state `isAccessible` gives. The catalogue lists every
/// App there is, so one is kept only when the account can reach it, a plugin brings it, or
/// `app/installed` has it.
fn connectors(list: Option<&Value>, installed: Option<&Value>) -> Vec<PlatformConnector> {
    let installed: HashSet<&str> = installed
        .and_then(|reply| reply.pointer("/result/apps"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|app| app.get("id").and_then(Value::as_str))
        .collect();
    list.and_then(|reply| reply.pointer("/result/data"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|app| {
            let id = app.get("id").and_then(Value::as_str)?;
            let accessible = app.get("isAccessible").and_then(Value::as_bool);
            let plugins: Vec<String> = app
                .get("pluginDisplayNames")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|name| name.as_str().map(str::to_string))
                .collect();
            if accessible != Some(true) && plugins.is_empty() && !installed.contains(id) {
                return None;
            }
            Some(PlatformConnector {
                id: id.to_string(),
                name: app
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(id)
                    .to_string(),
                state: connector_state(accessible),
                plugins,
            })
        })
        .collect()
}

/// Codex's `isAccessible` as a connector state.
fn connector_state(accessible: Option<bool>) -> ConnectorState {
    match accessible {
        Some(true) => ConnectorState::Connected,
        Some(false) => ConnectorState::NotConnected,
        None => ConnectorState::Unknown,
    }
}

/// The servers of a `mcpServerStatus/list` reply, `codex_apps` left out.
fn servers(reply: &Value) -> Vec<LocalMcpServer> {
    reply
        .pointer("/result/data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|server| {
            let name = server.get("name").and_then(Value::as_str)?;
            if name == APPS_SERVER {
                return None;
            }
            let text = |key: &str| server.get(key).and_then(Value::as_str).map(str::to_string);
            let tools_error = text("toolsError");
            let (sign_in, runtime, runtime_message) = server_states(
                server.get("authStatus").and_then(Value::as_str),
                server.get("runtimeStatus").and_then(Value::as_str),
                tools_error.as_deref(),
            );
            let tool_count = match (&tools_error, server.get("tools").and_then(Value::as_object)) {
                (None, Some(tools)) => u32::try_from(tools.len()).ok(),
                _ => None,
            };
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
/// not known here is `unknown`.
fn server_states(
    auth: Option<&str>,
    runtime: Option<&str>,
    tools_error: Option<&str>,
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
        None => (McpRuntimeState::Running, None),
    };
    (sign_in, runtime, message)
}

#[cfg(test)]
mod tests;
