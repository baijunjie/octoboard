//! Claude compatibility for Codex: the instructions and the environment policy for Claude Code's
//! MCP servers.

use std::collections::HashMap;

use crate::adapter::claude_mcp::Reserved;
use crate::protocol::Agent;

/// The instructions that have Codex read Claude Code's configuration (`claude_compatibility.md`),
/// with `{CLAUDE_CONFIG_DIR}` filled in by [`claude_compatibility_instructions`].
const CLAUDE_COMPATIBILITY_INSTRUCTIONS: &str = include_str!("claude_compatibility.md");

/// The instructions that have Codex read Claude Code's instructions and skills, naming the
/// user-level configuration directory of Claude Code's default account as the launch's shell
/// snapshot resolves it (`~/.claude` when it names none).
pub(super) fn claude_compatibility_instructions(shell_env: &HashMap<String, String>) -> String {
    CLAUDE_COMPATIBILITY_INSTRUCTIONS.replace(
        "{CLAUDE_CONFIG_DIR}",
        &crate::adapter::default_account_dir(Agent::Claude, shell_env).to_string_lossy(),
    )
}

/// How a Claude Code MCP server's `env` may treat a variable name. `OCTOBOARD_*` is Octoboard's own
/// namespace (its header variables are in `plan.env`), the session variables and `GIT_CONFIG_*` are
/// set after `plan.env` in project sessions, and `TERM` is set last, so a server naming one is
/// refused. What steers Codex itself is `Held`: its authentication and behaviour (`CODEX_*`,
/// `OPENAI_*`), logging (`RUST_LOG`, `RUST_BACKTRACE`), network (the proxy variables in both cases,
/// `SSL_CERT_FILE`, `SSL_CERT_DIR`) and process (`NODE_OPTIONS`, `DYLD_*`, `LD_*`); a server may
/// name those only at the value the shell or `plan.env` already gives them.
pub(super) fn reserved_env(name: &str) -> Reserved {
    if name == "TERM"
        || ["GIT_CONFIG_", "OCTOBOARD_"]
            .iter()
            .any(|p| name.starts_with(p))
    {
        return Reserved::Refused;
    }
    let held_name = [
        "RUST_LOG",
        "RUST_BACKTRACE",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
        "NODE_OPTIONS",
    ]
    .contains(&name)
        || ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY"]
            .iter()
            .any(|proxy| name.eq_ignore_ascii_case(proxy))
        || ["CODEX_", "OPENAI_", "DYLD_", "LD_"]
            .iter()
            .any(|prefix| name.starts_with(prefix));
    if held_name {
        Reserved::Held
    } else {
        Reserved::No
    }
}
