//! Qwen Code's behavior (`crate::providers::qwen`): its launch, with Octoboard's hooks, MCP server
//! and role text injected, and the sub-interfaces its measurements give an answer for. The
//! measurements are in `docs/agent-cli-reference.md`, section "Qwen Code".
//!
//! Everything is injected per launch and nothing of the user's is edited:
//!
//! - **Hooks and five interface settings** go in a settings file in the session's scratch
//!   directory, named by `QWEN_CODE_SYSTEM_SETTINGS_PATH` ([`settings`]). Qwen Code adds the hook
//!   lists of its settings layers together, so the user's own hooks keep running, and the content
//!   of the system settings file this one stands in for is carried into it. A system settings
//!   file that is there and cannot be used makes the enhanced launch unavailable, and the session
//!   starts bare, where Qwen Code reads that file itself; the system defaults file, which Qwen
//!   Code looks for beside the system settings file, is named for it.
//! - **The Octoboard MCP server** is a file given to `--mcp-config`, whose servers are added to the
//!   user's own.
//! - **The role text** is `--append-system-prompt`, which a resume replaces with what it passes.
//!
//! An option a release no longer takes makes Qwen Code print `Unknown arguments: …` and its whole
//! usage text, and exit before it draws anything (0.28.3). The refusal line comes first, within
//! the head the daemon keeps of an early exit, so which option it was could be read; Qwen Code
//! still gives no reduced plan, by choice: no release is known to drop one of the launch's
//! options, so there is no refusal of a real release to shape a launch with less around, and an
//! early exit goes straight to the bare launch.
//!
//! The hooks give a session its status, and Qwen Code's own OSC 7501 reports say beside them when
//! it waits on the user and when a turn was cancelled ([`AgentProvider::reported_signals`]). Where
//! the user has switched every hook off, the launch does not switch them back on: it tells the
//! user ([`hooks_off`]).
//!
//! `SessionStart` is deliberately not among the hooks: Qwen Code runs it before its folder-trust
//! dialog is answered, and Octoboard takes a session's first hook to mean it is past that dialog
//! (`crate::session::trust`), so a message's Enter would trust the folder on nobody's say-so. What
//! says its startup is behind it is its input box ([`screen`]).

mod accounts;
mod hooks;
mod hooks_off;
mod interaction;
mod login;
mod mcp_inventory;
mod models;
mod records;
mod screen;
mod settings;
mod settings_file;
mod shared_config;
mod version;

use std::collections::HashMap;

use anyhow::{Context, Result};
use serde_json::json;

use super::{AgentProvider, CliPackaging, LaunchPlan, LaunchSpec};
use crate::hooks::HookInterpreter;
use crate::hostfs::replace::{replace_beside, Permissions};
use crate::platform::shell::{self, ResolvedBinary, Shim};
use crate::providers::{AgentId, ProviderDescriptor};
use crate::session::screen::ScreenReader;
use crate::session_tools;

pub struct QwenProvider;

/// The MCP configuration file of a launch, in the session's scratch directory.
const MCP_CONFIG_FILE: &str = "qwen-mcp.json";

/// The flag that takes a prompt to run before staying interactive.
const PROMPT_FLAG: &str = "--prompt-interactive";

/// The variable that makes Qwen Code stay in the process it was started in instead of handing over
/// to another runtime, so that what stops the session reaches the agent itself.
const NO_RELAUNCH_VAR: &str = "QWEN_CODE_NO_RELAUNCH";
const NPM_PACKAGE: &str = "@qwen-code/qwen-code";

impl QwenProvider {
    /// The MCP configuration that registers Octoboard's server: trusted, so that its tools raise no
    /// permission prompt, and with Qwen Code's own time limit on a call (in milliseconds) above
    /// Octoboard's forwarding limit (`crate::adapter::MCP_TOOL_TIMEOUT_SECS`).
    fn mcp_config_json(spec: &LaunchSpec<'_>) -> String {
        let (command, args) = super::mcp_server_command(spec);
        json!({ "mcpServers": {
            session_tools::catalogue::SERVER_KEY: {
                "command": command,
                "args": args,
                "trust": true,
                "timeout": super::MCP_TOOL_TIMEOUT_SECS * 1000,
            },
        }})
        .to_string()
    }
}

