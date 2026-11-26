//! Pi's behavior (`crate::providers::pi`). Pi has no hooks and no status integration of its own:
//! it reports its status through OSC 7501 once the terminal answers the support query. On the
//! rendering inbox ConPTY host, which did not deliver that report, the extension forwards its run
//! lifecycle to the same program-status reader (`crate::session::program_status`).
//!
//! Every launch and resume passes, beside the conversation's id, the model and the effort:
//!
//! - the role text as a file in the session's scratch directory, with `--append-system-prompt`,
//!   which takes a file's contents as readily as text. Pi builds its system prompt afresh on every
//!   start, so a resume carries the role once, not again on top of the one it started with;
//! - Octoboard's extension (`extension.js`), written into the scratch directory with the data file
//!   beside it ([`EXTENSION_DATA`]) and loaded with `--extension`, which needs no trust decision.
//!   It registers the session's Octoboard MCP server with its tools declared to the model, under
//!   `mcp__octoboard__<tool>`; where the user's own `mcp.json` names a server that would take the
//!   registered one's place, it runs the server itself and registers each tool under the same
//!   name, so the role text holds either way and the user's file is left as it is. It forwards
//!   through the session's hook script (`hooks`) Pi's `session_start`, again once the session it
//!   started has been written to its file, each prompt that enters the conversation, and what a
//!   run said once Pi has settled after it. Where the session is kept to its own project, it asks
//!   the session's guard script about each call of Pi's file-writing tools and refuses the ones
//!   the guard refuses. It makes the images Octoboard pasted into the session attachments, where
//!   Pi itself would send their paths as text;
//! - a pinned account's directory as `PI_CODING_AGENT_DIR`.
//!
//! A new session's first task is a positional argument after a bare `--`, except one that starts
//! with `@`, which Pi would read as a file to include: that one is queued as the session's first
//! message ([`LaunchPlan::first_message`]).
//!
//! A new session passes `--session-id`, which opens the working directory's session of that id and
//! creates it where there is none; a resume passes `--session`, which opens the stored one and
//! fails, printing that no session matches, where it has gone, rather than quietly starting
//! another. Where the record is in a records directory an earlier launch of the session selected
//! through `PI_CODING_AGENT_SESSION_DIR`, the resume names that directory with `--session-dir`,
//! where `--session` alone would not look. The measurements are in `docs/agent-cli-reference.md`,
//! section "Pi".
//!
//! An option a Pi release no longer takes makes Pi exit as it starts, before it draws anything
//! (`Error: Unknown option: …`, 1.5.2), and so does an extension that throws while loading: the
//! launch is then started again bare, and the extension never throws into Pi.

mod accounts;
mod hooks;
mod login;
mod mcp_inventory;
mod models;
mod records;
mod screen;
mod shared_config;

use anyhow::{Context, Result};
use serde_json::json;

use super::files::{read_regular_text, SMALL_FILE_LIMIT};
use super::{mcp_server_command, AgentProvider, LaunchPlan, LaunchSpec, MCP_TOOL_TIMEOUT_SECS};
use crate::hostfs::replace::{replace_beside, Permissions};
use crate::providers::{AgentId, ProviderDescriptor};
use crate::session::screen::ScreenReader;
use crate::session_tools;

/// Octoboard's extension, as it is written into a session's scratch directory.
const EXTENSION: &str = include_str!("extension.js");

/// The file the extension is written to in the scratch directory.
const EXTENSION_FILE: &str = "octoboard-pi.js";

/// The file beside it holding what differs per session, which the extension reads by this name,
/// and the daemon's own facts the extension goes by: the hook script and how long a prompt waits
/// for it, the guard script where there is one with the tools it is asked about and its timeout,
/// the MCP server's command, and how the images Octoboard pastes are named. Readable by its owner
/// alone, as the key file the server is given is.
const EXTENSION_DATA: &str = "octoboard-pi.json";

/// The file the role text is written to in the scratch directory.
const ROLE_FILE: &str = "octoboard-role.md";

/// The name the session's Octoboard MCP server is registered under.
const MCP_SERVER_NAME: &str = "octoboard";

pub struct PiProvider;

