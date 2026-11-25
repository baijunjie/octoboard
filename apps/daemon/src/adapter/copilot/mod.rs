//! GitHub Copilot CLI's adapter (`crate::providers::copilot`). Everything is
//! injected for one run and nothing of the user's is written, the folder-trust answer apart
//! (`screen`); the measurements are in `docs/agent-cli-reference.md`, "GitHub Copilot CLI".
//!
//! - **Hooks** are a plugin directory in the session's scratch directory, loaded with
//!   `--plugin-dir`, which needs no install step and no trusted folder. Copilot's native payloads
//!   name no event, so each event has a script of its own that passes its name
//!   (`write_hook_script`), run without a shell.
//! - **The Octoboard MCP server** is a file given to `--additional-mcp-config`, with an allow
//!   rule for the server so that its tools raise no prompt, and with the limit on one call set
//!   above Octoboard's own (`MCP_TOOL_TIMEOUT_SECS`).
//! - **The role text** is an instructions file in a scratch directory that
//!   `COPILOT_CUSTOM_INSTRUCTIONS_DIRS` names beside the user's own. Copilot builds its system
//!   prompt from it on every launch, a resume included.
//!
//! The CLI is started through the launcher npm installs: it forwards the stop's signal to the
//! native program and ends with it, leaving nothing behind.

mod accounts;
mod feed;
mod hooks;
mod interaction;
mod login;
mod mcp_inventory;
mod models;
mod quota;
mod record_lines;
mod record_watch;
mod records;
mod screen;
mod shared_config;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::json;

use super::{AgentProvider, LaunchPlan, LaunchSpec};
use crate::providers::{AgentId, ProviderDescriptor};
use crate::session::screen::ScreenReader;
use crate::session_tools;

/// The events registered, each with a script of its own. `errorOccurred` and `permissionRequest`
/// are left out: neither says what its name suggests (`hooks`).
const HOOK_EVENTS: &[&str] = &[
    hooks::SESSION_START,
    hooks::PROMPT_SUBMITTED,
    hooks::PRE_TOOL_USE,
    hooks::POST_TOOL_USE,
    hooks::POST_TOOL_USE_FAILURE,
    hooks::AGENT_STOP,
    hooks::SUBAGENT_START,
    hooks::SUBAGENT_STOP,
    hooks::NOTIFICATION,
];

/// The file-writing tools, which the cross-project guard decides: `create` and `edit`, and
/// `apply_patch` for the model families given that in their place. A `matcher` is anchored at
/// both ends, so the alternatives are grouped.
const EDIT_TOOLS: &str = "(create|edit|apply_patch)";

/// The variable naming more directories to read instruction files from, separated by commas.
const INSTRUCTIONS_DIRS_VAR: &str = "COPILOT_CUSTOM_INSTRUCTIONS_DIRS";

/// What Copilot ends its terminal title with, after the session's name.
const TITLE_SUFFIX: &str = " - GitHub Copilot";
const TITLE_UNNAMED: &str = "GitHub Copilot";

pub struct CopilotProvider;

