//! Signing a Codex account in to and out of an MCP server that uses OAuth: `codex mcp login <name>`
//! and `codex mcp logout <name>`, run under the account's home. Codex opens the browser itself. A
//! non-default account's token goes to its own store, as its sessions and its inventory reading
//! use (`super::MCP_FILE_CREDENTIALS_STORE`); the default account keeps Codex's default one, as
//! `codex` in a terminal does. The server's name follows `--`, so that a name starting with `-` is
//! not read as an option.

use std::path::Path;

use crate::accounts::mcp_sign_in::{McpAuthAction, McpServerAuth};

pub(super) struct CodexMcpAuth;

impl McpServerAuth for CodexMcpAuth {
    fn args(&self, action: McpAuthAction, server: &str, config_dir: Option<&Path>) -> Vec<String> {
        let mut args = vec![
            "mcp".to_string(),
            match action {
                McpAuthAction::SignIn => "login",
                McpAuthAction::SignOut => "logout",
            }
            .to_string(),
        ];
        if config_dir.is_some() {
            args.extend([
                "-c".to_string(),
                super::MCP_FILE_CREDENTIALS_STORE.to_string(),
            ]);
        }
        args.extend(["--".to_string(), server.to_string()]);
        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_non_default_account_gets_its_own_credential_store_and_the_name_follows_a_separator() {
        let account = Path::new("/accounts/work");
        assert_eq!(
            CodexMcpAuth.args(McpAuthAction::SignIn, "-odd", Some(account)),
            [
                "mcp",
                "login",
                "-c",
                "mcp_oauth_credentials_store=\"file\"",
                "--",
                "-odd"
            ]
        );
        assert_eq!(
            CodexMcpAuth.args(McpAuthAction::SignOut, "linear", None),
            ["mcp", "logout", "--", "linear"]
        );
    }
}