impl AgentProvider for PiProvider {
    fn descriptor(&self) -> &'static ProviderDescriptor {
        AgentId::PI.descriptor()
    }

    fn hooks(&self) -> &'static dyn crate::hooks::HookInterpreter {
        &hooks::PiHooks
    }

    fn screen(&self) -> &'static dyn ScreenReader {
        &screen::PiScreen
    }

    fn accounts(&self) -> Option<&'static dyn crate::accounts::AccountOps> {
        Some(&accounts::PiAccounts)
    }

    fn records(&self) -> &'static dyn crate::records::RecordStore {
        &records::PiRecords
    }

    fn catalogue(&self) -> &'static dyn crate::agents::models::ModelCatalogue {
        &models::PiCatalogue
    }

    fn cli_packaging(&self) -> &'static super::CliPackaging {
        &super::CliPackaging {
            native_layouts: &[MANAGED_LAUNCHER_LAYOUT],
            // The vendor's installer's marker, beside the launcher's `bin` in whatever
            // configuration directory it was installed into.
            native_markers: &[&["..", "install", MANAGED_MARKER]],
            native_update: &["update", "--self"],
            native_latest: None,
            native_manifest: None,
            npm_packages: &[NPM_PACKAGE],
            homebrew_formula: None,
            homebrew_cask: None,
        }
    }

    /// Read from where Pi is installed: the npm package's `package.json`, found from its entry
    /// point (`<package>/dist/bundle/cli.js`, 1.5.2), or, for the vendor's installer, the release
    /// its launcher starts ([`managed_version`]).
    fn installed_version(&self, binary: &str) -> Option<super::AgentVersion> {
        let resolved = crate::hostfs::canonical(binary).ok()?;
        managed_version(&resolved).or_else(|| {
            use crate::platform::shell::{self, ResolvedBinary, Shim};

            match shell::shim_package(&ResolvedBinary::new(&resolved), NPM_PACKAGE) {
                Shim::Package(package) => package_version(&package.join("package.json")),
                _ => npm_version(&resolved),
            }
        })
    }

    /// Pi titles its terminal `π - <session name> - <folder>` once the session has a name (`/name`,
    /// `--name` or an extension's), and `π - <folder>` before, the folder being the working
    /// directory's last component. The session name is the task title; the bare form has none.
    ///
    /// Both the name and the folder may hold ` - ` themselves, so the folder is taken off by its
    /// own name, the first of `project_names`. A title that ends with none of them (a row read
    /// with no names to go by) is split at its last ` - `.
    fn task_title(&self, raw: &str, project_names: &[String]) -> Option<String> {
        let rest = raw.trim().strip_prefix("\u{3c0} - ")?;
        let named = |name: &str| {
            let name = name.trim();
            (!name.is_empty()).then(|| name.to_owned())
        };
        for folder in project_names {
            if rest == folder {
                return None;
            }
            if let Some(name) = rest
                .strip_suffix(folder.as_str())
                .and_then(|rest| rest.strip_suffix(" - "))
            {
                return named(name);
            }
        }
        let (name, _folder) = rest.rsplit_once(" - ")?;
        named(name)
    }

    /// The title ends with the working directory's name, which has to be known to take it off.
    fn names_project(&self) -> bool {
        true
    }

    fn extension_channel(&self, _spec: &LaunchSpec<'_>, _plan: &LaunchPlan) -> bool {
        true
    }

    /// The extension forwards Pi's `session_start` as Pi starts, once past the project-trust
    /// confirmation, before anything is written into it. An extension that Pi loads but that
    /// cannot set itself up (an extension interface changed under it) sends nothing, and the
    /// launch is then read as one without hooks.
    fn first_hook_due(&self, _spec: &LaunchSpec<'_>, _plan: &LaunchPlan) -> super::FirstHook {
        super::FirstHook::AtStart
    }

    /// `--session <id>` for a resume (after `--session-dir` where the record is kept in an earlier
    /// launch's records directory, [`resume_args`]), `--model`, and a new conversation's task as
    /// the plan gives it ([`task_args`]).
    fn bare_plan(&self, spec: &LaunchSpec<'_>) -> super::BarePlan {
        let mut args = Vec::new();
        if let Some(id) = spec.resume_agent_session_id {
            args.extend(resume_args(spec, id));
        }
        if let Some(model) = spec.model {
            args.push("--model".to_string());
            args.push(model.to_string());
        }
        let (task, first_message) = task_args(spec);
        args.extend(task);
        super::BarePlan {
            args,
            shaped_prompt: None,
            first_message,
        }
    }

    fn plan(&self, spec: &LaunchSpec<'_>) -> Result<LaunchPlan> {
        let agent = AgentId::PI;
        // Set only for a pinned directory; otherwise the shell snapshot's own value, if any,
        // stands.
        let env = super::pinned_config_dir_export(spec, agent)?
            .into_iter()
            .collect();
        let mut args = Vec::new();
        if spec.kind == crate::protocol::SessionKind::Console {
            args.push("--approve".to_owned());
        }
        match spec.resume_agent_session_id {
            Some(id) => args.extend(resume_args(spec, id)),
            None => {
                args.push("--session-id".to_string());
                args.push(spec.new_agent_session_id.to_string());
            }
        }
        if let Some(model) = spec.model {
            args.push("--model".to_string());
            args.push(model.to_string());
        }
        if let Some(effort) = spec.effort {
            args.push("--thinking".to_string());
            args.push(effort.to_string());
        }

        let role = spec.scratch.join(ROLE_FILE);
        replace_beside(
            &role,
            session_tools::role::role_description(
                spec.kind,
                spec.owner.as_ref(),
                agent,
                &spec.role_options(),
            )
            .as_bytes(),
            Permissions::Private,
        )
        .context("writing the session's role file")?;
        args.push("--append-system-prompt".to_string());
        args.push(role.to_string_lossy().into_owned());

        let extension = write_extension(spec)?;
        args.push("--extension".to_string());
        args.push(extension.to_string_lossy().into_owned());

        let (task, first_message) = task_args(spec);
        args.extend(task);

        Ok(LaunchPlan {
            args,
            env,
            first_message,
            ..LaunchPlan::default()
        })
    }
}

/// How a launch gives Pi a new conversation's task, the plan and the bare plan alike: the
/// arguments, and the session's first message where it is not one. Positional messages start the
/// first turn and stay interactive; after `--`, so a task that starts with `-` is not read as an
/// option. One that starts with `@` is read as a file to include even there, and Pi exits where
/// there is no such file (1.5.2), so it is queued as the session's first message instead: pasted
/// into the input box it is taken as text. A resume is given no task.
fn task_args(spec: &LaunchSpec<'_>) -> (Vec<String>, Option<String>) {
    match spec.task.filter(|_| spec.resume_agent_session_id.is_none()) {
        Some(task) if task.starts_with('@') => (Vec::new(), Some(task.to_string())),
        Some(task) => (vec!["--".to_string(), task.to_string()], None),
        None => (Vec::new(), None),
    }
}

/// How a launch resumes conversation `id`: `--session <id>`, after `--session-dir <dir>` where the
/// record is in a records directory an earlier launch of the session selected through
/// `PI_CODING_AGENT_SESSION_DIR` ([`records::kept_record_dir`]), since `--session` alone looks
/// only in the directory the current environment selects (1.5.2). Where it is in none of them, the
/// option is left out and Pi looks where a plain terminal's run would.
fn resume_args(spec: &LaunchSpec<'_>, id: &str) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(dir) = records::kept_record_dir(spec.record_roots, id) {
        args.push("--session-dir".to_string());
        args.push(dir.to_string_lossy().into_owned());
    }
    args.push("--session".to_string());
    args.push(id.to_string());
    args
}

/// Pi's npm package.
const NPM_PACKAGE: &str = "@earendil-works/pi-coding-agent";

/// Where the vendor's installer (`https://pi.dev/install.sh`, read on 2026-11-23) puts the
/// launcher it writes, under the default configuration directory: `~/.pi/agent/bin/pi`, which a
/// link in a directory on `PATH` (`~/.local/bin/pi`, say) may lead to. The launcher is a shell
/// script, which reads the version to start from `install/current-version` beside `bin` and
/// starts the release installed under `install/releases/<version>`, and `pi update --self`
/// updates that installation itself. A launcher under another configuration directory
/// (`PI_CODING_AGENT_DIR` set as the installer ran) is recognised by the installer's marker
/// beside its `bin` instead (`CliPackaging::native_markers`).
const MANAGED_LAUNCHER_LAYOUT: &[&str] = &[".pi", "agent", "bin"];

