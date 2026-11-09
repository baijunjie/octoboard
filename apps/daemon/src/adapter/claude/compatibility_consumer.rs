//! Claude Code as a consumer of a compatibility source (Codex's, through the switch
//! `codex_compatibility`): the source's MCP servers merged into the launch's `--mcp-config`, and
//! its skills and custom prompts offered as a plugin directory of the session's scratch space.
//! The source's instructions are appended to the role text by `plan`.
//!
//! A server given with `--mcp-config` replaces a project's `.mcp.json` server of the same name
//! (Claude Code 2.1.319), so a name Claude Code defines itself is never taken from the source, and
//! with Claude Code's own definitions unknown (an unreadable file) no server is taken at all.
//! `${VAR}` and `${VAR:-default}` are expanded in the `args`, `env` values and `headers` of that
//! flag; that is how the source says "this variable of your environment" (see
//! `CompatibilitySource::mcp_servers`). A reference to a variable the launch's environment lacks is
//! settled as the source's agent would have: an `env` entry or header that is only that reference
//! is dropped, and a server whose other value refers to one is left out
//! (`compatibility_offer::translate`).
//!
//! Values that may be secrets stay off Claude Code's argv, which anyone on the machine can list: a
//! literal `env` or header value goes into the Claude Code process's environment under
//! `OCTOBOARD_CODEX_MCP_<n>_<m>`, `<n>` the server's position among the source's servers and `<m>`
//! the entry's within its `env` or `headers`, and the JSON refers to it. A value that already
//! refers to a variable stays as it is. `command`, `args` and `url` are on argv, which is
//! accepted: they are the server's own argv when it is started.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::compatibility::defined_server_names;
use crate::adapter::compatibility_offer::{is_name, offer_entries, translate};
use crate::adapter::{CompatibilitySource, LaunchSpec};

/// Prefix of the variables set on the Claude Code process to carry literal `env` and header values.
const VALUE_ENV_PREFIX: &str = "OCTOBOARD_CODEX_MCP_";

/// The name of the plugin the source's skills and prompts are offered as, so that they are invoked
/// as `codex-compat:<name>`; Claude Code takes it from the directory's name. Not `codex`, the name
/// of the plugin OpenAI ships for Claude Code.
const PLUGIN_DIR: &str = "codex-compat";

/// What a Claude Code launch adds for the source's servers.
#[derive(Debug, Default, PartialEq)]
pub(super) struct Servers {
    /// The servers to add to the `mcpServers` of `--mcp-config`, by name.
    pub servers: Map<String, Value>,
    /// Variables to set on the Claude Code process.
    pub env: Vec<(String, String)>,
}

/// The source's servers for this launch, less those that cannot be given (see the module's text).
pub(super) fn servers(
    source: &dyn CompatibilitySource,
    spec: &LaunchSpec<'_>,
    pinned: Option<&Path>,
) -> Servers {
    let mut out = Servers::default();
    let Some(own) = defined_server_names(spec.shell_env, pinned, spec.cwd) else {
        return out;
    };
    let from = source.mcp_servers(spec.shell_env, spec.cwd);
    for (index, (name, raw)) in from.into_iter().enumerate() {
        if name == crate::session_tools::catalogue::SERVER_KEY
            || own.contains(&name)
            || !is_name(&name)
        {
            continue;
        }
        let mut env = Vec::new();
        match translate(
            &raw,
            VALUE_ENV_PREFIX,
            index,
            Some(spec.shell_env),
            &mut env,
        ) {
            Some(server) => {
                out.servers.insert(name, server);
                out.env.extend(env);
            }
            None => {
                tracing::warn!(server = %name, "skipping a compatibility source's MCP server for Claude Code");
            }
        }
    }
    out
}

/// Builds the plugin directory of the source's skills and prompts in the session's scratch space,
/// afresh: `skills/<name>` a link to each skill directory and `commands/<name>.md` a link to each
/// prompt file, or a copy of it where the host makes no file links. `None` while there is
/// nothing to offer. An entry that cannot be made is left out.
pub(super) fn plugin_dir(
    source: &dyn CompatibilitySource,
    spec: &LaunchSpec<'_>,
) -> Option<PathBuf> {
    let skills = source.skill_dirs(spec.shell_env, spec.cwd);
    let commands = source.command_files(spec.shell_env);
    let dir = spec.scratch.join(PLUGIN_DIR);
    if let Err(err) = crate::hostfs::remove_any(&dir) {
        tracing::warn!(path = %dir.display(), %err, "could not clear the compatibility plugin directory");
        return None;
    }
    let file_links = crate::platform::fs::file_links_available();
    let made = offer_entries("Claude Code", &dir.join("skills"), skills, true, file_links)
        + offer_entries(
            "Claude Code",
            &dir.join("commands"),
            commands,
            false,
            file_links,
        );
    (made > 0).then_some(dir)
}
