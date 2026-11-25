//! OpenCode's launch integration. The baseline and the OpenCode 2 measurements are in
//! `docs/agent-cli-reference.md`, section "OpenCode".
//!
//! Both generations take `OPENCODE_CONFIG_CONTENT`: a whole
//! configuration as inline JSON, which OpenCode merges over the user's global and project
//! configuration, so it wins where they disagree and leaves the rest of theirs in force. It holds
//! the Octoboard MCP server, the role text as an instruction file and Octoboard's plugin, the last
//! two as paths of files in the session's scratch directory. Where the user's shell already
//! carries a configuration in that variable, Octoboard's is merged over it, and the plugin hands
//! the user's own back to every shell the agent starts.
//!
//! On 1.x the launch's permission rules are not in that value: a key keeps the place it first had when
//! OpenCode merges one configuration over another, so an inline rule cannot be placed beneath a
//! rule of a file's or right after its `*`. The plugin writes them instead, into the configuration
//! OpenCode hands its `config` hook once everything is merged, in memory only (`writeRules` in
//! `plugin.js`): under the `ask` approval mode asking as the default beneath the user's own rules,
//! and the rule that allows Octoboard's own tools right after the default, where the default does
//! not ask for them and a rule of the user's that names one of them still holds.
//!
//! `OPENCODE_CONFIG_DIR` is not used: OpenCode installs its plugin package into whatever
//! directory that names, on every launch, and a user who sets it would lose their own.
//!
//! OpenCode has no hooks. Its status comes from the plugin, which runs inside its process and
//! forwards a few of its events through the session's hook script (`plugin.js`, read by
//! [`hooks`]). No id can be given to a new session: the first event names it.
//!
//! The cross-project guard goes through the same plugin: OpenCode has no hook that can refuse an
//! edit, but an error a plugin throws from `tool.execute.before` refuses the call before it runs,
//! ahead of any permission prompt, and the model is handed the error's message as the tool's
//! result (1.19.6). The plugin runs the launch's guard script for each call of a tool that writes
//! a file ([`hooks::EDIT_TOOLS`]) and throws the refusal's reason where it refuses.

mod accounts;
mod hooks;
mod interaction;
mod mcp_inventory;
mod models;
mod records;
mod screen;
mod server;
#[cfg(test)]
mod tests;
mod version;

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::{json, Map, Value};

use super::files::{canonical_project, read_bounded_text, read_regular_text, SMALL_FILE_LIMIT};
use super::user_json::{self, Takes};
use super::{AgentProvider, CliPackaging, LaunchPlan, LaunchSpec, MCP_TOOL_TIMEOUT_SECS};
use crate::hostfs::replace::{replace_beside, Permissions};
use crate::protocol::{CodedError, ProviderOperation};
use crate::providers::{AgentId, ProviderDescriptor};
use crate::session::screen::ScreenReader;
use crate::session_tools;
use version::Generation;