impl AgentProvider for CopilotProvider {
    fn descriptor(&self) -> &'static ProviderDescriptor {
        AgentId::COPILOT.descriptor()
    }

    fn hooks(&self) -> &'static dyn crate::hooks::HookInterpreter {
        &hooks::CopilotHooks
    }

    fn has_status_integration(&self) -> bool {
        true
    }

    fn records(&self) -> &'static dyn crate::records::RecordStore {
        &records::CopilotRecords
    }

    fn screen(&self) -> &'static dyn ScreenReader {
        &screen::CopilotScreen
    }

    fn accounts(&self) -> Option<&'static dyn crate::accounts::AccountOps> {
        Some(&accounts::CopilotAccounts)
    }

    fn interactions(&self) -> Option<&'static dyn crate::session::interaction::InteractionReader> {
        Some(&interaction::CopilotInteractions)
    }

    fn catalogue(&self) -> &'static dyn crate::agents::models::ModelCatalogue {
        &models::CopilotCatalogue
    }

    /// The package is `@github/copilot`. The layout of Copilot's own installer is not measured,
    /// so only an npm install is recognised and updated.
    fn cli_packaging(&self) -> &'static super::CliPackaging {
        &super::CliPackaging {
            native_layouts: &[],
            native_markers: &[],
            native_update: &[],
            native_latest: None,
            native_manifest: None,
            npm_packages: &["@github/copilot"],
            homebrew_formula: None,
            homebrew_cask: None,
        }
    }

    /// Read from the npm install the resolved `binary` belongs to: the package `@github/copilot`
    /// above its launcher (or the platform package that holds the native program, when that is
    /// what `binary` resolves to), and the version of the platform package beside the launcher
    /// where there is one, since that is the program the launcher starts.
    fn installed_version(&self, binary: &str) -> Option<super::AgentVersion> {
        use crate::platform::shell::{self, ResolvedBinary, Shim};

        let resolved = crate::hostfs::canonical(binary).ok()?;
        let launcher = match shell::shim_package(&ResolvedBinary::new(&resolved), NPM_PACKAGE) {
            Shim::Package(package) => package.join("npm-loader.js"),
            _ => resolved,
        };
        installed_version_of(&launcher)
    }

    /// The session's name, which Copilot follows with its own; a session not yet named shows
    /// Copilot's name alone, which says nothing.
    fn task_title(&self, raw: &str, _project_names: &[String]) -> Option<String> {
        let raw = raw.trim();
        if raw == TITLE_UNNAMED {
            return None;
        }
        let name = raw.strip_suffix(TITLE_SUFFIX).unwrap_or(raw).trim();
        (!name.is_empty()).then(|| name.to_string())
    }

    fn summary_title(&self, summary: &serde_json::Value) -> Option<String> {
        crate::records::conversation::text_of(summary, "name")
    }

    fn plan(&self, spec: &LaunchSpec<'_>) -> Result<LaunchPlan> {
        let agent = AgentId::COPILOT;
        // Refused here, before anything is written for the launch.
        let pinned = super::pinned_config_dir_export(spec, agent)?;
        let mut args = Vec::new();
        match spec.resume_agent_session_id {
            // `--resume=` fails for a conversation that has gone, where `--session-id=` would
            // start a new one under the id.
            Some(id) => args.push(format!("--resume={id}")),
            None => args.push(format!("--session-id={}", spec.new_agent_session_id)),
        }

        args.push("--plugin-dir".to_string());
        args.push(path_text(&write_plugin(spec)?));
        args.push("--additional-mcp-config".to_string());
        args.push(format!("@{}", path_text(&write_mcp_config(spec)?)));
        // Octoboard's own tools are what the session was told to do, not a decision to put to
        // the user. The rule names the server, so nothing else is widened.
        args.push(format!(
            "--allow-tool={}",
            session_tools::catalogue::SERVER_KEY
        ));

        // The one permission mode a flag sets for an interactive session. Unset, Copilot's own
        // settings decide for a new session and a resumed one keeps its own.
        if spec.settings.choice("approval_mode") == Some("allow_all") {
            args.push("--allow-all".to_string());
        }
        if let Some(model) = spec.model {
            args.push("--model".to_string());
            args.push(model.to_string());
        }
        if let Some(effort) = spec.effort {
            args.push("--reasoning-effort".to_string());
            args.push(effort.to_string());
        }
        // Starts interactive mode on the prompt. Joined to the flag, since a task that starts with
        // `-` given as an argument of its own is refused as an unknown option.
        if spec.resume_agent_session_id.is_none() {
            if let Some(task) = spec.task {
                args.push(format!("--interactive={task}"));
            }
        }

        let mut env = Vec::new();
        // Set only for a pinned directory; otherwise the shell snapshot's own value, if any,
        // stands.
        env.extend(pinned);
        env.push((
            INSTRUCTIONS_DIRS_VAR.to_string(),
            instructions_dirs(spec, &write_role(spec)?),
        ));
        // A resume otherwise changes to the directory the conversation was recorded in.
        if spec.resume_agent_session_id.is_some() {
            env.push((
                "COPILOT_DISABLE_RESUME_AUTO_CD".to_string(),
                "1".to_string(),
            ));
        }
        Ok(LaunchPlan {
            args,
            env,
            ..LaunchPlan::default()
        })
    }
}

/// The name of the npm package that holds the launcher.
const NPM_PACKAGE: &str = "@github/copilot";

/// The version of the Copilot install `binary` (resolved through every link) belongs to; see
/// `CopilotProvider::installed_version`.
fn installed_version_of(binary: &Path) -> Option<super::AgentVersion> {
    binary.ancestors().skip(1).take(4).find_map(|directory| {
        let (name, version) = package_of(directory)?;
        if name == NPM_PACKAGE {
            let platform = platform_packages(directory, NpmPlatform::host())
                .iter()
                .find_map(|directory| package_of(directory))
                .and_then(|(_, version)| version);
            platform.or(version)
        } else if name.starts_with(&format!("{NPM_PACKAGE}-")) {
            version
        } else {
            None
        }
    })
}

/// The `package.json` of `directory`, parsed.
fn manifest_of(directory: &Path) -> Option<serde_json::Value> {
    let text = super::files::read_regular_text(
        &directory.join("package.json"),
        super::files::SMALL_FILE_LIMIT,
    )
    .ok()?;
    serde_json::from_str(&text).ok()
}

/// The `name` of the `package.json` in `directory`, with its version where it reads as one.
fn package_of(directory: &Path) -> Option<(String, Option<super::AgentVersion>)> {
    let value = manifest_of(directory)?;
    let name = value.get("name")?.as_str()?.to_string();
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_str)
        .and_then(|version| version.split(['-', '+']).next())
        .and_then(super::parse_version);
    Some((name, version))
}

/// A platform as npm names it in a package's `os` and `cpu` (`darwin`, `linux`, `win32`; `arm64`,
/// `x64`).
#[derive(Clone, Copy)]
struct NpmPlatform {
    os: &'static str,
    cpu: &'static str,
}