/// The marker the vendor's installer writes into the installation's root, with the fields it
/// checks before it takes a directory for one of its installations.
const MANAGED_MARKER: &str = "managed-install.json";

/// The version of the release a launcher of the vendor's installer starts: `resolved` is the
/// launcher, `<configuration directory>/bin/pi`, and the installation is `install` beside `bin`,
/// marked as the installer marks it. The version is read as the launcher reads it, from
/// `install/current-version`, and only where that release is installed.
fn managed_version(resolved: &std::path::Path) -> Option<super::AgentVersion> {
    let bin = resolved.parent()?;
    if bin.file_name()? != "bin" {
        return None;
    }
    let root = bin.parent()?.join("install");
    let marker: serde_json::Value = serde_json::from_str(
        &read_regular_text(&root.join(MANAGED_MARKER), SMALL_FILE_LIMIT).ok()?,
    )
    .ok()?;
    if marker.get("kind")?.as_str()? != "pi-managed-install"
        || marker.get("layout")?.as_str()? != "releases-v1"
    {
        return None;
    }
    let current = read_regular_text(&root.join("current-version"), SMALL_FILE_LIMIT).ok()?;
    let current = current.lines().next()?.trim();
    // As the launcher checks it: a version, never a path out of `releases`.
    if current.is_empty()
        || current == "."
        || current == ".."
        || !current
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'))
    {
        return None;
    }
    let release = root.join("releases").join(current);
    let manifest = release
        .join("node_modules")
        .join(NPM_PACKAGE)
        .join("package.json");
    package_version(&manifest)
}

/// The version of Pi's npm package whose file `resolved` is: the nearest `package.json` above it
/// that names the package, within the few levels its entry point lies below the package's root.
fn npm_version(resolved: &std::path::Path) -> Option<super::AgentVersion> {
    resolved
        .ancestors()
        .skip(1)
        .take(4)
        .find_map(|directory| package_version(&directory.join("package.json")))
}

/// The release version a `package.json` of Pi's npm package names, a pre-release's or a build's
/// suffix left off; `None` for any other package's.
fn package_version(manifest: &std::path::Path) -> Option<super::AgentVersion> {
    let text = read_regular_text(manifest, SMALL_FILE_LIMIT).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    if value.get("name")?.as_str()? != NPM_PACKAGE {
        return None;
    }
    let version = value.get("version")?.as_str()?;
    super::parse_version(version.split(['-', '+']).next()?)
}

/// How much longer than the hook mode's own deadline (`crate::adapter::HOOK_ROUND_TRIP_TIMEOUT`)
/// the extension waits for the hook script to exit before a run starts: the script's start and
/// exit.
const HOOK_WAIT_MARGIN: std::time::Duration = std::time::Duration::from_millis(500);