/// The variable that carries a whole configuration as inline JSON, merged over every file's.
const CONFIG_CONTENT_VAR: &str = "OPENCODE_CONFIG_CONTENT";
const CLAUDE_COMPATIBILITY_VARS: [&str; 3] = [
    "OPENCODE_DISABLE_CLAUDE_CODE",
    "OPENCODE_DISABLE_CLAUDE_CODE_PROMPT",
    "OPENCODE_DISABLE_CLAUDE_CODE_SKILLS",
];
/// The variable that turns off the title the session startup reads.
const TITLE_OFF_VAR: &str = "OPENCODE_DISABLE_TERMINAL_TITLE";
/// Where the files of the injection are written in the session's scratch directory.
const INJECTION_DIR: &str = "opencode";
const PLUGIN_FILE: &str = "octoboard.js";
/// What differs per session, read by the plugin from beside itself.
const PLUGIN_DATA_FILE: &str = "octoboard.json";
const ROLE_FILE: &str = "octoboard-role.md";
const PLUGIN_SOURCE: &str = include_str!("plugin.js");
const TUI_CONFIG_VAR: &str = "OPENCODE_TUI_CONFIG";
const TUI_PLUGIN_SOURCE: &str = include_str!("tui.mjs");
const V2_PLUGIN_SOURCE: &str = include_str!("plugin-v2.js");
const V2_TUI_SOURCE: &str = include_str!("tui-v2.js");
const CLI_CONFIG_VAR: &str = "OPENCODE_CLI_CONFIG_CONTENT";
const ORIGINAL_CLI_CONFIG: &str = "OCTOBOARD_OPENCODE_ORIGINAL_CLI_CONFIG";
const ORIGINAL_CONFIG: &str = "OCTOBOARD_OPENCODE_ORIGINAL_CONFIG";
/// The title OpenCode shows outside a session, and what it puts before a session's title.
const BARE_TITLE: &str = "OpenCode";
const TITLE_PREFIX: &str = "OC | ";
/// What OpenCode names a session until it has generated a title for it, followed by a timestamp.
const DEFAULT_TITLE_PREFIX: &str = "New session - ";
/// What OpenCode's reader of a configuration takes beyond JSON, in a file and in the inline value
/// alike (1.19.6): comments and a comma after the last element or member. It refuses a byte order
/// mark, and a comma that follows no value (`[,]`, `{,}`, `[1,,]`).
const CONFIG_TAKES: Takes = Takes {
    byte_order_mark: false,
    comments: true,
    trailing_commas: true,
    leading_comment_lines: false,
};
/// A permission pattern that matches every tool, as `*` does. The rule for Octoboard's tools is
/// written under `octoboard_**`, a key of its own: the plugin writes that key right after the
/// merged rules' `*`, and a rule of the user's for `octoboard_*` stays where they wrote it.
const EVERY_TOOL: &str = "**";
/// The approval mode whose default the plugin writes, and the action it stands for.
const ASK_MODE: &str = "ask";
/// How OpenCode writes a variable of its environment into a configuration: `{env:NAME}`.
const ENV_TOKEN: &str = "{env:";
/// How OpenCode writes the content of a file into a configuration: `{file:path}`.
const FILE_TOKEN: &str = "{file:";
/// The variable OpenCode takes the home directory of a `{file:~/…}` from.
const HOME_VAR: &str = "HOME";
/// The most bytes read from a file a `{file:…}` outside a string names. Only a file holding a
/// JSON value with no string in it (a number, `true`, `false`) reads as JSON there.
const VALUE_FILE_LIMIT: usize = 64 * 1024;

pub struct OpenCodeProvider;

impl AgentProvider for OpenCodeProvider {
    #[cfg(test)]
    fn launch_fixture_version(&self) -> Option<super::AgentVersion> {
        crate::providers::opencode::SUPPORTED_VERSIONS
            .last()
            .copied()
    }

