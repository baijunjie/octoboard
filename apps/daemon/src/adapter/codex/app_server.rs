//! What every conversation Octoboard holds with `codex app-server` shares: how the server is
//! started for an account outside a session (a quota probe, the use of a reset credit, a sign-in),
//! the overrides that keep Codex quiet on such a start, and the `initialize` handshake that opens
//! the conversation, which a session's private app server ([`super::transport`]) opens with too.

use serde_json::{json, Value};

/// The Codex configuration overrides that keep it from reaching out on start: the plugin catalogue
/// sync (which runs `git ls-remote` over SSH), apps, and the user's hooks. Set as configuration
/// rather than `--disable`, which refuses a feature name the installed Codex does not know, where
/// an unknown configuration key is ignored.
pub(crate) const CODEX_QUIET_FEATURES: &[&str] = &[
    "-c",
    "features.plugins=false",
    "-c",
    "features.remote_plugin=false",
    "-c",
    "features.apps=false",
    "-c",
    "features.hooks=false",
];

/// How Codex's app server is started for an account outside a session.
pub(crate) const CODEX_APP_SERVER_ARGS: &[&str] = &APP_SERVER_ARGS;

const APP_SERVER_ARGS: [&str; 1 + CODEX_QUIET_FEATURES.len()] = {
    let mut args = ["app-server"; 1 + CODEX_QUIET_FEATURES.len()];
    let mut i = 0;
    while i < CODEX_QUIET_FEATURES.len() {
        args[i + 1] = CODEX_QUIET_FEATURES[i];
        i += 1;
    }
    args
};

/// The parameters of the `initialize` request: who the client is. A caller that needs more of the
/// server adds its own capabilities to them.
pub(crate) fn initialize_params() -> Value {
    json!({"clientInfo": {"name": "octoboard", "version": env!("CARGO_PKG_VERSION")}})
}

/// The `initialize` request that opens a conversation, as id 1.
pub(crate) fn initialize() -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": initialize_params()})
}

/// The `initialized` notification that follows the reply to `initialize`.
pub(crate) fn initialized() -> Value {
    json!({"jsonrpc": "2.0", "method": "initialized"})
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The handshake as Codex is sent it.
    #[test]
    fn the_handshake_names_octoboard_and_its_version() {
        assert_eq!(
            initialize(),
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {"clientInfo": {"name": "octoboard",
                    "version": env!("CARGO_PKG_VERSION")}}})
        );
        assert_eq!(
            initialized(),
            json!({"jsonrpc": "2.0", "method": "initialized"})
        );
    }

    #[test]
    fn the_app_server_starts_with_the_quiet_overrides() {
        assert_eq!(CODEX_APP_SERVER_ARGS[0], "app-server");
        assert_eq!(&CODEX_APP_SERVER_ARGS[1..], CODEX_QUIET_FEATURES);
    }
}