/// Writes Octoboard's extension and its data file into the session's scratch directory, and
/// returns the extension's path. The MCP server's timeout is set above Octoboard's own forwarding
/// limit, in seconds, the unit Pi takes it in. What the extension goes by of the daemon's own
/// facts is passed here rather than copied into it.
fn write_extension(spec: &LaunchSpec<'_>) -> Result<std::path::PathBuf> {
    use crate::session::pasted_image;
    let (command, args) = mcp_server_command(spec);
    let image_types: serde_json::Map<String, serde_json::Value> = pasted_image::FORMATS
        .iter()
        .map(|format| (format.extension.to_owned(), format.media_type.into()))
        .collect();
    let data = json!({
        // This script carries the launch ID on every event the extension forwards.
        "hook": spec.hook_script.to_string_lossy(),
        "report_turn_status": crate::platform::term::facts().host_renders,
        "channel": {
            "command": spec.self_exe,
            "args": ["channel", "--session", spec.session_id, "--port",
                &spec.daemon_port.to_string(), "--key-file", &spec.key_file.to_string_lossy()],
        },
        // How long a run's prompt waits for the hook script to exit, in milliseconds.
        "hook_wait_ms": (crate::adapter::HOOK_ROUND_TRIP_TIMEOUT + HOOK_WAIT_MARGIN).as_millis() as u64,
        // The session's guard script, where the session is kept to its own project.
        "guard": spec.guard.as_ref().map(|guard| guard.script.to_string_lossy()),
        "guard_timeout_ms": u64::from(super::GUARD_HOOK_TIMEOUT_SECS) * 1000,
        "file_writing_tools": hooks::FILE_WRITING_TOOLS,
        "mcp": {
            "name": MCP_SERVER_NAME,
            "command": command,
            "args": args,
            "timeout": MCP_TOOL_TIMEOUT_SECS,
        },
        // How the images Octoboard pastes into the session are kept, beside the extension.
        "pasted": {
            "directory": pasted_image::DIRECTORY,
            "prefix": pasted_image::NAME_PREFIX,
            "types": image_types,
        },
    });
    replace_beside(
        &spec.scratch.join(EXTENSION_DATA),
        data.to_string().as_bytes(),
        Permissions::Private,
    )
    .context("writing the data file of the session's Pi extension")?;
    let extension = spec.scratch.join(EXTENSION_FILE);
    replace_beside(&extension, EXTENSION.as_bytes(), Permissions::Private)
        .context("writing the session's Pi extension")?;
    Ok(extension)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::tests::spec_fixture;
    use crate::protocol::{error_code, CodedError};

    /// Pi's bare launch: `--session`, `--model`, and a new conversation's task after `--`, or as the
    /// first message where it starts with `@`; no extension, role or approval flag.
    #[test]
    fn its_bare_plan_names_the_conversation_the_model_and_the_task_only() {
        let mut fixture = crate::adapter::tests::spec_fixture();
        fixture.model = Some("model-a".to_string());
        let bare = |resume: Option<&str>, task: Option<&str>| {
            let mut spec = fixture.spec();
            spec.resume_agent_session_id = resume;
            spec.task = task;
            PiProvider.bare_plan(&spec)
        };
        assert_eq!(
            bare(Some("c-1"), None).args,
            ["--session", "c-1", "--model", "model-a"]
        );
        assert_eq!(
            bare(None, Some("do it")).args,
            ["--model", "model-a", "--", "do it"]
        );
        let file = bare(None, Some("@notes.md"));
        assert_eq!(file.args, ["--model", "model-a"]);
        assert_eq!(file.first_message.as_deref(), Some("@notes.md"));
    }

    /// The bare plan reads nothing it could fail on: over kept records directories that are gone
    /// or not directories at all it still names the conversation, with no `--session-dir`.
    #[test]
    fn its_bare_plan_is_built_over_kept_directories_that_cannot_be_read() {
        let fixture = spec_fixture();
        let not_a_dir = fixture.scratch.join("a-file");
        std::fs::write(&not_a_dir, "").unwrap();
        let roots = [fixture.scratch.join("gone"), not_a_dir];
        let mut spec = fixture.spec();
        spec.record_roots = &roots;
        spec.resume_agent_session_id = Some(KEPT);
        assert_eq!(PiProvider.bare_plan(&spec).args, ["--session", KEPT]);
    }

    const KEPT: &str = "7e57ab1e-1111-4222-8333-944455556666";

    /// Writes an empty record of conversation [`KEPT`] into `dir`, named as Pi names it.
    fn keep_record(dir: &std::path::Path) {
        put(
            dir.join(format!("2026-11-26T05-00-00-000Z_{KEPT}.jsonl")),
            "",
        );
    }

    /// A resume whose record is in a records directory an earlier launch selected names that
    /// directory with `--session-dir` before `--session`, in the plan and the bare plan alike,
    /// since `--session` alone looks only where the current environment points (1.5.2); one whose
    /// record is in none of them names none, and a new session never does.
    #[test]
    fn a_resume_names_the_kept_directory_that_holds_its_record() {
        let mut fixture = spec_fixture();
        fixture.new_agent_session_id = NEW.to_string();
        let earlier = fixture.scratch.join("earlier-records");
        let holds = fixture.scratch.join("kept-records");
        std::fs::create_dir_all(&earlier).unwrap();
        keep_record(&holds);
        let roots = [earlier.clone(), holds.clone()];
        let holds_arg = holds.to_string_lossy().into_owned();

        let mut spec = fixture.spec();
        spec.record_roots = &roots;
        spec.resume_agent_session_id = Some(KEPT);
        assert_eq!(
            PiProvider.plan(&spec).expect("plan").args,
            with_injection(
                &fixture.scratch,
                &["--session-dir", holds_arg.as_str(), "--session", KEPT],
                &[]
            )
        );
        assert_eq!(
            PiProvider.bare_plan(&spec).args,
            ["--session-dir", holds_arg.as_str(), "--session", KEPT]
        );

        // Not in any kept directory: Pi looks where the environment points, as in a terminal.
        let elsewhere = [earlier];
        spec.record_roots = &elsewhere;
        assert_eq!(PiProvider.bare_plan(&spec).args, ["--session", KEPT]);
        assert_eq!(
            PiProvider.plan(&spec).expect("plan").args,
            with_injection(&fixture.scratch, &["--session", KEPT], &[])
        );

        // A new session goes where the environment points, whatever is kept.
        let mut spec = fixture.spec();
        spec.record_roots = &roots;
        assert!(!PiProvider
            .plan(&spec)
            .expect("plan")
            .args
            .iter()
            .any(|arg| arg == "--session-dir"));
        assert!(PiProvider.bare_plan(&spec).args.is_empty());
    }

    /// Pi's first hook is the extension's `session_start`, as Pi starts.
    #[test]
    fn its_first_hook_comes_as_pi_starts() {
        let fixture = spec_fixture();
        let spec = fixture.spec();
        let plan = PiProvider.plan(&spec).expect("plan");
        assert_eq!(
            PiProvider.first_hook_due(&spec, &plan),
            crate::adapter::FirstHook::AtStart
        );
    }

    /// What Pi 1.5.2 writes when it exits as it starts: an option it does not take, an extension
    /// that throws while loading, beside the near-miss of a resume whose record is not found. All
    /// three are one line or two of plain output and nothing drawn, so each is an early exit by
    /// the daemon's reading, and none leaves a conversation to name on the screen.
    #[test]
    fn its_early_exits_are_captured_before_anything_is_drawn() {
        const UNKNOWN_OPTION: &[u8] =
            include_bytes!("../../../testdata/pi_unknown_option_1.5.2.bin");
        const EXTENSION_THROWS: &[u8] =
            include_bytes!("../../../testdata/pi_extension_throws_1.5.2.bin");
        const NOT_FOUND: &[u8] = include_bytes!("../../../testdata/pi_session_not_found_1.5.2.bin");
        let text = |bytes: &[u8]| String::from_utf8(bytes.to_vec()).expect("UTF-8");
        assert!(text(UNKNOWN_OPTION).contains("Error: Unknown option: --no-such-option"));
        assert!(text(EXTENSION_THROWS).contains("Failed to load extension"));
        assert!(text(NOT_FOUND).contains("No session found matching"));
        for capture in [UNKNOWN_OPTION, EXTENSION_THROWS, NOT_FOUND] {
            // No alternate screen, no bracketed paste, no title: nothing an interactive Pi does.
            let capture = text(capture);
            for drawn in ["\x1b[?1049h", "\x1b[?2004h", "\x1b]0;", "\x1b]7501"] {
                assert!(!capture.contains(drawn), "{drawn:?} in {capture:?}");
            }
        }
    }

    const NEW: &str = "new-agent-session";
    const RESUMED: &str = "resumed-session";

    /// `leading`, then what every launch injects: the role file and the extension, both in the
    /// session's scratch directory; then `trailing`.
    fn with_injection(
        scratch: &std::path::Path,
        leading: &[&str],
        trailing: &[&str],
    ) -> Vec<String> {
        let path = |name: &str| scratch.join(name).to_string_lossy().into_owned();
        let mut args: Vec<String> = leading.iter().map(|arg| arg.to_string()).collect();
        args.extend([
            "--append-system-prompt".to_string(),
            path(ROLE_FILE),
            "--extension".to_string(),
            path(EXTENSION_FILE),
        ]);
        args.extend(trailing.iter().map(|arg| arg.to_string()));
        args
    }

    #[test]
    fn trusts_the_console_directory_for_one_launch() {
        let mut fixture = spec_fixture();
        let project = PiProvider.plan(&fixture.spec()).expect("project plan");
        assert!(!project.args.iter().any(|arg| arg == "--approve"));

        fixture.kind = crate::protocol::SessionKind::Console;
        let console = PiProvider.plan(&fixture.spec()).expect("console plan");
        assert!(console.args.iter().any(|arg| arg == "--approve"));
    }

    /// A new session of Pi, with nothing else asked for.
    #[test]
    fn a_new_session_passes_its_id() {
        let mut fixture = spec_fixture();
        fixture.new_agent_session_id = NEW.to_string();
        let plan = PiProvider.plan(&fixture.spec()).expect("plan");
        assert_eq!(
            plan.args,
            with_injection(
                &fixture.scratch,
                &["--session-id", "new-agent-session"],
                &[]
            )
        );
    }

    /// A new session passes its model, its first task, and an effort only where the CLI has a flag
    /// for it: it passes the reasoning effort with `--thinking`.
    #[test]
    fn a_new_session_passes_the_model_the_effort_and_the_first_task() {
        let mut fixture = spec_fixture();
        fixture.new_agent_session_id = NEW.to_string();
        fixture.model = Some("probe-model".to_string());
        fixture.effort = Some("high".to_string());
        let mut spec = fixture.spec();
        spec.task = Some("first task");
        let plan = PiProvider.plan(&spec).expect("plan");
        assert_eq!(
            plan.args,
            with_injection(
                &fixture.scratch,
                &[
                    "--session-id",
                    "new-agent-session",
                    "--model",
                    "probe-model",
                    "--thinking",
                    "high",
                ],
                &["--", "first task"]
            )
        );
    }

    /// A resume names the conversation to continue, and neither starts a new id nor repeats the
    /// first task, whatever model and effort are set.
    #[test]
    fn a_resume_names_the_conversation_and_not_the_first_task() {
        let mut fixture = spec_fixture();
        fixture.new_agent_session_id = NEW.to_string();
        let mut spec = fixture.spec();
        spec.resume_agent_session_id = Some(RESUMED);
        spec.task = Some("first task");
        let plan = PiProvider.plan(&spec).expect("plan");
        assert_eq!(
            plan.args,
            with_injection(&fixture.scratch, &["--session", "resumed-session"], &[])
        );

        fixture.model = Some("probe-model".to_string());
        fixture.effort = Some("high".to_string());
        let mut spec = fixture.spec();
        spec.resume_agent_session_id = Some(RESUMED);
        spec.task = Some("first task");
        let plan = PiProvider.plan(&spec).expect("plan");
        assert_eq!(
            plan.args,
            with_injection(
                &fixture.scratch,
                &[
                    "--session",
                    "resumed-session",
                    "--model",
                    "probe-model",
                    "--thinking",
                    "high",
                ],
                &[]
            )
        );
    }

    /// An unpinned launch sets no environment, so the shell snapshot's own value of the directory
    /// variable is left to stand, and has no private server, no notice and nothing carried over.
    #[test]
    fn an_unpinned_launch_sets_no_environment() {
        let mut fixture = spec_fixture();
        fixture.shell_env.insert(
            "PI_CODING_AGENT_DIR".to_string(),
            "/somewhere/else".to_string(),
        );
        let plan = PiProvider.plan(&fixture.spec()).expect("plan");
        assert!(plan.env.is_empty());
        assert!(plan.private_server.is_none());
        assert!(!plan.resolves_approvals_itself);
        assert!(plan.notice.is_none());
        assert!(plan.carried_trust.is_none());
        assert!(plan.after_exit.is_none());
        assert!(plan.adoption.is_none());
    }

    /// A pinned account directory is exported through the layout's variable, for a new session and
    /// a resume alike.
    #[test]
    fn a_pinned_directory_is_exported_through_the_layouts_variable() {
        let mut fixture = spec_fixture();
        let dir = fixture.scratch.join("pi-account");
        std::fs::create_dir_all(&dir).expect("config dir");
        fixture.config_dir = Some(dir);
        let expected = vec![(
            "PI_CODING_AGENT_DIR".to_string(),
            fixture
                .scratch
                .join("pi-account")
                .to_string_lossy()
                .into_owned(),
        )];
        assert_eq!(
            PiProvider.plan(&fixture.spec()).expect("plan").env,
            expected
        );
        let mut spec = fixture.spec();
        spec.resume_agent_session_id = Some(RESUMED);
        assert_eq!(PiProvider.plan(&spec).expect("plan").env, expected);
    }

    /// The variable names the directory itself, so a pinned directory is never refused for its name.
    #[test]
    fn a_pinned_directory_is_exported_whatever_it_is_called() {
        let mut fixture = spec_fixture();
        let dir = fixture.scratch.join("any-name-at-all");
        std::fs::create_dir_all(&dir).expect("config dir");
        fixture.config_dir = Some(dir.clone());
        let plan = PiProvider.plan(&fixture.spec()).expect("plan");
        assert_eq!(
            plan.env,
            vec![(
                "PI_CODING_AGENT_DIR".to_string(),
                dir.to_string_lossy().into_owned()
            )]
        );
    }

    /// A pinned directory that has gone refuses a resume with the code every provider uses for it,
    /// and does not refuse a new session, which lets the agent create it.
    #[test]
    fn a_pinned_directory_that_has_gone_refuses_only_a_resume() {
        let mut fixture = spec_fixture();
        fixture.config_dir = Some(fixture.scratch.join("missing"));
        PiProvider
            .plan(&fixture.spec())
            .expect("a new session is not refused");

        let mut spec = fixture.spec();
        spec.resume_agent_session_id = Some(RESUMED);
        let err = PiProvider.plan(&spec).err().expect("refused");
        assert_eq!(
            err.downcast_ref::<CodedError>().expect("coded").code,
            error_code::CONFIG_DIR_UNREACHABLE
        );
        let message = err.to_string();
        assert!(message.contains("Pi"), "{message}");
        assert!(!message.contains("  "), "stray spaces in: {message}");
    }

    /// The model and the effort are each passed alone with the flag that takes them, for a new
    /// session and a resume: Pi's `provider/id` spelling for the model, `--thinking` for the effort.
    #[test]
    fn the_model_and_the_effort_are_passed_with_their_own_flags() {
        let mut fixture = spec_fixture();
        fixture.new_agent_session_id = NEW.to_string();
        fixture.model = Some("openai/gpt-4.1".to_string());
        let plan = PiProvider.plan(&fixture.spec()).expect("plan");
        assert_eq!(
            plan.args,
            with_injection(
                &fixture.scratch,
                &["--session-id", NEW, "--model", "openai/gpt-4.1"],
                &[]
            )
        );

        fixture.model = None;
        fixture.effort = Some("xhigh".to_string());
        let plan = PiProvider.plan(&fixture.spec()).expect("plan");
        assert_eq!(
            plan.args,
            with_injection(
                &fixture.scratch,
                &["--session-id", NEW, "--thinking", "xhigh"],
                &[]
            )
        );
    }

    /// A first task that starts with `-` stays a message: it comes after a bare `--`, as the one
    /// argument after it.
    #[test]
    fn a_first_task_that_looks_like_an_option_is_a_message() {
        let mut fixture = spec_fixture();
        fixture.new_agent_session_id = NEW.to_string();
        let mut spec = fixture.spec();
        spec.task = Some("--help me with this");
        let plan = PiProvider.plan(&spec).expect("plan");
        let tail = &plan.args[plan.args.len() - 2..];
        assert_eq!(tail, ["--", "--help me with this"]);
        assert_eq!(
            plan.args.iter().filter(|arg| *arg == "--").count(),
            1,
            "{:?}",
            plan.args
        );
    }

    fn written(fixture: &crate::adapter::tests::SpecFixture, name: &str) -> String {
        std::fs::read_to_string(fixture.scratch.join(name)).expect(name)
    }

    /// The role text goes in a file of the session's scratch directory for `--append-system-prompt`,
    /// and names Octoboard's tools as Pi's model sees them, `mcp__octoboard__<tool>`.
    #[test]
    fn the_role_text_is_written_to_the_file_the_flag_names() {
        let fixture = spec_fixture();
        let plan = PiProvider.plan(&fixture.spec()).expect("plan");
        let at = plan
            .args
            .iter()
            .position(|arg| arg == "--append-system-prompt")
            .expect("the flag");
        let path = std::path::PathBuf::from(&plan.args[at + 1]);
        assert_eq!(path, fixture.scratch.join("octoboard-role.md"));
        let role = std::fs::read_to_string(&path).expect("the role file");
        assert!(role.contains("mcp__octoboard__report"), "{role}");
        assert!(
            !role.contains("--append-system-prompt"),
            "the text, not the flag"
        );
    }

    /// The extension is the one `--extension` names, with the data file beside it that it reads:
    /// the hook script, and the session's Octoboard MCP server registered under its name, command
    /// and arguments, with a timeout above Octoboard's own forwarding limit, in seconds.
    #[test]
    fn the_extension_and_its_data_file_are_written_beside_each_other() {
        let mut fixture = spec_fixture();
        fixture.session_id = "session-for-pi".to_string();
        let spec = fixture.spec();
        let plan = PiProvider.plan(&spec).expect("plan");
        let at = plan
            .args
            .iter()
            .position(|arg| arg == "--extension")
            .expect("the flag");
        assert_eq!(
            std::path::PathBuf::from(&plan.args[at + 1]),
            fixture.scratch.join("octoboard-pi.js")
        );

        let extension = written(&fixture, "octoboard-pi.js");
        assert!(
            extension.contains("octoboard-pi.json"),
            "reads its data file"
        );
        assert!(extension.contains("registerMcpServer"));
        assert!(extension.contains("session_start"));
        // A session written after its `session_start` is named once a run has ended.
        assert!(extension.contains("agent_end"));
        // The tools are its own to register where the user's `mcp.json` names the server.
        assert!(extension.contains("registerTool"));
        assert!(extension.contains("mcp.json"));
        // What a run said is forwarded as Pi settles, under the field the hooks read.
        assert!(extension.contains("agent_settled"));
        assert!(extension.contains("last_assistant_message"));

        let data: serde_json::Value =
            serde_json::from_str(&written(&fixture, "octoboard-pi.json")).expect("JSON");
        assert_eq!(data["hook"], fixture.hook_script.to_string_lossy().as_ref());
        assert_eq!(
            data["channel"],
            json!({
                "command": spec.self_exe,
                "args": ["channel", "--session", spec.session_id, "--port",
                    &spec.daemon_port.to_string(), "--key-file", &spec.key_file.to_string_lossy()],
            })
        );
        assert_eq!(data["mcp"]["name"], "octoboard");
        let (command, args) = crate::adapter::mcp_server_command(&spec);
        assert_eq!(data["mcp"]["command"], command.as_str());
        assert_eq!(data["mcp"]["args"], serde_json::json!(args));
        assert!(args.contains(&"session-for-pi".to_string()), "{args:?}");
        assert!(args.contains(&fixture.mcp_token), "{args:?}");
        // The production constant, which sits above `session_tools::CALL_TIMEOUT`.
        assert_eq!(data["mcp"]["timeout"], MCP_TOOL_TIMEOUT_SECS);
    }

    /// The daemon's own facts the extension goes by come in its data file, from where the daemon
    /// keeps them: how long a prompt waits for the hook script, a little past the hook mode's own
    /// deadline; the tools the guard is asked about and its timeout, as the other agents' guard
    /// hooks are given; and how pasted images are named and typed.
    #[test]
    fn the_data_file_gives_the_extension_the_daemons_own_facts() {
        use crate::session::pasted_image;
        let fixture = spec_fixture();
        PiProvider.plan(&fixture.spec()).expect("plan");
        let data: serde_json::Value =
            serde_json::from_str(&written(&fixture, "octoboard-pi.json")).expect("JSON");
        let wait = data["hook_wait_ms"].as_u64().expect("milliseconds");
        assert!(
            u128::from(wait) > crate::adapter::HOOK_ROUND_TRIP_TIMEOUT.as_millis(),
            "{wait}"
        );
        assert_eq!(
            data["guard_timeout_ms"],
            u64::from(crate::adapter::GUARD_HOOK_TIMEOUT_SECS) * 1000
        );
        assert_eq!(data["file_writing_tools"], json!(hooks::FILE_WRITING_TOOLS));
        assert_eq!(data["pasted"]["directory"], pasted_image::DIRECTORY);
        assert_eq!(data["pasted"]["prefix"], pasted_image::NAME_PREFIX);
        for format in &pasted_image::FORMATS {
            assert_eq!(data["pasted"]["types"][format.extension], format.media_type);
        }
        assert_eq!(
            data["pasted"]["types"].as_object().map(|types| types.len()),
            Some(pasted_image::FORMATS.len())
        );
    }

    /// The one part of a pasted image's name the extension does not take from its data file is
    /// the two runs of digits after the prefix, which its pattern spells out: the daemon's names
    /// have exactly that shape.
    #[test]
    fn the_extensions_pattern_has_the_shape_of_the_daemons_image_names() {
        assert!(EXTENSION.contains(r"${literally(pasted.prefix)}\\d+-\\d+\\.("));
        let name = "paste-1795443-7.png";
        assert_eq!(
            crate::session::pasted_image::saved_name_len(name),
            Some(name.len())
        );
        assert_eq!(
            crate::session::pasted_image::saved_name_len("paste-1795443.png"),
            None
        );
    }

    /// The data file names the session's MCP server and its token, and the role file is the
    /// session's own: all three files are readable by their owner alone.
    #[cfg(unix)]
    #[test]
    fn the_files_written_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = spec_fixture();
        PiProvider.plan(&fixture.spec()).expect("plan");
        for name in ["octoboard-role.md", "octoboard-pi.js", "octoboard-pi.json"] {
            let mode = std::fs::metadata(fixture.scratch.join(name))
                .expect(name)
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "{name}");
        }
    }

    /// A resume writes the role and the extension again: Pi builds its system prompt and loads its
    /// extensions afresh on every start, so each launch brings its own.
    #[test]
    fn a_resume_writes_the_role_and_the_extension_again() {
        let fixture = spec_fixture();
        let names = ["octoboard-role.md", "octoboard-pi.js", "octoboard-pi.json"];
        PiProvider.plan(&fixture.spec()).expect("plan");
        for name in names {
            std::fs::remove_file(fixture.scratch.join(name)).expect(name);
        }
        let mut spec = fixture.spec();
        spec.resume_agent_session_id = Some(RESUMED);
        PiProvider.plan(&spec).expect("plan");
        for name in names {
            assert!(fixture.scratch.join(name).is_file(), "{name}");
        }
    }

    /// A first task that starts with `@` is not an argument, where Pi would read it as a file to
    /// include: it is the session's first message, whole, and nothing follows the injection. A
    /// resume has neither.
    #[test]
    fn a_first_task_that_starts_with_an_at_sign_is_the_first_message() {
        let mut fixture = spec_fixture();
        fixture.new_agent_session_id = NEW.to_string();
        let mut spec = fixture.spec();
        spec.task = Some("@docs/plan.md is the plan to follow");
        let plan = PiProvider.plan(&spec).expect("plan");
        assert_eq!(
            plan.first_message.as_deref(),
            Some("@docs/plan.md is the plan to follow")
        );
        assert_eq!(
            plan.args,
            with_injection(&fixture.scratch, &["--session-id", NEW], &[])
        );

        spec.task = Some("mail @someone about it");
        let plan = PiProvider.plan(&spec).expect("plan");
        assert!(plan.first_message.is_none());
        assert_eq!(
            plan.args[plan.args.len() - 2..],
            ["--", "mail @someone about it"]
        );

        spec.task = Some("@docs/plan.md");
        spec.resume_agent_session_id = Some(RESUMED);
        let plan = PiProvider.plan(&spec).expect("plan");
        assert!(plan.first_message.is_none());
        assert!(!plan.args.iter().any(|arg| arg.starts_with('@')));
    }

    /// The folder is taken off the title by its own name, so a session name and a folder that
    /// hold ` - ` themselves read right, and a folder alone is no title.
    #[test]
    fn the_task_title_is_read_against_the_folders_name() {
        let names = |names: &[&str]| {
            names
                .iter()
                .map(|name| name.to_string())
                .collect::<Vec<_>>()
        };
        let title = |raw: &str, folders: &[&str]| PiProvider.task_title(raw, &names(folders));
        assert!(PiProvider.names_project());
        for (raw, folders, expected) in [
            // Measured on 1.5.2 in a folder named `my - proj`.
            ("\u{3c0} - my - proj", &["my - proj"][..], None),
            (
                "\u{3c0} - Fix - the build - my - proj",
                &["my - proj"][..],
                Some("Fix - the build"),
            ),
            (
                "\u{3c0} - Fix - the build - proj",
                &["proj"][..],
                Some("Fix - the build"),
            ),
            ("\u{3c0} - proj - proj", &["proj"][..], Some("proj")),
            // The working directory's name comes first and wins over the repository's.
            ("\u{3c0} - a - b - wt", &["wt", "b - wt"][..], Some("a - b")),
            ("\u{3c0} - Fix - elsewhere", &["proj"][..], Some("Fix")),
            ("\u{3c0} -  - proj", &["proj"][..], None),
            ("Fix - proj", &["proj"][..], None),
        ] {
            assert_eq!(title(raw, folders).as_deref(), expected, "{raw:?}");
        }
    }

    /// With no folder name to go by, a named session's title is split at its last ` - `: `π -
    /// <session name> - <folder>` gives the session name, and `π - <folder>` none.
    #[test]
    fn the_task_title_is_the_session_name_of_pis_terminal_title() {
        let title = |raw: &str| PiProvider.task_title(raw, &[]);
        for (raw, expected) in [
            ("\u{3c0} - Fix the build - octoboard", Some("Fix the build")),
            (
                "  \u{3c0} - Fix the build - octoboard \n",
                Some("Fix the build"),
            ),
            (
                "\u{3c0} - Fix - the build - octoboard",
                Some("Fix - the build"),
            ),
            ("\u{3c0} - octoboard", None),
            ("\u{3c0} -  - octoboard", None),
            ("\u{3c0} - ", None),
            ("Fix the build - octoboard", None),
            ("octoboard", None),
            ("", None),
        ] {
            assert_eq!(title(raw).as_deref(), expected, "{raw:?}");
        }
    }

    /// Pi is installed from its npm package or by the vendor's installer, whose launcher lies
    /// under `~/.pi/agent/bin`, or under another configuration directory beside the installer's
    /// marker, and updates its installation with `pi update --self`; its latest version is the npm
    /// package's.
    #[test]
    fn its_cli_is_an_npm_package_or_the_installers_launcher() {
        let packaging = PiProvider.cli_packaging();
        assert_eq!(packaging.npm_packages, ["@earendil-works/pi-coding-agent"]);
        assert_eq!(packaging.native_layouts, &[&[".pi", "agent", "bin"][..]]);
        assert_eq!(
            packaging.native_markers,
            &[&["..", "install", "managed-install.json"][..]]
        );
        assert_eq!(packaging.native_update, &["update", "--self"]);
        assert!(packaging.native_latest.is_none());
        assert!(packaging.homebrew_formula.is_none());
        assert!(packaging.homebrew_cask.is_none());
        use crate::protocol::CliInstallMethod;
        let method = |path: &str| {
            crate::agents::cli::install::Install::of_path(packaging, std::path::Path::new(path))
                .method
        };
        assert_eq!(
            method("/Users/u/.pi/agent/bin/pi"),
            CliInstallMethod::Native
        );
        assert_eq!(
            method(
                "/usr/local/lib/node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js"
            ),
            CliInstallMethod::Npm
        );
        assert_eq!(
            method("/Users/u/elsewhere/bin/pi"),
            CliInstallMethod::Unknown
        );

        // A launcher under a configuration directory of any name, reached through the link the
        // installer puts on `PATH`, is the installer's by the marker beside its `bin`.
        let fixture = crate::test_support::ScratchDir::new("pi-managed-elsewhere");
        let agent = fixture.join("pi-config");
        put(agent.join("bin/pi"), "#!/bin/sh\n");
        let binary = crate::platform::shell::ResolvedBinary::new(agent.join("bin/pi"));
        let install = || crate::agents::cli::install::Install::of_binary(packaging, &binary);
        assert_eq!(install().method, CliInstallMethod::Unknown);
        put(
            agent.join("install/managed-install.json"),
            r#"{"kind":"pi-managed-install","schemaVersion":1,"layout":"releases-v1"}"#,
        );
        assert_eq!(install().method, CliInstallMethod::Native);
        if crate::platform::fs::file_links_available() {
            let link = fixture.join("home/.local/bin/pi");
            std::fs::create_dir_all(link.parent().unwrap()).unwrap();
            crate::test_support::symlink(agent.join("bin/pi"), &link).unwrap();
            let through_link = crate::platform::shell::ResolvedBinary::new(&link);
            assert_eq!(
                crate::agents::cli::install::Install::of_binary(packaging, &through_link).method,
                CliInstallMethod::Native
            );
        }
    }

    /// Writes `text` to `path`, making its directory.
    fn put(path: std::path::PathBuf, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// The version is read without running Pi: from the npm package above its entry point, and
    /// for the vendor's installer from the release its launcher starts.
    #[test]
    fn the_installed_version_is_read_from_where_pi_is_installed() {
        let fixture = crate::test_support::ScratchDir::new("pi-installed-version");
        let version =
            |path: std::path::PathBuf| PiProvider.installed_version(path.to_str().unwrap());
        let manifest = |version: &str| {
            format!(r#"{{"name":"@earendil-works/pi-coding-agent","version":"{version}"}}"#)
        };

        // npm: `<package>/dist/bundle/cli.js`, as 1.5.2 lays it out.
        let package = fixture.join("lib/node_modules/@earendil-works/pi-coding-agent");
        put(package.join("dist/bundle/cli.js"), "");
        put(package.join("package.json"), &manifest("1.5.2"));
        assert_eq!(version(package.join("dist/bundle/cli.js")), Some((1, 5, 2)));
        put(fixture.join("other/dist/bundle/cli.js"), "");
        put(
            fixture.join("other/package.json"),
            r#"{"name":"another","version":"9.9.9"}"#,
        );
        assert_eq!(version(fixture.join("other/dist/bundle/cli.js")), None);

        // The installer: the launcher in `bin`, the installation in `install` beside it.
        let agent = fixture.join("home/.pi/agent");
        let install = agent.join("install");
        put(agent.join("bin/pi"), "#!/bin/sh\n");
        put(
            install.join("managed-install.json"),
            r#"{"kind":"pi-managed-install","schemaVersion":1,"layout":"releases-v1"}"#,
        );
        put(install.join("current-version"), "1.6.0\n");
        put(
            install
                .join("releases/1.6.0/node_modules/@earendil-works/pi-coding-agent/package.json"),
            &manifest("1.6.0-beta.1"),
        );
        assert_eq!(version(agent.join("bin/pi")), Some((1, 6, 0)));

        // A link on `PATH` to the launcher reads the same.
        #[cfg(unix)]
        {
            std::fs::create_dir_all(fixture.join("home/.local/bin")).unwrap();
            std::os::unix::fs::symlink(agent.join("bin/pi"), fixture.join("home/.local/bin/pi"))
                .unwrap();
            assert_eq!(version(fixture.join("home/.local/bin/pi")), Some((1, 6, 0)));
        }

        // A version that is not one, a release that is not there, and a marker that is not the
        // installer's read as nothing.
        put(install.join("current-version"), "../../escape\n");
        assert_eq!(version(agent.join("bin/pi")), None);
        put(install.join("current-version"), "1.7.0\n");
        assert_eq!(version(agent.join("bin/pi")), None);
        put(install.join("current-version"), "1.6.0\n");
        put(
            install.join("managed-install.json"),
            r#"{"kind":"something-else"}"#,
        );
        assert_eq!(version(agent.join("bin/pi")), None);
    }

    /// The data file names the session's guard script where the session is kept to its own
    /// project, and none otherwise, so the extension guards nothing then.
    #[test]
    fn the_data_file_names_the_guard_script_only_where_there_is_one() {
        let fixture = spec_fixture();
        PiProvider.plan(&fixture.spec()).expect("plan");
        let data: serde_json::Value =
            serde_json::from_str(&written(&fixture, "octoboard-pi.json")).expect("JSON");
        assert!(data["guard"].is_null(), "{data}");

        let list = fixture.scratch.join("cross-project-guard.json");
        let script = fixture.scratch.join("guard-hook");
        let mut spec = fixture.spec();
        spec.guard = Some(crate::adapter::GuardHook {
            exe: "/opt/octoboard/install/octoboardd",
            list: &list,
            script: &script,
        });
        PiProvider.plan(&spec).expect("plan");
        let data: serde_json::Value =
            serde_json::from_str(&written(&fixture, "octoboard-pi.json")).expect("JSON");
        assert_eq!(data["guard"], script.to_string_lossy().as_ref());
        let extension = written(&fixture, "octoboard-pi.js");
        assert!(extension.contains("tool_call"));
        assert!(extension.contains("message_start"));
    }
}