    fn descriptor(&self) -> &'static ProviderDescriptor {
        AgentId::OPENCODE.descriptor()
    }

    fn hooks(&self) -> &'static dyn crate::hooks::HookInterpreter {
        &hooks::OpenCodeHooks
    }

    fn interactions(&self) -> Option<&'static dyn crate::session::interaction::InteractionReader> {
        Some(&interaction::OpenCodeInteractions)
    }

    fn has_status_integration(&self) -> bool {
        true
    }

    fn records(&self) -> &'static dyn crate::records::RecordStore {
        &records::OpenCodeRecords
    }

    fn prompt_text(&self, line: &str) -> Option<String> {
        records::prompt_text(line)
    }

    fn rebuild_launch(&self, launch: super::AdoptedLaunch<'_>) -> super::RebuiltLaunch {
        super::RebuiltLaunch {
            transport: server::adopt(launch),
            ..Default::default()
        }
    }

    fn screen(&self) -> &'static dyn ScreenReader {
        &screen::OpenCodeScreen
    }

    fn accounts(&self) -> Option<&'static dyn crate::accounts::AccountOps> {
        Some(&accounts::OpenCodeAccounts)
    }

    fn catalogue(&self) -> &'static dyn crate::agents::models::ModelCatalogue {
        &models::OpenCodeCatalogue
    }

    /// Its own installer puts it under `~/.opencode/bin`, and `opencode upgrade` updates an
    /// install of any method itself.
    fn cli_packaging(&self) -> &'static CliPackaging {
        &CliPackaging {
            native_layouts: &[&[".opencode", "bin"]],
            native_markers: &[],
            native_update: &["upgrade"],
            native_latest: None,
            native_manifest: None,
            npm_packages: NPM_PACKAGES,
            homebrew_formula: Some(HOMEBREW_FORMULA),
            homebrew_cask: None,
        }
    }

    fn native_binary(
        &self,
        binary: &crate::platform::shell::ResolvedBinary,
    ) -> Result<Option<crate::platform::shell::ResolvedBinary>> {
        use crate::platform::shell::{self, ResolvedBinary, Shim};

        if !binary
            .program
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("cmd"))
        {
            return Ok(None);
        }
        for package in NPM_PACKAGES {
            if let Shim::Package(directory) = shell::shim_package(binary, package) {
                let program = directory.join("bin").join("opencode.exe");
                if program.is_file() {
                    return Ok(Some(ResolvedBinary::new(program)));
                }
            }
        }
        Ok(None)
    }

    /// Read from where the binary is installed (1.19.6): an npm install's program is
    /// `opencode-ai/bin/opencode.exe`, a copy of the platform package's
    /// (`opencode-ai/node_modules/opencode-<os>-<arch>…/bin/opencode`), and either package's
    /// `package.json`, two levels up, names the version; a Homebrew install's program is
    /// `Cellar/opencode/<version>/bin/opencode`. Its own installer writes the program alone into
    /// `~/.opencode/bin`, with nothing beside it that names a version, so that install is `None`
    /// and its version is asked of the program.
    fn installed_version(&self, binary: &str) -> Option<super::AgentVersion> {
        let resolved = crate::hostfs::canonical(binary).ok()?;
        let package = resolved.parent()?.parent()?;
        if let Some(version) = package_version(package) {
            return Some(version);
        }
        let formula = package.parent()?;
        if formula.file_name()? == HOMEBREW_FORMULA && formula.parent()?.file_name()? == "Cellar" {
            let name = package.file_name()?.to_str()?;
            // A revision of the formula follows the version after `_`.
            return super::parse_version(name.split('_').next()?);
        }
        None
    }

    /// `OC | <title>` in a session; `OpenCode` alone, and the name a session has before OpenCode
    /// has generated its title, carry none.
    fn task_title(&self, raw: &str, _project_names: &[String]) -> Option<String> {
        let title = raw.trim().strip_prefix(TITLE_PREFIX)?.trim();
        (!title.is_empty() && title != BARE_TITLE && !title.starts_with(DEFAULT_TITLE_PREFIX))
            .then(|| title.to_owned())
    }

    fn plan(&self, spec: &LaunchSpec<'_>) -> Result<LaunchPlan> {
        // OpenCode has the default account only: no variable moves its data directory alone, so a
        // launch given an account's directory would run as the default account under another
        // name. Refused before anything is written.
        if spec.config_dir.is_some() {
            return Err(CodedError::provider_unsupported(
                AgentId::OPENCODE,
                ProviderOperation::OwnedAccounts,
            ));
        }
        let generation = version::at_launch(spec)?;
        let private = generation == Generation::V2
            || !spec.shell_env.contains_key("OPENCODE_SERVER_PASSWORD");
        let mut args = Vec::new();
        if generation == Generation::V2 {
            args.push("--standalone".to_owned());
        }
        if let Some(id) = spec.resume_agent_session_id {
            if generation == Generation::V2 {
                records::check_resume(spec, id)?;
            }
            args.push("--session".to_string());
            args.push(id.to_string());
        }
        if let Some(model) = spec.model.filter(|_| generation == Generation::V1) {
            args.push("--model".to_string());
            args.push(model.to_string());
        }
        // The TUI has no reasoning-effort flag, so an effort is not passed.
        // `--auto` holds for this process only. `ask` is a default the plugin writes
        // ([`write_injection`]), and no choice leaves OpenCode's own configuration to decide.
        if spec.settings.choice("approval_mode") == Some("auto") {
            args.push("--auto".to_string());
        }
        // Keep the first task in Octoboard's queue until the attached or standalone TUI is ready.
        let first_message = spec
            .resume_agent_session_id
            .is_none()
            .then(|| spec.task.map(str::to_owned))
            .flatten();

        let user_content = spec
            .shell_env
            .get(CONFIG_CONTENT_VAR)
            .filter(|content| !content.trim().is_empty());
        // The start of its session is read off its title, so a shell that turns the title off is
        // overruled for this process; the plugin hands the shell's own value back to the shells
        // the agent starts.
        let user_title_off = spec.shell_env.get(TITLE_OFF_VAR);
        let injection = write_injection(
            spec,
            user_title_off.map(String::as_str),
            generation,
            private,
        )?;
        let mut env = vec![(
            CONFIG_CONTENT_VAR.to_string(),
            config_content(
                spec,
                &injection,
                user_content.map(String::as_str),
                generation,
            ),
        )];
        env.push((
            ORIGINAL_CONFIG.into(),
            user_content.cloned().unwrap_or_default(),
        ));
        if generation == Generation::V2 {
            env.push((CLI_CONFIG_VAR.into(), v2_cli_config(spec.shell_env)));
            env.push((
                ORIGINAL_CLI_CONFIG.into(),
                spec.shell_env
                    .get(CLI_CONFIG_VAR)
                    .cloned()
                    .unwrap_or_default(),
            ));
            if spec.settings.choice("approval_mode") == Some(ASK_MODE) {
                env.push(("OCTOBOARD_OPENCODE_APPROVAL".into(), ASK_MODE.into()));
            }
        }
        let disable = (!spec.settings.switch("claude_compatibility")).to_string();
        env.extend(CLAUDE_COMPATIBILITY_VARS.map(|name| (name.to_owned(), disable.clone())));
        // An empty value reads as unset.
        if user_title_off.is_some() {
            env.push((TITLE_OFF_VAR.to_string(), String::new()));
        }
        let adoption = injection.attachment.as_ref().map(|attachment| {
            let mut metadata = json!({"opencode_route": path_text(&attachment.route)});
            if generation == Generation::V2 {
                metadata["opencode_api"] = json!(2);
            }
            metadata
        });
        let private_server = private.then(|| {
            Box::new(server::OpenCodeServer {
                generation,
                resume: spec.resume_agent_session_id.map(str::to_owned),
                model: spec
                    .model
                    .filter(|_| generation == Generation::V1)
                    .map(str::to_owned),
                auto: spec.settings.choice("approval_mode") == Some("auto"),
                attachment: injection.attachment,
            }) as Box<dyn crate::session::term::PrivateServer>
        });
        Ok(LaunchPlan {
            args,
            env,
            first_message,
            private_server,
            adoption,
            ..LaunchPlan::default()
        })
    }
}