impl NpmPlatform {
    /// The platform Rust names `os` and `arch` (`std::env::consts::OS` and `ARCH`: `macos`,
    /// `linux`, `windows`; `aarch64`, `x86_64`) as npm names it; a name npm shares, or one it has
    /// none for, is kept.
    fn of(os: &'static str, arch: &'static str) -> Self {
        Self {
            os: match os {
                "macos" => "darwin",
                "windows" => "win32",
                other => other,
            },
            cpu: match arch {
                "aarch64" => "arm64",
                "x86_64" => "x64",
                other => other,
            },
        }
    }

    /// This host's platform as npm names it.
    fn host() -> Self {
        Self::of(std::env::consts::OS, std::env::consts::ARCH)
    }
}

/// Whether a package's manifest lets it be installed on `host`: each of its `os` and `cpu`, where
/// it lists one, names the host's. Copilot's platform packages list both (1.0.114:
/// `@github/copilot-darwin-arm64` lists `darwin` and `arm64`); the glibc and musl packages of one
/// Linux platform both fit, and carry the same version, the one the launcher pins.
fn fits(manifest: &serde_json::Value, host: NpmPlatform) -> bool {
    let allows = |field: &str, value: &str| match manifest.get(field) {
        Some(serde_json::Value::Array(listed)) => {
            listed.iter().any(|listed| listed.as_str() == Some(value))
        }
        _ => true,
    };
    allows("os", host.os) && allows("cpu", host.cpu)
}

/// The platform packages installed under the launcher's package `directory`
/// (`node_modules/@github/copilot-<platform>`) that fit `host` ([`fits`]), in name order: a
/// package left from another platform, or installed with `--force`, is passed over.
fn platform_packages(directory: &Path, host: NpmPlatform) -> Vec<PathBuf> {
    let prefix = format!("{}-", NPM_PACKAGE.rsplit('/').next().unwrap_or(NPM_PACKAGE));
    let scope = directory
        .join("node_modules")
        .join(NPM_PACKAGE.split('/').next().unwrap_or(NPM_PACKAGE));
    let mut packages: Vec<PathBuf> = std::fs::read_dir(scope)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&prefix))
        })
        .filter(|path| manifest_of(path).is_some_and(|manifest| fits(&manifest, host)))
        .collect();
    packages.sort();
    packages
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Writes the plugin that carries Octoboard's hooks and returns its directory. Each event runs
/// its own script, which forwards the payload with the event's name and exits 0 whatever
/// happens: a `preToolUse` command that fails would deny the call. The cross-project guard is a
/// second `preToolUse` entry for the edit tools, whose output Copilot reads as a decision.
fn write_plugin(spec: &LaunchSpec<'_>) -> Result<PathBuf> {
    let dir = spec.scratch.join("copilot-plugin");
    std::fs::create_dir_all(&dir).context("creating the hooks plugin directory")?;
    let mut events = serde_json::Map::new();
    for event in HOOK_EVENTS {
        let script = super::write_hook_script(
            spec.scratch,
            spec.self_exe,
            spec.session_id,
            spec.launch_id,
            spec.daemon_port,
            spec.key_file,
            Some(event),
        )?;
        let mut entries = vec![json!({
            "type": "command",
            "exec": path_text(&script),
            "timeoutSec": super::COMMAND_HOOK_TIMEOUT_SECS,
        })];
        if let (Some(guard), hooks::PRE_TOOL_USE) = (&spec.guard, *event) {
            entries.push(json!({
                "type": "command",
                "matcher": EDIT_TOOLS,
                "exec": path_text(guard.script),
                "timeoutSec": super::GUARD_HOOK_TIMEOUT_SECS,
            }));
        }
        events.insert((*event).to_string(), serde_json::Value::Array(entries));
    }
    std::fs::write(
        dir.join("plugin.json"),
        json!({ "name": "octoboard" }).to_string(),
    )
    .context("writing the hooks plugin's manifest")?;
    std::fs::write(
        dir.join("hooks.json"),
        json!({ "version": 1, "hooks": events }).to_string(),
    )
    .context("writing the hooks plugin's hooks")?;
    Ok(dir)
}

/// Writes the MCP configuration that adds Octoboard's server for this session, over the user's
/// own servers, and returns its path. `timeout` is Copilot's limit on one call, in milliseconds;
/// the reasoning for the value is at `MCP_TOOL_TIMEOUT_SECS`.
fn write_mcp_config(spec: &LaunchSpec<'_>) -> Result<PathBuf> {
    let (command, args) = super::mcp_server_command(spec);
    let mut servers = serde_json::Map::new();
    servers.insert(
        session_tools::catalogue::SERVER_KEY.to_string(),
        json!({
            "type": "local",
            "command": command,
            "args": args,
            "tools": ["*"],
            "timeout": super::MCP_TOOL_TIMEOUT_SECS * 1000,
        }),
    );
    let path = spec.scratch.join("copilot-mcp.json");
    std::fs::write(&path, json!({ "mcpServers": servers }).to_string())
        .context("writing the MCP configuration")?;
    Ok(path)
}