impl AgentProvider for QwenProvider {
    fn descriptor(&self) -> &'static ProviderDescriptor {
        AgentId::QWEN.descriptor()
    }

    fn hooks(&self) -> &'static dyn HookInterpreter {
        &hooks::QwenHooks
    }

    fn has_status_integration(&self) -> bool {
        true
    }

    /// A launch found with its hooks switched off ([`hooks_off`]) has no hook to open or close a
    /// turn, and takes its whole status from Qwen Code's reports, which go on as ever (0.28.3).
    /// The launch's notice is what says it was found so.
    fn launch_has_status_integration(&self, _spec: &LaunchSpec<'_>, plan: &LaunchPlan) -> bool {
        !hooks_switched_off(plan)
    }

    /// A launch found with its hooks switched off reports no prompt, so a held message the turn
    /// took cannot be told from one a cancel dropped: it has nothing queued again at a cancel.
    fn launch_cancel_drops_held_messages(&self, _spec: &LaunchSpec<'_>, plan: &LaunchPlan) -> bool {
        !hooks_switched_off(plan) && self.screen().cancel_drops_held_messages()
    }

    /// Measured on 0.28.3: `blocked` is reported at a permission prompt, at a question and at the
    /// folder-trust dialog, with its kind. The wait starts the question or permission reader
    /// when a call was reported by a hook, and the report that follows ends the wait. `idle`
    /// follows `working` only where a turn was cancelled, Esc and Ctrl-C alike, a turn that ends
    /// and a prompt that is declined being `done`.
    fn reported_signals(&self) -> crate::session::ReportedSignals {
        crate::session::ReportedSignals {
            wait_on_user: true,
            cancel: true,
        }
    }

    /// Qwen Code reports `done` between the model calls of one turn as well as at its end, for
    /// about 0.1 s before the next call's `working` (0.28.3; the title's plain spell in the same
    /// gap lasted 0.05 s to 0.6 s). Read only for a launch whose status the reports give, one
    /// with its hooks switched off. One second covers the longest gap measured with room for a
    /// loaded machine, as the title's settle of the same length already does, and holds a turn's
    /// real end back by no more than that.
    fn reported_done_gap(&self) -> Option<std::time::Duration> {
        Some(REPORTED_DONE_GAP)
    }

    fn screen(&self) -> &'static dyn ScreenReader {
        &screen::QwenScreen
    }

    /// Its permission prompt and its `ask_user_question` form, read off the screen and answered
    /// by keys ([`interaction`]).
    fn interactions(&self) -> Option<&'static dyn crate::session::interaction::InteractionReader> {
        Some(&interaction::QwenInteractions)
    }

    /// The models of the account's settings ([`models`]).
    fn catalogue(&self) -> &'static dyn crate::agents::models::ModelCatalogue {
        &models::QwenCatalogue
    }

    fn records(&self) -> &'static dyn crate::records::RecordStore {
        &records::QwenRecords
    }

    fn accounts(&self) -> Option<&'static dyn crate::accounts::AccountOps> {
        Some(&accounts::QwenAccounts)
    }

    /// The version the Qwen home runs, which its own updates make its own ([`version`]).
    fn account_version(&self, binary: &str, home: &std::path::Path) -> Option<super::AgentVersion> {
        version::of_home(binary, home)
    }

    /// Published on npm as `@qwen-code/qwen-code`; no installer of its own is known.
    fn cli_packaging(&self) -> &'static CliPackaging {
        &CliPackaging {
            native_layouts: &[],
            native_markers: &[],
            native_update: &[],
            native_latest: None,
            native_manifest: None,
            npm_packages: &[NPM_PACKAGE],
            homebrew_formula: None,
            homebrew_cask: None,
        }
    }

    /// On Windows, the npm shim's `cli-entry.js` run by Node. A `node.exe` found neither beside the
    /// shim nor on the shell's `PATH` is an error, which starts the session bare through the shim
    /// itself (`crate::session::term::plan_launch`): the shim finds Node its own way, as it does in
    /// a plain terminal, and only the multiline role text it could not carry is lost.
    fn terminal_binary(
        &self,
        binary: ResolvedBinary,
        shell_env: &HashMap<String, String>,
    ) -> Result<ResolvedBinary> {
        if crate::platform::HostOs::current() != crate::platform::HostOs::Windows
            || !binary.program.extension().is_some_and(|ext| {
                ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("bat")
            })
        {
            return Ok(binary);
        }
        let Shim::Package(package) = shell::shim_package(&binary, NPM_PACKAGE) else {
            return Ok(binary);
        };
        // The npm launcher uses Node on cli-entry.js. Invoke the same entry point directly
        // because cmd cannot carry the line breaks in the role's --append-system-prompt.
        let entry = package.join("cli-entry.js");
        if !entry.is_file() {
            return Ok(binary);
        }
        let adjacent_node = binary.program.with_file_name("node.exe");
        let mut node = if adjacent_node.is_file() {
            ResolvedBinary::new(adjacent_node)
        } else {
            crate::env_shell::resolve_binary("node.exe", shell_env)?
        };
        node.leading_args.push(entry.into_os_string());
        Ok(node)
    }

    /// `--resume <id>` for a resume, `--model`, and a new conversation's task joined to
    /// `--prompt-interactive`.
    fn bare_plan(&self, spec: &LaunchSpec<'_>) -> super::BarePlan {
        let mut args = Vec::new();
        if let Some(id) = spec.resume_agent_session_id {
            args.push("--resume".to_string());
            args.push(id.to_string());
        }
        if let Some(model) = spec.model {
            args.push("--model".to_string());
            args.push(model.to_string());
        }
        args.extend(task_arg(spec));
        super::BarePlan {
            args,
            ..super::BarePlan::default()
        }
    }

    fn plan(&self, spec: &LaunchSpec<'_>) -> Result<LaunchPlan> {
        let agent = AgentId::QWEN;
        // Checked first, so that a pinned directory that has gone refuses the launch before
        // anything is written for it.
        let pinned = super::pinned_config_dir_export(spec, agent)?;
        let mut args = Vec::new();
        match spec.resume_agent_session_id {
            Some(id) => {
                args.push("--resume".to_string());
                args.push(id.to_string());
            }
            // Qwen Code refuses an id that is not a UUID.
            None => {
                args.push("--session-id".to_string());
                args.push(spec.new_agent_session_id.to_string());
            }
        }

        // A path, not the JSON, as for every file a launch hands over.
        let mcp_config = spec.scratch.join(MCP_CONFIG_FILE);
        replace_beside(
            &mcp_config,
            Self::mcp_config_json(spec).as_bytes(),
            Permissions::Private,
        )
        .context("writing the session's MCP configuration")?;
        args.push("--mcp-config".to_string());
        args.push(mcp_config.to_string_lossy().into_owned());

        args.push("--append-system-prompt".to_string());
        args.push(session_tools::role::role_description(
            spec.kind,
            spec.owner.as_ref(),
            agent,
            &spec.role_options(),
        ));

        // With none chosen the flag is left out, and Qwen Code's own configuration decides.
        let approval_mode = match spec.settings.choice("approval_mode") {
            Some("plan") => Some("plan"),
            Some("default") => Some("default"),
            Some("auto_edit") => Some("auto-edit"),
            Some("auto") => Some("auto"),
            Some("yolo") => Some("yolo"),
            _ => None,
        };
        if let Some(mode) = approval_mode {
            args.push("--approval-mode".to_string());
            args.push(mode.to_string());
        }
        if let Some(model) = spec.model {
            args.push("--model".to_string());
            args.push(model.to_string());
        }
        // It has no reasoning-effort flag, so an effort is not passed.
        args.extend(task_arg(spec));

        let settings_file = settings::write(spec)?;
        let mut env = vec![
            (
                settings::SYSTEM_SETTINGS_VAR.to_string(),
                settings_file.to_string_lossy().into_owned(),
            ),
            // Qwen Code looks for the system defaults file beside the system settings file, so
            // the one it would have read is named for it.
            (
                settings::SYSTEM_DEFAULTS_VAR.to_string(),
                settings::system_defaults_path(spec.shell_env)
                    .to_string_lossy()
                    .into_owned(),
            ),
            (NO_RELAUNCH_VAR.to_string(), "1".to_string()),
        ];
        // Set only for a pinned directory; otherwise the shell snapshot's own value, if any,
        // stands.
        env.extend(pinned);
        if spec.kind == crate::protocol::SessionKind::Console {
            let trust_file = spec.scratch.join("trusted-folders.json");
            let trust = serde_json::json!({ (spec.cwd.to_string_lossy()): "TRUST_FOLDER" });
            replace_beside(
                &trust_file,
                trust.to_string().as_bytes(),
                Permissions::Private,
            )
            .context("writing the console's launch trust file")?;
            env.push((
                "QWEN_CODE_TRUSTED_FOLDERS_PATH".to_owned(),
                trust_file.to_string_lossy().into_owned(),
            ));
        }
        Ok(LaunchPlan {
            args,
            env,
            notice: hooks_off::switched_off_by(spec)
                .as_ref()
                .map(hooks_off::notice),
            ..LaunchPlan::default()
        })
    }

    /// No hook is registered for the start (`SessionStart` runs before the folder-trust dialog is
    /// answered, see [`settings::HOOK_EVENTS`]), so the first comes with the first turn's
    /// `UserPromptSubmit`; a launch found with its hooks switched off has none to come.
    fn first_hook_due(&self, _spec: &LaunchSpec<'_>, plan: &LaunchPlan) -> super::FirstHook {
        if hooks_switched_off(plan) {
            super::FirstHook::Never
        } else {
            super::FirstHook::WithFirstTurn
        }
    }

    fn prompt_text(&self, line: &str) -> Option<String> {
        records::prompt_text(line)
    }

    fn summary_title(&self, summary: &serde_json::Value) -> Option<String> {
        match summary["source"].as_str()? {
            "manual" | "auto" => summary["displayName"].as_str().map(str::to_owned),
            _ => None,
        }
    }

    /// Qwen Code's title is `Qwen - <folder>` unless the user names it otherwise (0.28.0), after
    /// the glyph that says what it is doing, and padded with spaces: it names no task.
    fn task_title(&self, raw: &str, _project_names: &[String]) -> Option<String> {
        let title = screen::without_status(raw);
        (!title.is_empty() && !title.starts_with("Qwen - ")).then(|| title.to_owned())
    }
}