const NPM_PACKAGES: &[&str] = &["opencode-ai", "@opencode/cli"];

/// How the names of the platform packages that ship OpenCode's binary begin.
const PLATFORM_PACKAGE_PREFIXES: [&str; 6] = [
    "opencode-darwin-",
    "opencode-linux-",
    "opencode-windows-",
    "@opencode/cli-darwin-",
    "@opencode/cli-linux-",
    "@opencode/cli-windows-",
];
/// The Homebrew formula OpenCode is installed as.
const HOMEBREW_FORMULA: &str = "opencode";

/// The version the `package.json` in `package` names, where it is OpenCode's npm package or one of
/// its platform packages.
fn package_version(package: &Path) -> Option<super::AgentVersion> {
    let text = read_regular_text(&package.join("package.json"), SMALL_FILE_LIMIT).ok()?;
    let manifest: Value = serde_json::from_str(&text).ok()?;
    let name = manifest.get("name")?.as_str()?;
    let ours = NPM_PACKAGES.contains(&name)
        || PLATFORM_PACKAGE_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix));
    ours.then(|| super::parse_version(manifest.get("version")?.as_str()?))
        .flatten()
}

/// The files of one launch's injection, in the session's scratch directory.
struct Injection {
    plugin: std::path::PathBuf,
    role: std::path::PathBuf,
    attachment: Option<server::Attachment>,
}

