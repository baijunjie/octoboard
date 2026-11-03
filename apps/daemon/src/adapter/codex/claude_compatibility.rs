//! Claude compatibility for Codex: the environment policy for the MCP servers it is given from a
//! compatibility source.

use super::claude_mcp::Reserved;

/// How a compatibility source's MCP server's `env` may treat a variable name. `OCTOBOARD_*` is
/// Octoboard's own namespace (its header variables are in `plan.env`), the session variables and
/// `GIT_CONFIG_*` are set after `plan.env` in project sessions, and `TERM` is set last, so a server
/// naming one is refused. What steers Codex itself is `Held`: its authentication and behaviour
/// (`CODEX_*`, `OPENAI_*`), logging (`RUST_LOG`, `RUST_BACKTRACE`), network (the proxy variables in
/// both cases, `SSL_CERT_FILE`, `SSL_CERT_DIR`) and process (`NODE_OPTIONS`, `DYLD_*`, `LD_*`); a
/// server may name those only at the value the shell or `plan.env` already gives them.
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