/// How long a reported `done` is held for the next model call's `working`
/// ([`AgentProvider::reported_done_gap`]).
const REPORTED_DONE_GAP: std::time::Duration = std::time::Duration::from_secs(1);

/// Whether the launch was found with its hooks switched off: its notice says so.
fn hooks_switched_off(plan: &LaunchPlan) -> bool {
    plan.notice.as_ref().is_some_and(hooks_off::is_notice)
}

/// A new conversation's task, the plan and the bare plan alike. A positional prompt is one-shot;
/// `--prompt-interactive` (`-i`) runs it and stays interactive. The task goes in the same argument
/// as the flag: given as the next one, a task that starts with a dash (a list, "--help me with
/// X") is read as options, and Qwen Code prints its usage and exits. A resume is given no task.
fn task_arg(spec: &LaunchSpec<'_>) -> Option<String> {
    spec.task
        .filter(|_| spec.resume_agent_session_id.is_none())
        .map(|task| format!("{PROMPT_FLAG}={task}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::tests::{spec_fixture, SpecFixture};
    use crate::protocol::{error_code, CodedError};

    /// Qwen Code's bare launch: `--resume`, `--model` and a new conversation's task joined to
    /// `--prompt-interactive=`, without the MCP configuration, role or settings files.
    #[test]
    fn its_bare_plan_names_the_conversation_the_model_and_the_task_only() {
        let mut fixture = crate::adapter::tests::spec_fixture();
        fixture.model = Some("model-a".to_string());
        let bare = |resume: Option<&str>, task: Option<&str>| {
            let mut spec = fixture.spec();
            spec.resume_agent_session_id = resume;
            spec.task = task;
            QwenProvider.bare_plan(&spec)
        };
        assert_eq!(
            bare(Some("c-1"), Some("ignored")).args,
            ["--resume", "c-1", "--model", "model-a"]
        );
        assert_eq!(
            bare(None, Some("-do it")).args,
            ["--model", "model-a", "--prompt-interactive=-do it"]
        );
    }

    const NEW: &str = "new-agent-session";
    const RESUMED: &str = "resumed-session";

    #[cfg(windows)]
    #[test]
    fn a_terminal_runs_the_npm_entry_point_directly_to_pass_multiline_role_text() {
        let scratch = crate::test_support::ScratchDir::new("qwen-npm-terminal");
        let launcher = scratch.join("qwen.cmd");
        let node = scratch.join("node.exe");
        let package = scratch.join("node_modules").join(NPM_PACKAGE);
        std::fs::create_dir_all(&package).unwrap();
        let entry = package.join("cli-entry.js");
        for path in [&launcher, &node, &entry, &package.join("package.json")] {
            std::fs::write(path, "").unwrap();
        }
        let command = QwenProvider
            .terminal_binary(ResolvedBinary::new(launcher), &HashMap::new())
            .unwrap();
        assert_eq!(command.program, node);
        let [argument] = command.leading_args.as_slice() else {
            panic!("Node takes one leading argument");
        };
        assert_eq!(
            crate::hostfs::canonical(std::path::Path::new(argument)).unwrap(),
            crate::hostfs::canonical(entry).unwrap()
        );
        let plan = QwenProvider.plan(&isolated().spec()).unwrap();
        assert!(value_after(&plan.args, "--append-system-prompt")
            .unwrap()
            .contains('\n'));
    }

    /// A launch fixture whose shell names the system settings and defaults files in its scratch
    /// directory, so that no launch reads the machine's own (`/Library/Application
    /// Support/QwenCode`, `/etc/qwen-code`).
    fn isolated() -> SpecFixture {
        let mut fixture = spec_fixture();
        for (variable, name) in [
            (settings::SYSTEM_SETTINGS_VAR, "system-settings.json"),
            (settings::SYSTEM_DEFAULTS_VAR, "system-defaults.json"),
        ] {
            fixture.shell_env.insert(
                variable.to_string(),
                fixture.scratch.join(name).to_string_lossy().into_owned(),
            );
        }
        fixture
    }

    fn value_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
        let at = args.iter().position(|arg| arg == flag)?;
        args.get(at + 1).map(String::as_str)
    }

    /// A new session passes its id, and a resume names the conversation instead, never both.
    #[test]
    fn trusts_the_console_directory_for_one_launch() {
        let mut fixture = spec_fixture();
        let project = QwenProvider.plan(&fixture.spec()).expect("project plan");
        let trust_var = "QWEN_CODE_TRUSTED_FOLDERS_PATH";
        assert_eq!(env_of(&project, trust_var), None);

        fixture.kind = crate::protocol::SessionKind::Console;
        let console = QwenProvider.plan(&fixture.spec()).expect("console plan");
        let path = std::path::Path::new(env_of(&console, trust_var).expect("launch trust file"));
        assert!(path.starts_with(&fixture.scratch));
        let trust: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            trust,
            serde_json::json!({ (fixture.cwd.to_string_lossy()): "TRUST_FOLDER" })
        );
    }

    #[test]
    fn a_new_session_passes_its_id_and_a_resume_names_the_conversation() {
        let mut fixture = isolated();
        fixture.new_agent_session_id = NEW.to_string();
        let plan = QwenProvider.plan(&fixture.spec()).expect("plan");
        assert_eq!(&plan.args[..2], ["--session-id", NEW]);
        assert!(!plan.args.iter().any(|arg| arg == "--resume"));

        let mut spec = fixture.spec();
        spec.resume_agent_session_id = Some(RESUMED);
        spec.task = Some("first task");
        let plan = QwenProvider.plan(&spec).expect("plan");
        assert_eq!(&plan.args[..2], ["--resume", RESUMED]);
        assert!(!plan.args.iter().any(|arg| arg == "--session-id"));
        // A resume does not repeat the first task.
        assert!(!plan
            .args
            .iter()
            .any(|arg| arg == "-i" || arg.starts_with("--prompt-interactive")));
    }

    /// A new session passes its model and its first task, and never an effort: Qwen Code has no
    /// flag for one.
    #[test]
    fn a_new_session_passes_the_model_and_the_first_task_and_no_effort() {
        let mut fixture = isolated();
        fixture.model = Some("probe-model".to_string());
        fixture.effort = Some("high".to_string());
        let mut spec = fixture.spec();
        spec.task = Some("first task");
        let plan = QwenProvider.plan(&spec).expect("plan");
        assert_eq!(value_after(&plan.args, "--model"), Some("probe-model"));
        assert!(plan
            .args
            .contains(&"--prompt-interactive=first task".to_string()));
        assert!(!plan.args.iter().any(|arg| arg == "high"));
    }

    /// The first task is one argument, the flag and the task together, so that a task which starts
    /// with a dash is not read as an option: Qwen Code would print its usage and exit. It is never
    /// the positional prompt, which is one-shot, nor a separate value after `-i`.
    #[test]
    fn the_first_task_is_one_argument_whatever_it_starts_with() {
        for task in ["first task", "- a list", "--help me with X", "-i x", "a=b"] {
            let fixture = isolated();
            let mut spec = fixture.spec();
            spec.task = Some(task);
            let plan = QwenProvider.plan(&spec).expect("plan");
            let flagged: Vec<&String> = plan
                .args
                .iter()
                .filter(|arg| arg.starts_with("--prompt-interactive"))
                .collect();
            assert_eq!(
                flagged,
                [&format!("--prompt-interactive={task}")],
                "{task:?}"
            );
            // No separate `-i`, and the task is nowhere an argument of its own.
            assert!(!plan.args.iter().any(|arg| arg == "-i"), "{task:?}");
            assert!(!plan.args.iter().any(|arg| arg == task), "{task:?}");
        }
    }

    /// A pinned account directory is exported through the layout's variable, whatever it is
    /// called, for a new session and a resume alike; an unpinned launch leaves the variable alone.
    #[test]
    fn a_pinned_directory_is_exported_through_the_layouts_variable() {
        let mut fixture = isolated();
        fixture
            .shell_env
            .insert("QWEN_HOME".to_string(), "/somewhere/else".to_string());
        let plan = QwenProvider.plan(&fixture.spec()).expect("plan");
        assert!(!plan.env.iter().any(|(var, _)| var == "QWEN_HOME"));

        let dir = fixture.scratch.join("any-name-at-all");
        std::fs::create_dir_all(&dir).expect("config dir");
        fixture.config_dir = Some(dir.clone());
        let expected = ("QWEN_HOME".to_string(), dir.to_string_lossy().into_owned());
        let plan = QwenProvider.plan(&fixture.spec()).expect("plan");
        assert!(plan.env.contains(&expected));
        let mut spec = fixture.spec();
        spec.resume_agent_session_id = Some(RESUMED);
        assert!(QwenProvider
            .plan(&spec)
            .expect("plan")
            .env
            .contains(&expected));
    }

    /// A pinned directory that has gone refuses a resume with the code every provider uses for it,
    /// and does not refuse a new session, which lets the agent create it.
    #[test]
    fn a_pinned_directory_that_has_gone_refuses_only_a_resume() {
        let mut fixture = isolated();
        fixture.config_dir = Some(fixture.scratch.join("missing"));
        QwenProvider
            .plan(&fixture.spec())
            .expect("a new session is not refused");

        let mut spec = fixture.spec();
        spec.resume_agent_session_id = Some(RESUMED);
        let err = QwenProvider.plan(&spec).err().expect("refused");
        assert_eq!(
            err.downcast_ref::<CodedError>().expect("coded").code,
            error_code::CONFIG_DIR_UNREACHABLE
        );
        let message = err.to_string();
        assert!(message.contains("Qwen Code"), "{message}");
        assert!(!message.contains("  "), "stray spaces in: {message}");
    }

    fn scratch_file(fixture: &crate::adapter::tests::SpecFixture, name: &str) -> String {
        fixture.scratch.join(name).to_string_lossy().into_owned()
    }

    /// The MCP configuration is a file in the session's scratch directory, which the launch names
    /// by path. It registers Octoboard's server, trusted so that its tools raise no permission
    /// prompt, with Qwen Code's own time limit on a call (in milliseconds) above Octoboard's
    /// forwarding limit.
    #[test]
    fn the_mcp_configuration_is_a_file_that_trusts_octoboards_server() {
        let fixture = isolated();
        let plan = QwenProvider.plan(&fixture.spec()).expect("plan");
        let path = value_after(&plan.args, "--mcp-config").expect("--mcp-config");
        assert_eq!(path, scratch_file(&fixture, "qwen-mcp.json"));
        let config: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).expect("the file")).expect("JSON");
        let servers = config["mcpServers"].as_object().expect("mcpServers");
        assert_eq!(servers.keys().collect::<Vec<_>>(), ["octoboard"]);
        let server = &servers["octoboard"];
        assert_eq!(server["command"], fixture.self_exe);
        let args: Vec<&str> = server["args"]
            .as_array()
            .expect("args")
            .iter()
            .map(|arg| arg.as_str().expect("text"))
            .collect();
        assert_eq!(args[0], "mcp");
        assert_eq!(value_after_str(&args, "--session"), Some("session-1"));
        assert_eq!(value_after_str(&args, "--token"), Some("token-1"));
        assert_eq!(server["trust"], true);
        // 600 s of forwarding limit and a minute of margin, in milliseconds; moving the limit
        // means checking it against what Qwen Code does with the value.
        assert_eq!(server["timeout"], 660_000);
        assert!(
            server["timeout"].as_u64().expect("number")
                > crate::session_tools::CALL_TIMEOUT.as_millis() as u64
        );
    }

    fn value_after_str<'a>(args: &[&'a str], flag: &str) -> Option<&'a str> {
        let at = args.iter().position(|arg| *arg == flag)?;
        args.get(at + 1).copied()
    }

    /// A new session and a resume both pass the role text, which Qwen Code builds afresh on every
    /// launch.
    #[test]
    fn the_role_text_is_passed_on_a_new_session_and_on_a_resume() {
        let fixture = isolated();
        let plan = QwenProvider.plan(&fixture.spec()).expect("plan");
        let role = value_after(&plan.args, "--append-system-prompt").expect("the role text");
        assert!(!role.trim().is_empty());

        let mut spec = fixture.spec();
        spec.resume_agent_session_id = Some(RESUMED);
        let plan = QwenProvider.plan(&spec).expect("plan");
        assert_eq!(
            value_after(&plan.args, "--append-system-prompt"),
            Some(role)
        );
    }

    /// The setting's values become Qwen Code's own mode names, for a new session and a resume;
    /// unset, or a value the setting does not take, leaves the flag out so its configuration
    /// decides.
    #[test]
    fn the_approval_mode_setting_becomes_the_flag_or_leaves_it_out() {
        for (choice, expected) in [
            (None, None),
            (Some("plan"), Some("plan")),
            (Some("default"), Some("default")),
            (Some("auto_edit"), Some("auto-edit")),
            (Some("auto"), Some("auto")),
            (Some("yolo"), Some("yolo")),
            (Some("bypass"), None),
        ] {
            for resume in [false, true] {
                let fixture = isolated();
                let settings = crate::providers::ProviderSettings::new([(
                    "approval_mode".to_string(),
                    choice.map(|value| crate::providers::SettingValue::Choice(value.to_string())),
                )]);
                let mut spec = fixture.spec();
                spec.settings = &settings;
                spec.resume_agent_session_id = resume.then_some(RESUMED);
                let plan = QwenProvider.plan(&spec).expect("plan");
                assert_eq!(
                    value_after(&plan.args, "--approval-mode"),
                    expected,
                    "{choice:?} resume={resume}"
                );
                assert_eq!(
                    plan.args
                        .iter()
                        .filter(|arg| *arg == "--approval-mode")
                        .count(),
                    usize::from(expected.is_some()),
                    "{choice:?} resume={resume}"
                );
            }
        }
    }

    /// Without a first task there is no prompt flag; and no model means no `--model`.
    #[test]
    fn no_first_task_and_no_model_pass_neither_flag() {
        let fixture = isolated();
        let plan = QwenProvider.plan(&fixture.spec()).expect("plan");
        assert!(!plan
            .args
            .iter()
            .any(|arg| arg == "-i" || arg == "--model" || arg.starts_with("--prompt-interactive")));
    }

    /// Every launch stays in the process it was started in and runs against the settings file of
    /// its own; the account's variable is left alone for the default account.
    #[test]
    fn every_launch_stays_in_one_process_and_names_its_settings_file() {
        let fixture = isolated();
        for resume in [false, true] {
            let mut spec = fixture.spec();
            spec.resume_agent_session_id = resume.then_some(RESUMED);
            let plan = QwenProvider.plan(&spec).expect("plan");
            let var = |name: &str| {
                plan.env
                    .iter()
                    .find(|(var, _)| var == name)
                    .map(|(_, value)| value.as_str())
            };
            assert_eq!(var("QWEN_CODE_NO_RELAUNCH"), Some("1"));
            let settings = var("QWEN_CODE_SYSTEM_SETTINGS_PATH").expect("the settings file");
            assert_eq!(settings, scratch_file(&fixture, "qwen-settings.json"));
            assert!(std::path::Path::new(settings).is_file());
            assert_eq!(var("QWEN_HOME"), None);
        }
    }

    /// The settings file the launch names carries over the system file the user's shell names, so
    /// the user's own system settings keep applying.
    #[test]
    fn the_launchs_settings_carry_over_the_system_file_the_shell_names() {
        let mut fixture = isolated();
        let system = fixture.scratch.join("system.json");
        std::fs::write(&system, r#"{"model":{"name":"kept"}}"#).expect("system settings");
        fixture.shell_env.insert(
            "QWEN_CODE_SYSTEM_SETTINGS_PATH".to_string(),
            system.to_string_lossy().into_owned(),
        );
        let plan = QwenProvider.plan(&fixture.spec()).expect("plan");
        let (_, file) = plan
            .env
            .iter()
            .find(|(var, _)| var == "QWEN_CODE_SYSTEM_SETTINGS_PATH")
            .expect("the settings file");
        assert_ne!(file, &system.to_string_lossy());
        let launched: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(file).expect("file")).expect("JSON");
        assert_eq!(launched["model"]["name"], "kept");
        assert!(launched["hooks"]["Stop"].is_array());
    }

    #[cfg(unix)]
    #[test]
    fn the_mcp_configuration_is_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = isolated();
        QwenProvider.plan(&fixture.spec()).expect("plan");
        let mode = std::fs::metadata(fixture.scratch.join("qwen-mcp.json"))
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "{mode:o}");
    }

    /// Qwen Code is installed with npm, from the package it publishes, and has no installer, update
    /// command or formula of its own.
    #[test]
    fn it_is_packaged_for_npm_only() {
        let packaging = QwenProvider.cli_packaging();
        assert_eq!(packaging.npm_packages, ["@qwen-code/qwen-code"]);
        assert!(packaging.native_layouts.is_empty());
        assert!(packaging.native_update.is_empty());
        assert!(packaging.native_latest.is_none());
        assert!(packaging.homebrew_formula.is_none());
        assert!(packaging.homebrew_cask.is_none());
    }

    /// Its reports say when it waits on the user and when a turn was cancelled, beside the hooks
    /// that give its status.
    #[test]
    fn its_reports_say_the_wait_and_the_cancel_beside_its_hooks() {
        assert!(QwenProvider.has_status_integration());
        assert_eq!(
            QwenProvider.reported_signals(),
            crate::session::ReportedSignals {
                wait_on_user: true,
                cancel: true,
            }
        );
    }

    /// A launch whose hooks the user switched off says so, naming the switch, for a new session and
    /// a resume alike, and registers its hooks all the same; any other launch says nothing.
    #[test]
    fn a_launch_whose_hooks_are_switched_off_says_so() {
        let fixture = isolated();
        assert!(QwenProvider
            .plan(&fixture.spec())
            .expect("plan")
            .notice
            .is_none());

        let mut fixture = isolated();
        fixture
            .shell_env
            .insert("QWEN_CODE_SAFE_MODE".to_string(), "1".to_string());
        for resume in [false, true] {
            let mut spec = fixture.spec();
            spec.resume_agent_session_id = resume.then_some(RESUMED);
            let plan = QwenProvider.plan(&spec).expect("plan");
            let notice = plan.notice.expect("a notice");
            assert_eq!(
                notice.code,
                crate::protocol::Code::of_provider(crate::providers::qwen::HOOKS_OFF),
                "resume={resume}"
            );
            assert_eq!(
                notice.params.get("switch").map(String::as_str),
                Some("QWEN_CODE_SAFE_MODE")
            );
            let settings: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(scratch_file(&fixture, "qwen-settings.json"))
                    .expect("the settings file"),
            )
            .expect("JSON");
            assert!(settings["hooks"]["Stop"].is_array());
            // The switch is the user's: the launch's file does not set it either way.
            assert!(settings.get("disableAllHooks").is_none());
        }
    }

    /// A launch whose hooks are on takes its status from them and has the messages a cancel
    /// dropped queued again; one found with them switched off, by a variable or by the setting,
    /// takes it from the reports and has nothing queued again, having no prompt reports to tell a
    /// taken message from a dropped one.
    #[test]
    fn a_launch_whose_hooks_are_switched_off_takes_its_status_from_the_reports() {
        let answers = |fixture: &SpecFixture| {
            let spec = fixture.spec();
            let plan = QwenProvider.plan(&spec).expect("plan");
            (
                QwenProvider.launch_has_status_integration(&spec, &plan),
                QwenProvider.launch_cancel_drops_held_messages(&spec, &plan),
            )
        };
        let integrated = |fixture: &SpecFixture| {
            let (integrated, drops) = answers(fixture);
            assert_eq!(drops, integrated, "the two answers follow the same finding");
            integrated
        };
        assert!(integrated(&isolated()));

        let mut by_variable = isolated();
        by_variable
            .shell_env
            .insert("QWEN_CODE_SIMPLE".to_string(), "1".to_string());
        assert!(!integrated(&by_variable));

        let mut by_setting = isolated();
        let account = by_setting.scratch.join("account");
        std::fs::create_dir_all(&account).expect("account");
        std::fs::write(
            account.join("settings.json"),
            r#"{"disableAllHooks": true}"#,
        )
        .expect("settings");
        by_setting.config_dir = Some(account);
        assert!(!integrated(&by_setting));
        // The provider's own answers, for all its launches, stay what they were.
        assert!(QwenProvider.has_status_integration());
        assert!(QwenProvider.screen().cancel_drops_held_messages());
    }

    #[test]
    fn it_reports_its_status_through_hooks_and_has_accounts() {
        assert!(QwenProvider.has_status_integration());
        assert!(QwenProvider.accounts().is_some());
        assert_eq!(QwenProvider.descriptor().id, "qwen");
    }

    /// Qwen Code's title is `Qwen - <folder>` after the glyph that says what it is doing, so it
    /// names no task; any other title does.
    #[test]
    fn its_default_title_names_no_task_and_another_does() {
        for raw in [
            "Qwen - proj",
            "\u{25D0}\u{FE0E} Qwen - proj",
            "\u{2733}\u{FE0E} Qwen - proj",
            "  Qwen - proj                    ",
            "",
            "\u{25D0}\u{FE0E}",
        ] {
            assert_eq!(QwenProvider.task_title(raw, &[]), None, "{raw:?}");
        }
        assert_eq!(
            QwenProvider.task_title("\u{25D0}\u{FE0E} Fix the build  ", &[]),
            Some("Fix the build".to_string())
        );
        assert_eq!(
            QwenProvider.task_title("Fix the build", &["proj".to_string()]),
            Some("Fix the build".to_string())
        );
    }

    fn env_of<'a>(plan: &'a LaunchPlan, name: &str) -> Option<&'a str> {
        plan.env
            .iter()
            .find(|(var, _)| var == name)
            .map(|(_, value)| value.as_str())
    }

    /// Qwen Code looks for the system defaults file beside the system settings file it is given,
    /// so the launch names the one it would have read: the shell's own, else the file beside the
    /// system settings file the shell names, for a new session and a resume alike.
    #[test]
    fn the_launch_names_the_system_defaults_file_it_would_have_read() {
        let mut fixture = isolated();
        let own = fixture.scratch.join("system-defaults.json");
        for resume in [false, true] {
            let mut spec = fixture.spec();
            spec.resume_agent_session_id = resume.then_some(RESUMED);
            let plan = QwenProvider.plan(&spec).expect("plan");
            assert_eq!(
                env_of(&plan, "QWEN_CODE_SYSTEM_DEFAULTS_PATH"),
                Some(own.to_string_lossy().as_ref()),
                "resume={resume}"
            );
        }

        // Named by the shell: that one, wherever it is.
        let named = fixture.scratch.join("elsewhere").join("defaults.json");
        fixture.shell_env.insert(
            "QWEN_CODE_SYSTEM_DEFAULTS_PATH".to_string(),
            named.to_string_lossy().into_owned(),
        );
        let plan = QwenProvider.plan(&fixture.spec()).expect("plan");
        assert_eq!(
            env_of(&plan, "QWEN_CODE_SYSTEM_DEFAULTS_PATH"),
            Some(named.to_string_lossy().as_ref())
        );

        // Not named: beside the system settings file the shell names.
        fixture.shell_env.remove("QWEN_CODE_SYSTEM_DEFAULTS_PATH");
        let beside = fixture.scratch.join("admin");
        std::fs::create_dir_all(&beside).expect("directory");
        fixture.shell_env.insert(
            "QWEN_CODE_SYSTEM_SETTINGS_PATH".to_string(),
            beside.join("settings.json").to_string_lossy().into_owned(),
        );
        let plan = QwenProvider.plan(&fixture.spec()).expect("plan");
        assert_eq!(
            env_of(&plan, "QWEN_CODE_SYSTEM_DEFAULTS_PATH"),
            Some(
                beside
                    .join("system-defaults.json")
                    .to_string_lossy()
                    .as_ref()
            )
        );
    }

    /// A system settings file that cannot be used makes the enhanced launch unavailable, with the
    /// provider's code naming the file, which the session's bare launch nests in its notice; the
    /// bare launch sets no settings variable, so Qwen Code reads that file itself.
    #[test]
    fn a_system_settings_file_that_cannot_be_used_leaves_the_launch_bare() {
        let fixture = isolated();
        std::fs::write(fixture.scratch.join("system-settings.json"), "{ not json").expect("file");
        for resume in [false, true] {
            let mut spec = fixture.spec();
            spec.resume_agent_session_id = resume.then_some(RESUMED);
            let err = QwenProvider.plan(&spec).err().expect("no enhanced plan");
            let coded = err.downcast_ref::<CodedError>().expect("coded");
            assert!(
                QwenProvider
                    .bare_plan(&spec)
                    .into_launch_plan()
                    .env
                    .is_empty(),
                "resume={resume}"
            );
            assert_eq!(
                coded.code,
                crate::protocol::Code::of_provider(
                    crate::providers::qwen::SYSTEM_SETTINGS_UNUSABLE
                ),
                "resume={resume}"
            );
            assert_eq!(
                coded.params.get("path").map(String::as_str),
                Some(
                    fixture
                        .scratch
                        .join("system-settings.json")
                        .to_string_lossy()
                        .as_ref()
                )
            );
        }
    }

    /// Its first hook comes with the first turn, since none is registered for the start; a launch
    /// found with its hooks switched off, by a variable or by the setting, has none to come.
    #[test]
    fn its_first_hook_comes_with_the_first_turn_unless_its_hooks_are_off() {
        let due = |fixture: &SpecFixture| {
            let spec = fixture.spec();
            let plan = QwenProvider.plan(&spec).expect("plan");
            QwenProvider.first_hook_due(&spec, &plan)
        };
        assert_eq!(due(&isolated()), crate::adapter::FirstHook::WithFirstTurn);
        assert!(!settings::HOOK_EVENTS.contains(&"SessionStart"));

        for variable in ["QWEN_CODE_SAFE_MODE", "QWEN_CODE_SIMPLE"] {
            let mut fixture = isolated();
            fixture
                .shell_env
                .insert(variable.to_string(), "1".to_string());
            assert_eq!(
                due(&fixture),
                crate::adapter::FirstHook::Never,
                "{variable}"
            );
        }

        let mut by_setting = isolated();
        let account = by_setting.scratch.join("account");
        std::fs::create_dir_all(&account).expect("account");
        std::fs::write(
            account.join("settings.json"),
            r#"{"disableAllHooks": true}"#,
        )
        .expect("settings");
        by_setting.config_dir = Some(account);
        assert_eq!(due(&by_setting), crate::adapter::FirstHook::Never);
    }

    /// What Qwen Code 0.28.3 wrote as it exited at start (`testdata/qwen-captures.md`): an
    /// unknown option and a value an option does not take, each a refusal line before the whole
    /// usage text, and the near-miss, a resume of a record that is not there. The early exit the
    /// daemon hands over keeps the refusal line, at its head; none gives a reduced plan all the
    /// same, by choice, so each early exit goes to the bare launch.
    #[test]
    fn an_early_exit_goes_to_the_bare_launch() {
        let captures: [(&[u8], Option<&str>); 3] = [
            (
                include_bytes!("../../../testdata/qwen_early_exit_unknown_option_0.28.3.bin"),
                Some("Unknown arguments: octoboard-unknown-flag"),
            ),
            (
                include_bytes!("../../../testdata/qwen_early_exit_invalid_value_0.28.3.bin"),
                Some("Argument: approval-mode, Given: \"bogus\""),
            ),
            (
                include_bytes!("../../../testdata/qwen_early_exit_resume_not_found_0.28.3.bin"),
                None,
            ),
        ];
        let fixture = isolated();
        let spec = fixture.spec();
        for (bytes, refusal) in captures {
            let whole = String::from_utf8_lossy(bytes);
            let exit = crate::coordinator::early_exit::exit_of_output(bytes);
            if let Some(refusal) = refusal {
                // The refusal is at the head, before the usage text, and kept there.
                let usage = whole.find("Usage: qwen [options]").expect("the usage text");
                assert!(whole[..usage].contains(refusal), "{refusal}");
                assert!(exit.output.contains(refusal), "{refusal}");
            } else {
                assert!(whole.starts_with("No saved session found with ID"));
                assert!(!whole.contains("Usage:"));
            }
            assert!(QwenProvider.reduced_plan(&spec, &exit).is_none());
        }
    }

    /// On Windows, a `node.exe` found neither beside the npm shim nor on the shell's `PATH` is an
    /// error, which starts the session bare through the shim rather than refusing it.
    #[cfg(windows)]
    #[test]
    fn a_shim_whose_node_is_not_found_is_an_error_to_launch_bare() {
        let scratch = crate::test_support::ScratchDir::new("qwen-npm-no-node");
        let launcher = scratch.join("qwen.cmd");
        let package = scratch.join("node_modules").join(NPM_PACKAGE);
        std::fs::create_dir_all(&package).unwrap();
        for path in [
            &launcher,
            &package.join("cli-entry.js"),
            &package.join("package.json"),
        ] {
            std::fs::write(path, "").unwrap();
        }
        let shell_env = HashMap::from([(
            "PATH".to_string(),
            scratch.join("empty").to_string_lossy().into_owned(),
        )]);
        assert!(QwenProvider
            .terminal_binary(ResolvedBinary::new(launcher), &shell_env)
            .is_err());
    }
}