/// Writes the plugin, its data file and the role text into the session's scratch directory,
/// afresh for every launch, a resume included. `user_content` and `user_title_off` are what the
/// user's shell carries in `OPENCODE_CONFIG_CONTENT` and `OPENCODE_DISABLE_TERMINAL_TITLE`, which
/// the launch replaces for OpenCode's own process and the plugin gives back to the shells the
/// agent starts. The data file also carries the permission rules the plugin writes into the
/// configuration OpenCode merged: the pattern of Octoboard's own tools, allowed right after the
/// rules' default, and under the `ask` approval mode the action that becomes that default beneath
/// the user's rules; and the cross-project guard's script, with the tools it is asked about.
fn write_injection(
    spec: &LaunchSpec<'_>,
    user_title_off: Option<&str>,
    generation: Generation,
    private: bool,
) -> Result<Injection> {
    let dir = spec.scratch.join(INJECTION_DIR);
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let write = |name: &str, bytes: &[u8]| -> Result<std::path::PathBuf> {
        let path = dir.join(name);
        replace_beside(&path, bytes, Permissions::Private)
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(path)
    };
    let descriptor = AgentId::OPENCODE.descriptor();
    // The name the server's tools reach the model under, which is also the name OpenCode asks
    // permission for.
    let tools_rule = format!(
        "{}{}{}{EVERY_TOOL}",
        descriptor.tool_name_prefix,
        session_tools::catalogue::SERVER_KEY,
        descriptor.tool_name_separator
    );
    let default_action =
        (spec.settings.choice("approval_mode") == Some(ASK_MODE)).then_some(ASK_MODE);
    let guard_tools: Vec<&str> = hooks::EDIT_TOOLS.iter().map(|(tool, _)| *tool).collect();
    let user_claude_env: HashMap<_, _> = CLAUDE_COMPATIBILITY_VARS
        .into_iter()
        .map(|name| (name, spec.shell_env.get(name).map_or("", String::as_str)))
        .collect();
    let data = json!({
        // This script carries the launch ID on every event the plugin forwards.
        "hook": spec.hook_script.to_string_lossy(),
        // The cross-project guard's script, which the plugin runs for a call of one of the tools,
        // when the launch has a guard.
        "guard": spec.guard.as_ref().map(|guard| path_text(guard.script)),
        "guard_tools": guard_tools,
        "guard_timeout_ms": super::GUARD_HOOK_TIMEOUT_SECS * 1000,
        "user_title_off": user_title_off,
        "user_claude_env": user_claude_env,
        "default_action": default_action,
        "tools_rule": tools_rule,
    });
    write(PLUGIN_DATA_FILE, data.to_string().as_bytes())?;
    let role = session_tools::role::role_description(
        spec.kind,
        spec.owner.as_ref(),
        AgentId::OPENCODE,
        &spec.role_options(),
    );
    let plugin = write(PLUGIN_FILE, PLUGIN_SOURCE.as_bytes())?;
    let attachment = if generation == Generation::V2 {
        let dir = dir.join(spec.launch_id);
        std::fs::create_dir_all(&dir).context("creating the OpenCode plugin directory")?;
        replace_beside(
            &dir.join("index.js"),
            V2_PLUGIN_SOURCE.as_bytes(),
            Permissions::Private,
        )?;
        replace_beside(
            &dir.join("tui.js"),
            V2_TUI_SOURCE.as_bytes(),
            Permissions::Private,
        )?;
        Some(server::Attachment {
            config: None,
            route: dir.join("attached-session"),
        })
    } else if private
        && spec
            .shell_env
            .get(TUI_CONFIG_VAR)
            .is_none_or(String::is_empty)
    {
        let dir = dir.join(spec.launch_id);
        std::fs::create_dir_all(&dir).context("creating the OpenCode attachment directory")?;
        let plugin = dir.join("attachment.mjs");
        replace_beside(&plugin, TUI_PLUGIN_SOURCE.as_bytes(), Permissions::Private)?;
        let config = dir.join("tui.json");
        let content = json!({"plugin": [path_text(&plugin)]}).to_string();
        replace_beside(&config, content.as_bytes(), Permissions::Private)?;
        Some(server::Attachment {
            config: Some(config),
            route: dir.join("attached-session"),
        })
    } else {
        None
    };
    let plugin = match generation {
        Generation::V1 => plugin,
        Generation::V2 => dir.join(spec.launch_id),
    };
    Ok(Injection {
        attachment,
        plugin,
        role: write(ROLE_FILE, role.as_bytes())?,
    })
}