/// Writes the role text where Copilot reads an instructions directory's files from and returns
/// that directory. Only `instructions/<name>.instructions.md` of such a directory is put into
/// the system prompt.
fn write_role(spec: &LaunchSpec<'_>) -> Result<PathBuf> {
    let dir = spec.scratch.join("copilot-instructions");
    let files = dir.join("instructions");
    std::fs::create_dir_all(&files).context("creating the instructions directory")?;
    let role = session_tools::role::role_description(
        spec.kind,
        spec.owner.as_ref(),
        AgentId::COPILOT,
        &spec.role_options(),
    );
    std::fs::write(files.join("octoboard.instructions.md"), role)
        .context("writing the role instructions")?;
    Ok(dir)
}

/// The value of the instructions variable: Octoboard's directory, then whatever the user's shell
/// already names there.
fn instructions_dirs(spec: &LaunchSpec<'_>, own: &Path) -> String {
    let mut dirs = path_text(own);
    if let Some(theirs) = spec
        .shell_env
        .get(INSTRUCTIONS_DIRS_VAR)
        .map(|dirs| dirs.trim().trim_matches(','))
        .filter(|dirs| !dirs.is_empty())
    {
        dirs.push(',');
        dirs.push_str(theirs);
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::tests::spec_fixture;
    use crate::protocol::{error_code, CodedError};

    const NEW: &str = "new-agent-session";
    const RESUMED: &str = "resumed-session";

    fn env_of<'a>(plan: &'a LaunchPlan, var: &str) -> Option<&'a str> {
        plan.env
            .iter()
            .find(|(name, _)| name == var)
            .map(|(_, value)| value.as_str())
    }

    /// A new session passes its id, then the injection, then the model, the effort and the first
    /// task; nothing of a resume.
    #[test]
    fn a_new_session_passes_its_id_the_injection_and_the_first_task() {
        let mut fixture = spec_fixture();
        fixture.new_agent_session_id = NEW.to_string();
        fixture.model = Some("probe-model".to_string());
        fixture.effort = Some("high".to_string());
        let mut spec = fixture.spec();
        spec.task = Some("first task");
        let plan = CopilotProvider.plan(&spec).expect("plan");
        let plugin = fixture.scratch.join("copilot-plugin");
        let mcp = fixture.scratch.join("copilot-mcp.json");
        assert_eq!(
            plan.args,
            vec![
                "--session-id=new-agent-session".to_string(),
                "--plugin-dir".to_string(),
                plugin.to_string_lossy().into_owned(),
                "--additional-mcp-config".to_string(),
                format!("@{}", mcp.to_string_lossy()),
                "--allow-tool=octoboard".to_string(),
                "--model".to_string(),
                "probe-model".to_string(),
                "--reasoning-effort".to_string(),
                "high".to_string(),
                "--interactive=first task".to_string(),
            ]
        );
        assert_eq!(env_of(&plan, "COPILOT_DISABLE_RESUME_AUTO_CD"), None);
        assert_eq!(env_of(&plan, "COPILOT_HOME"), None);
    }

    /// The first task is one argument joined to the flag, so that a task Copilot would take for an
    /// option of its own when given as the next argument (one starting with `-` or `--`) reaches
    /// it whole; the task text is passed unchanged, a line break included.
    #[test]
    fn the_first_task_is_one_argument_whatever_it_starts_with() {
        for task in [
            "first task",
            "- fix the build",
            "--resume is how this task starts",
            "-i",
            "two\nlines",
            "",
        ] {
            let fixture = spec_fixture();
            let mut spec = fixture.spec();
            spec.task = Some(task);
            let plan = CopilotProvider.plan(&spec).expect("plan");
            let tasks: Vec<&String> = plan
                .args
                .iter()
                .filter(|arg| arg.starts_with("--interactive"))
                .collect();
            assert_eq!(tasks, [&format!("--interactive={task}")], "{task:?}");
            // Nothing else of the plan carries the task, so no argument is the task alone.
            assert!(
                !plan.args.iter().any(|arg| !task.is_empty() && arg == task),
                "{task:?}"
            );
            assert!(!plan.args.contains(&"-i".to_string()), "{task:?}");
        }
    }

    /// A resume names the conversation, repeats the injection and not the first task, and keeps
    /// Copilot in the directory it is launched in.
    #[test]
    fn a_resume_names_the_conversation_and_stays_in_its_directory() {
        let mut fixture = spec_fixture();
        fixture.new_agent_session_id = NEW.to_string();
        let mut spec = fixture.spec();
        spec.resume_agent_session_id = Some(RESUMED);
        spec.task = Some("first task");
        let plan = CopilotProvider.plan(&spec).expect("plan");
        assert_eq!(plan.args[0], "--resume=resumed-session");
        assert!(plan.args.contains(&"--plugin-dir".to_string()));
        assert!(plan.args.contains(&"--additional-mcp-config".to_string()));
        assert!(!plan.args.iter().any(|arg| arg.starts_with("--interactive")));
        assert!(!plan.args.iter().any(|arg| arg.contains(NEW)));
        assert_eq!(env_of(&plan, "COPILOT_DISABLE_RESUME_AUTO_CD"), Some("1"));
    }

    /// A daemon started inside a resumed session starts a new one: the outer session's role
    /// directory is not taken for instructions of the user's, and the variable that keeps a resume
    /// in its directory does not reach it.
    #[test]
    fn a_nested_daemons_launch_inherits_neither_the_outer_role_nor_the_resume_variable() {
        let mut fixture = spec_fixture();
        let mut spec = fixture.spec();
        spec.resume_agent_session_id = Some(RESUMED);
        let outer = CopilotProvider.plan(&spec).expect("plan");
        let nested = crate::adapter::tests::nested_shell_env(&fixture.shell_env, &outer);

        fixture.shell_env = nested;
        let inner = CopilotProvider.plan(&fixture.spec()).expect("plan");
        let session = crate::adapter::tests::launched(&fixture.shell_env, &inner);
        assert!(!session[INSTRUCTIONS_DIRS_VAR].contains(','));
        assert!(!session.contains_key("COPILOT_DISABLE_RESUME_AUTO_CD"));
    }

    /// A pinned account directory is exported through the layout's variable, whatever it is
    /// called, for a new session and a resume alike; unpinned, the shell's own value stands.
    #[test]
    fn a_pinned_directory_is_exported_through_the_layouts_variable() {
        let mut fixture = spec_fixture();
        fixture
            .shell_env
            .insert("COPILOT_HOME".to_string(), "/somewhere/else".to_string());
        let plan = CopilotProvider.plan(&fixture.spec()).expect("plan");
        assert_eq!(env_of(&plan, "COPILOT_HOME"), None);

        let dir = fixture.scratch.join("any-name-at-all");
        std::fs::create_dir_all(&dir).expect("config dir");
        fixture.config_dir = Some(dir.clone());
        let expected = dir.to_string_lossy().into_owned();
        let plan = CopilotProvider.plan(&fixture.spec()).expect("plan");
        assert_eq!(env_of(&plan, "COPILOT_HOME"), Some(expected.as_str()));
        let mut spec = fixture.spec();
        spec.resume_agent_session_id = Some(RESUMED);
        let plan = CopilotProvider.plan(&spec).expect("plan");
        assert_eq!(env_of(&plan, "COPILOT_HOME"), Some(expected.as_str()));
    }

    /// A pinned directory that has gone refuses a resume with the code every provider uses for it,
    /// and does not refuse a new session, which lets the agent create it.
    #[test]
    fn a_pinned_directory_that_has_gone_refuses_only_a_resume() {
        let mut fixture = spec_fixture();
        fixture.config_dir = Some(fixture.scratch.join("missing"));
        CopilotProvider
            .plan(&fixture.spec())
            .expect("a new session is not refused");

        let mut spec = fixture.spec();
        spec.resume_agent_session_id = Some(RESUMED);
        let err = CopilotProvider.plan(&spec).err().expect("refused");
        assert_eq!(
            err.downcast_ref::<CodedError>().expect("coded").code,
            error_code::CONFIG_DIR_UNREACHABLE
        );
        let message = err.to_string();
        assert!(message.contains("GitHub Copilot CLI"), "{message}");
        assert!(!message.contains("  "), "stray spaces in: {message}");
    }

    fn json_of(path: &Path) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(path).expect("file")).expect("JSON")
    }

    /// The event name a hook script passes to the daemon's `hook` mode (`--event <name>`).
    fn event_of(script: &Path) -> String {
        let text = std::fs::read_to_string(script).expect("hook script");
        let mut words = text.split_whitespace();
        words
            .find(|word| *word == "--event")
            .expect("the script passes --event");
        words
            .next()
            .expect("an event name")
            .trim_matches(|c| c == '\'' || c == '"')
            .to_string()
    }

    const EVENTS: [&str; 9] = [
        "sessionStart",
        "userPromptSubmitted",
        "preToolUse",
        "postToolUse",
        "postToolUseFailure",
        "agentStop",
        "subagentStart",
        "subagentStop",
        "notification",
    ];

    fn hooks_of(fixture: &crate::adapter::tests::SpecFixture) -> serde_json::Value {
        json_of(&fixture.scratch.join("copilot-plugin").join("hooks.json"))
    }

    /// Nothing optional given: the id and the injection, with no model, effort, task or permission
    /// flag, and no variable but the instructions directory.
    #[test]
    fn a_launch_with_nothing_optional_passes_only_the_id_and_the_injection() {
        let mut fixture = spec_fixture();
        fixture.new_agent_session_id = NEW.to_string();
        let plan = CopilotProvider.plan(&fixture.spec()).expect("plan");
        let names: Vec<&str> = plan
            .args
            .iter()
            .filter(|arg| arg.starts_with("--") || *arg == "-i")
            .map(String::as_str)
            .collect();
        assert_eq!(
            names,
            [
                "--session-id=new-agent-session",
                "--plugin-dir",
                "--additional-mcp-config",
                "--allow-tool=octoboard",
            ]
        );
        let vars: Vec<&str> = plan.env.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(vars, ["COPILOT_CUSTOM_INSTRUCTIONS_DIRS"]);
    }

    /// The hooks are a plugin in the scratch directory: a manifest, and a hooks file with one
    /// command entry per registered event, each running a script of its own that passes the event's
    /// name. `permissionRequest` and `errorOccurred` are not registered.
    #[test]
    fn the_hooks_plugin_has_one_script_per_registered_event() {
        let fixture = spec_fixture();
        CopilotProvider.plan(&fixture.spec()).expect("plan");
        let dir = fixture.scratch.join("copilot-plugin");
        assert_eq!(
            json_of(&dir.join("plugin.json")),
            serde_json::json!({ "name": "octoboard" })
        );
        let hooks = hooks_of(&fixture);
        assert_eq!(hooks["version"], 1);
        let registered = hooks["hooks"].as_object().expect("hooks");
        let mut names: Vec<&str> = registered.keys().map(String::as_str).collect();
        names.sort_unstable();
        let mut expected = EVENTS.to_vec();
        expected.sort_unstable();
        assert_eq!(names, expected);
        assert!(!registered.contains_key("permissionRequest"));
        assert!(!registered.contains_key("errorOccurred"));

        let mut scripts = std::collections::HashSet::new();
        for (event, entries) in registered {
            let [entry] = entries.as_array().expect("entries").as_slice() else {
                panic!("{event}: one entry without a guard");
            };
            assert_eq!(entry["type"], "command", "{event}");
            assert!(entry.get("matcher").is_none(), "{event}");
            assert_eq!(
                entry["timeoutSec"],
                crate::adapter::COMMAND_HOOK_TIMEOUT_SECS,
                "{event}"
            );
            let script = PathBuf::from(entry["exec"].as_str().expect("exec"));
            assert!(script.is_file(), "{event}: {}", script.display());
            assert_eq!(&event_of(&script), event);
            assert!(scripts.insert(script), "{event}: a script of its own");
        }
    }

    /// The cross-project guard is a second entry of `preToolUse` alone, for the edit tools by their
    /// whole names, with the guard's own timeout; without a guard there is none.
    #[test]
    fn the_cross_project_guard_is_a_second_entry_of_pre_tool_use_alone() {
        let fixture = spec_fixture();
        let list = fixture.scratch.join("cross-project-guard.json");
        let script = fixture.scratch.join("guard-hook");
        let mut spec = fixture.spec();
        spec.guard = Some(crate::adapter::GuardHook {
            exe: "/opt/octoboard/install/octoboardd",
            list: &list,
            script: &script,
        });
        CopilotProvider.plan(&spec).expect("plan");
        let hooks = hooks_of(&fixture);
        for event in EVENTS {
            let entries = hooks["hooks"][event].as_array().expect(event);
            if event != "preToolUse" {
                assert_eq!(entries.len(), 1, "{event}");
                continue;
            }
            let [status, guard] = entries.as_slice() else {
                panic!("preToolUse: the status entry and the guard");
            };
            assert!(status.get("matcher").is_none());
            assert_eq!(guard["type"], "command");
            assert_eq!(guard["matcher"], "(create|edit|apply_patch)");
            assert_eq!(
                guard["exec"].as_str(),
                Some(script.to_string_lossy().as_ref())
            );
            assert_eq!(guard["timeoutSec"], crate::adapter::GUARD_HOOK_TIMEOUT_SECS);
        }
    }

    /// The MCP configuration adds Octoboard's server as a local one, started with the session's
    /// identity, with the limit on one call set above Octoboard's own, in milliseconds.
    #[test]
    fn the_mcp_configuration_names_the_server_and_sets_its_call_limit() {
        let fixture = spec_fixture();
        let plan = CopilotProvider.plan(&fixture.spec()).expect("plan");
        let path = fixture.scratch.join("copilot-mcp.json");
        assert!(plan.args.contains(&format!("@{}", path.to_string_lossy())));
        let config = json_of(&path);
        let server = &config["mcpServers"]["octoboard"];
        assert_eq!(
            config["mcpServers"].as_object().map(|map| map.len()),
            Some(1)
        );
        assert_eq!(server["type"], "local");
        assert_eq!(server["command"], "/opt/octoboard/octoboardd");
        let args: Vec<&str> = server["args"]
            .as_array()
            .expect("args")
            .iter()
            .map(|arg| arg.as_str().expect("text"))
            .collect();
        assert_eq!(args[0], "mcp");
        let at = args
            .iter()
            .position(|arg| *arg == "--session")
            .expect("--session");
        assert_eq!(args[at + 1], "session-1");
        assert_eq!(
            server["timeout"].as_u64(),
            Some(crate::adapter::MCP_TOOL_TIMEOUT_SECS * 1000)
        );
        assert!(
            server["timeout"].as_u64().unwrap()
                > crate::session_tools::CALL_TIMEOUT.as_millis() as u64
        );
    }

    /// `--allow-all` is passed for the one choice of `approval_mode` that sets it, for a new
    /// session and a resume alike, and for nothing else.
    #[test]
    fn allow_all_is_passed_only_when_the_setting_says_so() {
        use crate::providers::{ProviderSettings, SettingValue};
        let with = |choice: Option<&str>| {
            ProviderSettings::new([(
                "approval_mode".to_string(),
                choice.map(|value| SettingValue::Choice(value.to_string())),
            )])
        };
        let args_for = |choice: Option<&str>, resume: bool| {
            let mut fixture = spec_fixture();
            fixture.settings = with(choice);
            let mut spec = fixture.spec();
            if resume {
                spec.resume_agent_session_id = Some(RESUMED);
            }
            CopilotProvider.plan(&spec).expect("plan").args
        };
        for resume in [false, true] {
            assert!(args_for(Some("allow_all"), resume).contains(&"--allow-all".to_string()));
            assert!(!args_for(None, resume).contains(&"--allow-all".to_string()));
            assert!(!args_for(Some("manual"), resume).contains(&"--allow-all".to_string()));
        }
    }

    /// The role text is the one instructions file Copilot reads from a directory its variable names,
    /// and the user's own value of that variable follows Octoboard's directory.
    #[test]
    fn the_role_text_is_an_instructions_file_in_a_directory_the_variable_names() {
        let mut fixture = spec_fixture();
        let own = fixture.scratch.join("copilot-instructions");
        let own_text = own.to_string_lossy().into_owned();
        let plan = CopilotProvider.plan(&fixture.spec()).expect("plan");
        assert_eq!(
            env_of(&plan, "COPILOT_CUSTOM_INSTRUCTIONS_DIRS"),
            Some(own_text.as_str())
        );
        let spec = fixture.spec();
        let role = crate::session_tools::role::role_description(
            spec.kind,
            spec.owner.as_ref(),
            AgentId::COPILOT,
            &spec.role_options(),
        );
        let written = std::fs::read_to_string(own.join("instructions/octoboard.instructions.md"))
            .expect("the role file");
        assert!(!written.trim().is_empty());
        assert_eq!(written, role);
        // Octoboard's tools are named the way Copilot presents an MCP server's tool.
        assert!(
            written.contains("octoboard-"),
            "the role names no tool as <server>-<tool>: {written}"
        );

        fixture.shell_env.insert(
            "COPILOT_CUSTOM_INSTRUCTIONS_DIRS".to_string(),
            "/users/own,/users/other".to_string(),
        );
        let plan = CopilotProvider.plan(&fixture.spec()).expect("plan");
        assert_eq!(
            env_of(&plan, "COPILOT_CUSTOM_INSTRUCTIONS_DIRS"),
            Some(format!("{own_text},/users/own,/users/other").as_str())
        );
        // A value that names nothing adds nothing.
        fixture.shell_env.insert(
            "COPILOT_CUSTOM_INSTRUCTIONS_DIRS".to_string(),
            "  ".to_string(),
        );
        let plan = CopilotProvider.plan(&fixture.spec()).expect("plan");
        assert_eq!(
            env_of(&plan, "COPILOT_CUSTOM_INSTRUCTIONS_DIRS"),
            Some(own_text.as_str())
        );
    }

    /// The task title is the session's name, which Copilot follows with its own; Copilot's name
    /// alone, before the first prompt, says nothing.
    #[test]
    fn the_task_title_is_the_sessions_name_without_copilots_own() {
        for (raw, title) in [
            ("GitHub Copilot", None),
            ("  GitHub Copilot  ", None),
            (
                "Fix the login bug - GitHub Copilot",
                Some("Fix the login bug"),
            ),
            ("a - b - GitHub Copilot", Some("a - b")),
            ("  Fix it - GitHub Copilot ", Some("Fix it")),
            ("a name with no suffix", Some("a name with no suffix")),
            ("", None),
            ("   ", None),
        ] {
            assert_eq!(
                CopilotProvider.task_title(raw, &[]).as_deref(),
                title,
                "{raw:?}"
            );
        }
    }

    /// The CLI is started through the launcher npm installs, not its native program: the launcher
    /// forwards the stop's signal and ends with it.
    #[test]
    fn the_cli_is_started_through_its_launcher() {
        assert_eq!(
            CopilotProvider
                .native_binary(&crate::platform::shell::ResolvedBinary::new(
                    "/opt/npm/bin/copilot"
                ))
                .expect("no error"),
            None
        );
    }

    /// Writes `<directory>/package.json` naming `name` at `version`.
    fn package(directory: &Path, name: &str, version: &str) {
        manifest(directory, json!({"name": name, "version": version}));
    }

    /// Writes `<directory>/package.json` as `value`.
    fn manifest(directory: &Path, value: serde_json::Value) {
        std::fs::create_dir_all(directory).unwrap();
        std::fs::write(directory.join("package.json"), value.to_string()).unwrap();
    }

    /// A platform package `copilot-<suffix>` under the launcher's package `package_dir`, at
    /// `version`, listing `os` and `cpu` as Copilot's do.
    fn platform_package(package_dir: &Path, suffix: &str, version: &str, os: &str, cpu: &str) {
        let native = package_dir.join(format!("node_modules/@github/copilot-{suffix}"));
        manifest(
            &native,
            json!({
                "name": format!("{NPM_PACKAGE}-{suffix}"),
                "version": version,
                "os": [os],
                "cpu": [cpu],
            }),
        );
        std::fs::write(native.join("copilot"), "").unwrap();
    }

    /// An npm install, as measured on 1.0.114: the launcher `npm-loader.js` in `@github/copilot`,
    /// linked from the `bin` directory, and the platform package under its `node_modules`.
    #[cfg(unix)]
    fn npm_install(root: &Path, main: &str, platform: Option<&str>) -> PathBuf {
        let package_dir = root.join("lib/node_modules/@github/copilot");
        package(&package_dir, NPM_PACKAGE, main);
        std::fs::write(package_dir.join("npm-loader.js"), "").unwrap();
        if let Some(version) = platform {
            // A name no host has: the package is found by its prefix and its `os` and `cpu`, not
            // by this host's name.
            platform_package(
                &package_dir,
                "plan9-mips",
                version,
                NpmPlatform::host().os,
                NpmPlatform::host().cpu,
            );
        }
        std::fs::create_dir_all(root.join("bin")).unwrap();
        let link = root.join("bin/copilot");
        std::os::unix::fs::symlink(package_dir.join("npm-loader.js"), &link).unwrap();
        link
    }

    /// Of the platform packages under the launcher's, only those whose `os` and `cpu` name the
    /// host are read: one left from another platform is passed over, whatever its version.
    #[test]
    fn only_the_platform_packages_of_the_host_are_read() {
        let fixture = spec_fixture();
        let package_dir = fixture.scratch.join("lib/node_modules/@github/copilot");
        package(&package_dir, NPM_PACKAGE, "1.0.114");
        let host = NpmPlatform {
            os: "darwin",
            cpu: "arm64",
        };
        // In name order, the foreign packages come first.
        platform_package(&package_dir, "darwin-arm32", "9.9.9", "darwin", "arm32");
        platform_package(&package_dir, "aix-arm64", "8.8.8", "aix", "arm64");
        platform_package(&package_dir, "darwin-arm64", "1.0.115", "darwin", "arm64");
        let read: Vec<String> = platform_packages(&package_dir, host)
            .iter()
            .filter_map(|path| package_of(path))
            .map(|(name, _)| name)
            .collect();
        assert_eq!(read, ["@github/copilot-darwin-arm64"]);
        // A package that lists no `os` or `cpu` fits any host.
        manifest(
            &package_dir.join("node_modules/@github/copilot-any"),
            json!({"name": "@github/copilot-any", "version": "1.0.0"}),
        );
        assert_eq!(platform_packages(&package_dir, host).len(), 2);
    }

    /// The version is the platform package's, the program the launcher starts, read through the
    /// `bin` link; without a platform package it is the package's own.
    #[test]
    #[cfg(unix)]
    fn the_installed_version_is_read_from_the_npm_install() {
        let fixture = spec_fixture();
        let both = npm_install(&fixture.scratch.join("both"), "1.0.114", Some("1.0.115-0"));
        assert_eq!(
            CopilotProvider.installed_version(&path_text(&both)),
            Some((1, 0, 115))
        );
        let main_only = npm_install(&fixture.scratch.join("main"), "1.0.114", None);
        assert_eq!(
            CopilotProvider.installed_version(&path_text(&main_only)),
            Some((1, 0, 114))
        );
    }

    /// The native program inside its platform package reads that package's version; a program
    /// in no Copilot package, or one whose package is another's, reads none.
    #[test]
    fn the_installed_version_needs_a_copilot_package() {
        let fixture = spec_fixture();
        let native = fixture.scratch.join("native/@github/copilot-darwin-arm64");
        package(&native, "@github/copilot-darwin-arm64", "1.0.114");
        std::fs::write(native.join("copilot"), "").unwrap();
        assert_eq!(
            CopilotProvider.installed_version(&path_text(&native.join("copilot"))),
            Some((1, 0, 114))
        );
        let other = fixture.scratch.join("other/pkg");
        package(&other, "@openai/codex", "0.150.0");
        std::fs::write(other.join("copilot"), "").unwrap();
        assert_eq!(
            CopilotProvider.installed_version(&path_text(&other.join("copilot"))),
            None
        );
        let bare = fixture.scratch.join("bare");
        std::fs::create_dir_all(&bare).unwrap();
        std::fs::write(bare.join("copilot"), "").unwrap();
        assert_eq!(
            CopilotProvider.installed_version(&path_text(&bare.join("copilot"))),
            None
        );
    }
}