/// The value of `OPENCODE_CONFIG_CONTENT` for this launch: Octoboard's configuration, merged over
/// the user's own value of the variable ([`user_config`]). It carries no permission rule of the
/// launch's: the plugin writes those ([`write_injection`]), and a `permission` of the user's own
/// value goes through as they wrote it.
fn config_content(
    spec: &LaunchSpec<'_>,
    injection: &Injection,
    user_content: Option<&str>,
    generation: Generation,
) -> String {
    let (command, args) = super::mcp_server_command(spec);
    let mut server_command = vec![command];
    server_command.extend(args);
    let key = session_tools::catalogue::SERVER_KEY;
    let timeout = MCP_TOOL_TIMEOUT_SECS * 1000;
    let mcp = match generation {
        Generation::V1 => json!({ key: {
            "type": "local",
            "command": server_command,
            "enabled": true,
            "timeout": timeout,
        }}),
        Generation::V2 => json!({ "servers": { key: {
            "type": "local",
            "command": server_command,
            "disabled": false,
            "codemode": false,
            "protocol": "legacy",
            "timeout": { "startup": timeout, "catalog": timeout, "execution": timeout },
        }}}),
    };
    let mut ours = json!({
        "mcp": mcp,
        "instructions": [path_text(&injection.role)],
        (generation.plugin_key()): [path_text(&injection.plugin)],
    });
    if let Some(model) = spec.model.filter(|_| generation == Generation::V2) {
        ours["model"] = json!(model);
    }
    let mut merged = user_content
        .and_then(|content| user_config(content, spec.shell_env, spec.cwd))
        .unwrap_or_else(|| Value::Object(Map::new()));
    merge_over(&mut merged, ours);
    merged.to_string()
}

// The client reads the window title during startup. The v2 TUI reads this option from its
// own inline CLI configuration, rather than the server's configuration or a title variable.
fn v2_cli_config(env: &HashMap<String, String>) -> String {
    let mut config = env
        .get(CLI_CONFIG_VAR)
        .and_then(|text| user_json::parse(text, CONFIG_TAKES).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    merge_over(&mut config, json!({ "terminal": { "title": true } }));
    config.to_string()
}

/// The user's value of `OPENCODE_CONFIG_CONTENT` as the object OpenCode reads it as: comments and
/// trailing commas allowed, as in a configuration file ([`CONFIG_TAKES`]). Before it parses the
/// text OpenCode puts the value of a variable in place of every `{env:NAME}` and the content of a
/// file in place of every `{file:path}`, in a string or outside one. A value that reads as it is
/// written is kept as written, its tokens left for OpenCode to put in; one that is JSON only once
/// they are in place (a token outside a string) is read with the variables in place
/// ([`with_variables`]), from `env`, the environment the launch gives OpenCode, and with the files
/// named outside a string in place ([`with_files`]), a relative path taken from `cwd`. OpenCode
/// reads what is handed over once more, so there a value of a variable that itself holds a
/// `{env:…}` has it put in too, which OpenCode alone leaves as text. `None` for a value that is
/// no object either way, or that names a file outside a string that cannot be read: one OpenCode
/// itself refuses to start with.
fn user_config(content: &str, env: &HashMap<String, String>, cwd: &Path) -> Option<Value> {
    let read = |text: &str| {
        user_json::parse(text, CONFIG_TAKES)
            .ok()
            .filter(Value::is_object)
    };
    if let Some(config) = read(content) {
        return Some(config);
    }
    // A `{file:…}` can come out of a variable's value too, so the variables go in first, as in
    // OpenCode.
    if !content.contains(ENV_TOKEN) && !content.contains(FILE_TOKEN) {
        return None;
    }
    let text = with_variables(content, env);
    read(&with_files(&text, env.get(HOME_VAR).map(Path::new), cwd)?)
}

/// `text` with the value of a variable of `env` in place of every `{env:NAME}`, as OpenCode
/// (1.19.6) substitutes: once over the text, the name taken as written up to the closing brace,
/// nothing for a variable that is not set, a value that holds the token itself left as it is, and
/// a token with no name (`{env:}`) left as text.
fn with_variables(text: &str, env: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(ENV_TOKEN) {
        let after = &rest[at + ENV_TOKEN.len()..];
        let Some(end) = after.find('}') else {
            break;
        };
        if end == 0 {
            out.push_str(&rest[..at + ENV_TOKEN.len()]);
            rest = after;
            continue;
        }
        out.push_str(&rest[..at]);
        if let Some(value) = env.get(&after[..end]) {
            out.push_str(value);
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Where a reader of [`with_files`] is in the text.
#[derive(Clone, Copy, PartialEq)]
enum Within {
    Json,
    String,
    LineComment,
    BlockComment,
}

/// `text` with the content of the file in place of every `{file:path}` written outside a string
/// and outside a comment, as OpenCode (1.19.6) puts it in: the path taken as written up to the
/// closing brace, `~/` at its start standing for `home`, a relative one taken from `cwd`; the
/// content trimmed and escaped as the inside of a JSON string, and not read for tokens again.
/// OpenCode skips a token on a line that starts with `//`, so one after a comment that a carriage
/// return ended is left too.
///
/// A token inside a string is left as it is: what is handed over reads as JSON with it there, and
/// OpenCode puts the file in when it reads that, so the content of a file named there (a key, as
/// a rule) stays out of the environment the launch builds. One in a comment is left as well, and
/// the comment goes with the parse; a missing file named in one, which OpenCode refuses to start
/// with, is therefore not seen here.
///
/// `None` where a file named outside a string cannot be read within [`VALUE_FILE_LIMIT`]: it is
/// missing, no regular file, not UTF-8, or begins with `~/` when no home is known. The file is
/// only read.
fn with_files(text: &str, home: Option<&Path>, cwd: &Path) -> Option<String> {
    let bytes = text.as_bytes();
    let project = canonical_project(cwd);
    let mut out = String::with_capacity(text.len());
    // Every index the text is cut at is that of an ASCII byte, so a character boundary.
    let mut copied = 0;
    let mut within = Within::Json;
    let mut at = 0;
    while at < bytes.len() {
        let next = bytes.get(at + 1).copied();
        match (within, bytes[at]) {
            (Within::Json, b'{') if bytes[at..].starts_with(FILE_TOKEN.as_bytes()) => {
                let name_at = at + FILE_TOKEN.len();
                let named = bytes[name_at..].iter().position(|byte| *byte == b'}');
                if let Some(len) = named.filter(|len| *len > 0) {
                    let after = name_at + len + 1;
                    if !on_comment_line(&text[..at]) {
                        let content = named_file(&text[name_at..name_at + len], home, &project)?;
                        out.push_str(&text[copied..at]);
                        out.push_str(&string_inside(js_trimmed(&content)));
                        copied = after;
                    }
                    at = after;
                    continue;
                }
            }
            (Within::Json, b'"') => within = Within::String,
            (Within::Json, b'/') if next == Some(b'/') => {
                within = Within::LineComment;
                at += 1;
            }
            (Within::Json, b'/') if next == Some(b'*') => {
                within = Within::BlockComment;
                at += 1;
            }
            // The character after a backslash is the string's, a quote included.
            (Within::String, b'\\') => at += 1,
            (Within::String, b'"') => within = Within::Json,
            (Within::LineComment, b'\n' | b'\r') => within = Within::Json,
            (Within::BlockComment, b'*') if next == Some(b'/') => {
                within = Within::Json;
                at += 1;
            }
            _ => {}
        }
        at += 1;
    }
    out.push_str(&text[copied..]);
    Some(out)
}

/// Whether `before`, the text ahead of a token, ends in a line that starts with `//`, which is
/// where OpenCode (1.19.6) leaves a `{file:…}` as it is.
fn on_comment_line(before: &str) -> bool {
    let line = before.rfind('\n').map_or(before, |end| &before[end + 1..]);
    line.trim_start_matches(js_space).starts_with("//")
}

/// The text of the file a `{file:…}` names as `name`, as OpenCode (1.19.6) finds it: below `home`
/// for a name that starts with `~/` (no other use of `~` is taken), else `name` itself where it
/// is absolute and below `project`, the canonical working directory, where it is not.
fn named_file(name: &str, home: Option<&Path>, project: &Path) -> Option<String> {
    let path = match name.strip_prefix("~/") {
        Some(below_home) => home?.join(below_home),
        None => project.join(name),
    };
    let path = crate::hostfs::lexically_normalise(&path);
    read_bounded_text(&path, project, VALUE_FILE_LIMIT).ok()
}

/// `text` escaped as the inside of a JSON string, without its quotes, which is how OpenCode
/// writes a file's content into the text of a configuration.
fn string_inside(text: &str) -> String {
    let quoted = Value::String(text.to_string()).to_string();
    quoted[1..quoted.len() - 1].to_string()
}

/// `text` without the white space at its ends, as JavaScript's `trim` takes it: unlike Rust's,
/// that counts a byte order mark and does not count U+0085.
fn js_trimmed(text: &str) -> &str {
    text.trim_matches(js_space)
}

fn js_space(character: char) -> bool {
    matches!(
        character,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The lists OpenCode adds to when it merges one configuration over another; every other list is
/// replaced whole.
const ADDED_LISTS: [&str; 3] = ["instructions", "plugin", "plugins"];

/// Merges `over` into `base` the way OpenCode (1.19.6) merges one configuration over another: an
/// object key by key, the top-level lists of [`ADDED_LISTS`] by adding the entries not there yet,
/// anything else replaced.
fn merge_over(base: &mut Value, over: Value) {
    let (Value::Object(base), Value::Object(over)) = (base, over) else {
        return;
    };
    for (key, value) in over {
        match (base.get_mut(&key), value) {
            (Some(Value::Array(existing)), Value::Array(added))
                if ADDED_LISTS.contains(&key.as_str()) =>
            {
                for entry in added {
                    if !existing.contains(&entry) {
                        existing.push(entry);
                    }
                }
            }
            (Some(existing), value) => merge_nested(existing, value),
            (None, value) => {
                base.insert(key, value);
            }
        }
    }
}

/// Merges `over` into `base` below the top level: an object key by key, anything else replaced.
fn merge_nested(base: &mut Value, over: Value) {
    match (base, over) {
        (Value::Object(base), Value::Object(over)) => {
            for (key, value) in over {
                match base.get_mut(&key) {
                    Some(existing) => merge_nested(existing, value),
                    None => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (base, over) => *base = over,
    }
}
