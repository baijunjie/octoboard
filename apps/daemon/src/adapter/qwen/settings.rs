//! The settings file a Qwen Code launch is given in place of the system settings file: Octoboard's
//! hooks, and the five interface settings its reading of and writing to the terminal depend on,
//! over whatever the file it stands in for holds.
//!
//! Qwen Code reads system settings from the file `QWEN_CODE_SYSTEM_SETTINGS_PATH` names, else from
//! a fixed path per platform, and puts them above the user's and the project's. Measured on
//! 0.28.0: hook lists of the layers are added together, so the user's own hooks still run beside
//! these; hooks of this layer need no approval and run in a folder that is not trusted; and a
//! plain setting here wins over the user's.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::{json, Map, Value};

use super::settings_file::{self, SettingsFile};
use crate::adapter::files::{LARGE_FILE_LIMIT, SMALL_FILE_LIMIT};
use crate::adapter::hooks::{guard_command_fired_by, hook_command_fired_by};
use crate::adapter::{GuardHook, LaunchSpec, COMMAND_HOOK_TIMEOUT_SECS, GUARD_HOOK_TIMEOUT_SECS};
use crate::hostfs::replace::{replace_beside, Permissions};
use crate::platform::{shell::Dialect, HostOs};
use crate::protocol::{Code, CodedError};
use crate::providers::qwen;

/// The variable that names the system settings file.
pub(super) const SYSTEM_SETTINGS_VAR: &str = "QWEN_CODE_SYSTEM_SETTINGS_PATH";

/// The variable that names the system defaults file.
pub(super) const SYSTEM_DEFAULTS_VAR: &str = "QWEN_CODE_SYSTEM_DEFAULTS_PATH";

/// The system defaults file's name, beside the system settings file.
const SYSTEM_DEFAULTS_FILE: &str = "system-defaults.json";

/// The variable every hook Octoboard registers is given, in the hook's own `env`: what a hook of
/// Octoboard's, and with it a settings file a launch wrote, is recognised by. A marker in the file
/// rather than a guess from its path, since a path says nothing of a file an administrator keeps
/// and the hook's `env` is a place Qwen Code reads and nobody else would put this name in.
const OWN_HOOK_MARK: &str = "OCTOBOARD_SESSION_HOOK";

/// The settings file of a launch, in the session's scratch directory.
const SETTINGS_FILE: &str = "qwen-settings.json";

/// The events Octoboard's hook is registered for: the ones [`super::hooks`] reads. `SessionStart`
/// is left out on purpose (see [`super`]), and so are `PermissionRequest` and `Notification`,
/// whose waits are read off Qwen Code's status reports instead, else off the terminal title.
pub(super) const HOOK_EVENTS: &[&str] = &[
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "Stop",
    "StopFailure",
    "SubagentStart",
    "SubagentStop",
];

/// Where Qwen Code looks for the system settings file when no variable names one, on the host the
/// daemon and its sessions run on.
fn default_system_settings() -> &'static Path {
    system_settings_on(HostOs::current())
}

/// Where Qwen Code looks for the system settings file on a host of operating system `os` when no
/// variable names one (the three paths are read from its code, 0.28.3).
fn system_settings_on(os: HostOs) -> &'static Path {
    Path::new(match os {
        HostOs::Macos => "/Library/Application Support/QwenCode/settings.json",
        HostOs::Windows => r"C:\ProgramData\qwen-code\settings.json",
        HostOs::Linux => "/etc/qwen-code/settings.json",
    })
}

/// The system settings file a session started in `shell_env` would read without Octoboard: the
/// one that environment names, else the platform's.
///
/// A file an earlier launch of Octoboard's wrote is never that file ([`is_launch_file`]): the
/// environment names one when the daemon was itself started from a Qwen Code session's shell tool
/// (a development daemon run from a session, say), whose launch set the variable. The platform's
/// file is taken then, as what stood behind that launch file.
///
/// A daemon's snapshot already puts back what an outer session's launch set
/// (`crate::env_shell::outside_session`), by comparing each value with the one the launch
/// recorded. This check by the file stays for where that comparison misses: a value the CLI or a
/// shell profile passed on in another spelling, or a session launched by a build that recorded
/// nothing.
///
/// A relative path is taken as it is, so against the daemon's working directory, where Qwen Code
/// takes it against the session's.
pub(super) fn system_settings_path(shell_env: &HashMap<String, String>) -> PathBuf {
    shell_env
        .get(SYSTEM_SETTINGS_VAR)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .filter(|path| !is_launch_file(path))
        .unwrap_or_else(|| default_system_settings().to_path_buf())
}

/// The most of the system settings file, and of the system defaults file, that is read: a launch
/// carries the system settings file within it, and everything that reads a system file for what a
/// launch would see (the hooks-off check, the model list, the configured model) reads it within
/// the same size, so that none of them sees a file the enhanced launch cannot use.
pub(super) const SYSTEM_FILE_LIMIT: usize = SMALL_FILE_LIMIT;

/// Whether `path` is a settings file a launch of Octoboard's wrote for a session, and no system
/// settings file of the host's.
///
/// - A file that is there is one exactly when it holds a hook marked as Octoboard's
///   ([`is_own_hook`]), which every launch file does and nothing an administrator writes would.
/// - A file that is not there cannot be asked. It is taken for one when it is named as a launch
///   file is and lies where one does, `run/<session>/qwen-settings.json`: the scratch directory
///   of a session that has ended. A file elsewhere that is missing is the host's own to be
///   missing.
fn is_launch_file(path: &Path) -> bool {
    match settings_file::read(path, SYSTEM_FILE_LIMIT) {
        SettingsFile::Settings(settings) => holds_own_hook(&settings),
        SettingsFile::Missing => {
            // The directory the scratch directories are in, by its name: the file may be another
            // daemon's, under a data directory of its own.
            let run_dir = crate::paths::run_dir();
            let in_run_dir = path
                .parent()
                .and_then(Path::parent)
                .and_then(Path::file_name)
                .is_some_and(|dir| Some(dir) == run_dir.file_name());
            in_run_dir && path.file_name().is_some_and(|name| name == SETTINGS_FILE)
        }
        SettingsFile::Unusable(_) => false,
    }
}

/// Whether a hook entry of a settings file is one Octoboard registered: it carries the marker in
/// its `env`.
fn is_own_hook(hook: &Value) -> bool {
    hook.get("env")
        .and_then(|env| env.get(OWN_HOOK_MARK))
        .is_some()
}

/// Whether `settings` register a hook of Octoboard's for any event.
fn holds_own_hook(settings: &Map<String, Value>) -> bool {
    hook_groups(settings).any(|group| {
        group
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|hooks| hooks.iter().any(is_own_hook))
    })
}

/// Every hook group `settings` register, of every event.
fn hook_groups(settings: &Map<String, Value>) -> impl Iterator<Item = &Value> {
    settings
        .get("hooks")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(Map::values)
        .filter_map(Value::as_array)
        .flatten()
}

/// `settings` without the hooks Octoboard registered for another session, and without the groups
/// and events that held nothing else. Whatever file is carried over, no launch's hooks reach the
/// next launch's file, where they would post this session's events to that other session.
fn without_own_hooks(mut settings: Map<String, Value>) -> Map<String, Value> {
    let Some(events) = settings.get_mut("hooks").and_then(Value::as_object_mut) else {
        return settings;
    };
    for groups in events.values_mut().filter_map(Value::as_array_mut) {
        groups.retain_mut(|group| {
            let Some(hooks) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                return true;
            };
            let before = hooks.len();
            hooks.retain(|hook| !is_own_hook(hook));
            // A group is dropped only when taking Octoboard's hooks out is what emptied it.
            hooks.len() == before || !hooks.is_empty()
        });
    }
    events.retain(|_, groups| groups.as_array().is_none_or(|groups| !groups.is_empty()));
    settings
}

/// The system defaults file such a session would read: the one that environment names, else
/// `system-defaults.json` beside its system settings file. Qwen Code looks for it beside whatever
/// system settings file it is given, so a launch that names another has to name this one too, or
/// the layer is lost.
pub(super) fn system_defaults_path(shell_env: &HashMap<String, String>) -> PathBuf {
    shell_env
        .get(SYSTEM_DEFAULTS_VAR)
        .filter(|path| !path.is_empty())
        .map_or_else(
            || system_settings_path(shell_env).with_file_name(SYSTEM_DEFAULTS_FILE),
            PathBuf::from,
        )
}

/// The content of the system settings file this launch's file stands in for, empty when there is
/// none. A file that is there and cannot be used is an error, which makes the enhanced launch
/// unavailable: it is where an administrator denies permissions, locks MCP servers down and sets
/// the sandbox, and a launch file written without it would drop that layer without a word. The
/// session then starts bare, naming no settings file, so Qwen Code reads and reports the file
/// itself, as in a plain terminal. That goes for a file over the size read here too, though Qwen
/// Code has no such limit: leaving the file to Qwen Code is the safe side.
fn replaced_settings(spec: &LaunchSpec<'_>) -> Result<Map<String, Value>> {
    carried_settings(&system_settings_path(spec.shell_env))
}

/// What the settings file at `path` gives a launch's file to carry: its content without any hook of
/// Octoboard's ([`without_own_hooks`]), nothing when it is not there, and an error when it is there
/// and cannot be used, which makes the enhanced launch unavailable ([`replaced_settings`]).
/// [`system_settings_path`] already keeps a launch file from being asked for here; the hooks are
/// taken out all the same, so that a file holding one by any other route, the platform's own
/// included, cannot hand it on.
fn carried_settings(path: &Path) -> Result<Map<String, Value>> {
    match settings_file::read(path, SYSTEM_FILE_LIMIT) {
        SettingsFile::Settings(settings) => Ok(without_own_hooks(settings)),
        SettingsFile::Missing => Ok(Map::new()),
        SettingsFile::Unusable(why) => Err(CodedError::raised(
            Code::of_provider(qwen::SYSTEM_SETTINGS_UNUSABLE),
            format!(
                "Qwen Code's system settings file `{}` cannot be used ({why}), so Octoboard's \
                 settings file was not written",
                path.display()
            ),
            &[("path", &path.to_string_lossy())],
        )),
    }
}

/// Adds a filename to the operator's effective list. A workspace list cannot be promoted into
/// the system layer: Qwen Code applies workspace settings only after its trust check.
fn add_claude_instructions(spec: &LaunchSpec<'_>, system: &mut Map<String, Value>) {
    let account = crate::adapter::account_dir(
        crate::protocol::AgentId::QWEN,
        spec.config_dir,
        spec.shell_env,
    );
    let read_names = |file: &Path, limit| {
        let layer = settings_file::read(file, limit).settings()?;
        context_filenames(layer.as_object()?)
    };
    let mut files = context_filenames(system)
        .or_else(|| read_names(&account.join(qwen::QWEN_SETTINGS_FILE), LARGE_FILE_LIMIT))
        .or_else(|| read_names(&system_defaults_path(spec.shell_env), SYSTEM_FILE_LIMIT))
        .filter(|files| !files.is_empty())
        .unwrap_or_else(|| vec![json!("QWEN.md"), json!("AGENTS.md")]);
    let claude = json!("CLAUDE.md");
    if !files.contains(&claude) {
        files.push(claude);
    }
    object_entry(system, "context").insert("fileName".into(), Value::Array(files));
}

fn context_filenames(layer: &Map<String, Value>) -> Option<Vec<Value>> {
    let names = layer
        .get("context")
        .and_then(|context| context.get("fileName"))
        .or_else(|| layer.get("contextFileName"))?;
    if let Some(names) = names.as_array() {
        return Some(names.clone());
    }
    let name = names.as_str()?.trim();
    Some(if name.is_empty() {
        Vec::new()
    } else {
        vec![json!(name)]
    })
}

/// The settings of a launch: `replaced` with Octoboard's hook added to each event's list, and the
/// interface settings set.
fn settings_json(spec: &LaunchSpec<'_>, mut replaced: Map<String, Value>) -> Value {
    // The payload names its event, so one command serves every event.
    let os = HostOs::current();
    let command = own_session_only_on(os, spec);
    let hooks = object_entry(&mut replaced, "hooks");
    for event in HOOK_EVENTS {
        let mut group = json!({
            "matcher": "",
            "hooks": [{
                "type": "command",
                "command": command,
                "timeout": COMMAND_HOOK_TIMEOUT_SECS,
                // What tells this hook, and the file it is in, as Octoboard's.
                "env": { OWN_HOOK_MARK: "1" },
            }],
        });
        set_hook_shell(os, &mut group);
        push_group(hooks, event, group);
    }
    // The cross-project guard decides a file write before it runs: a group of its own for the
    // file-writing tools, with the guard's own timeout, and its decision on stdout.
    if let Some(guard) = &spec.guard {
        let mut group = json!({
            "matcher": GUARD_MATCHER,
            "hooks": [{
                "type": "command",
                "command": guard_only_for_own_session_on(os, spec, guard),
                "timeout": GUARD_HOOK_TIMEOUT_SECS,
                "env": { OWN_HOOK_MARK: "1" },
            }],
        });
        set_hook_shell(os, &mut group);
        push_group(hooks, "PreToolUse", group);
    }
    // Qwen Code's own memory across sessions (`memory.enableManagedAutoMemory`, and the
    // consolidation that runs on it) is switched off in this layer, which outranks the user's
    // (0.28.3: no memory section in the request, no extraction call, no memory directory).
    if spec.native_memory_off {
        let memory = object_entry(&mut replaced, "memory");
        memory.insert("enableManagedAutoMemory".to_string(), json!(false));
        memory.insert("enableManagedAutoDream".to_string(), json!(false));
    }
    if spec.settings.switch(qwen::CLAUDE_INSTRUCTIONS) {
        add_claude_instructions(spec, &mut replaced);
    }
    let ui = object_entry(&mut replaced, "ui");
    // The status glyph the title starts with is what a wait on the user, a running turn and its
    // return to the prompt are read from where Qwen Code makes no status report
    // (`super::screen`).
    ui.insert("hideWindowTitle".to_string(), json!(false));
    ui.insert("showStatusInTitle".to_string(), json!(true));
    // A suggested next prompt is drawn in the input box like text typed there, and would be taken
    // for a draft.
    ui.insert("enableFollowupSuggestions".to_string(), json!(false));
    // The feedback question shown after some turns takes a digit typed next as its answer, so a
    // message that starts with one would lose it.
    ui.insert("enableUserFeedback".to_string(), json!(false));
    // The scrollbar drawn in the last column while the history scrolls (`ui.showScrollbar`, on by
    // default, 0.28) would sit at the end of the rows a prompt is read from; the reader still takes
    // it off a row (`super::interaction`), for a Qwen Code that does not know the setting.
    ui.insert("showScrollbar".to_string(), json!(false));
    Value::Object(replaced)
}

/// Adds `group` to the groups of `event` in `hooks`, in place of anything there that is not a list
/// of groups.
fn push_group(hooks: &mut Map<String, Value>, event: &str, group: Value) {
    match hooks.get_mut(event).and_then(Value::as_array_mut) {
        Some(groups) => groups.push(group),
        None => {
            hooks.insert(event.to_string(), json!([group]));
        }
    }
}

/// The hook command of a launch, made to post only for the Qwen Code this launch started, and to
/// show the user nothing.
///
/// The launch's settings file is named by a variable every process Qwen Code starts inherits, so a
/// Qwen Code started inside the session loads it too, and its hooks would report that other
/// conversation's prompt, its end and its id as this session's. That goes for one its shell tool
/// starts, one an MCP server starts and one a hook command starts (0.28.3): only the first has
/// `QWEN_CODE` in its environment, so no variable tells them all from the session. What does is the
/// process that fires the hook: the session's own was started with the launch's MCP configuration
/// file on its command line and a nested one was not. So the hook is given that file's path as
/// its mark (`hook_command_fired_by`), and the hook program reads the command line itself, from
/// the process table, with nothing started for it. Measured on macOS (0.28.3): Qwen Code runs a
/// hook as `bash -c <command>`, a child of its own, so the first process above the command's
/// shells is the Qwen Code that fired it. The session id is no test: it changes on `/clear` and
/// `/new`.
///
/// Qwen Code shows what a failing hook printed under the turn's reply ("Stop says: Warning: …",
/// 0.28.3), so the command runs in a subshell whose output is dropped, and the whole exits 0.
fn own_session_only_on(os: HostOs, spec: &LaunchSpec<'_>) -> String {
    let dialect = hook_dialect(os);
    let mark = spec.scratch.join(super::MCP_CONFIG_FILE);
    let command = hook_command_fired_by(spec, &mark.to_string_lossy(), dialect);
    match dialect {
        Dialect::Posix => shown_to_nobody(&command),
        Dialect::PowerShell => command,
    }
}

/// The file-writing tools the cross-project guard decides (0.28.3's names).
const GUARD_MATCHER: &str = "^(edit|write_file|notebook_edit)$";

/// The guard also checks the launch's mark. Its stdout is the decision Qwen Code reads.
fn guard_only_for_own_session_on(
    os: HostOs,
    spec: &LaunchSpec<'_>,
    guard: &GuardHook<'_>,
) -> String {
    let mark = spec.scratch.join(super::MCP_CONFIG_FILE);
    guard_command_fired_by(guard, &mark.to_string_lossy(), hook_dialect(os))
}

fn hook_dialect(os: HostOs) -> Dialect {
    match os {
        HostOs::Macos | HostOs::Linux => Dialect::Posix,
        HostOs::Windows => Dialect::PowerShell,
    }
}

/// Pin the shell on Windows because Qwen Code otherwise chooses it from the launch environment.
fn set_hook_shell(os: HostOs, group: &mut Value) {
    if matches!(hook_dialect(os), Dialect::PowerShell) {
        group["hooks"][0]["shell"] = json!("powershell");
    }
}

/// `command`, for a POSIX shell, with whatever it prints dropped and its failure hidden.
fn shown_to_nobody(command: &str) -> String {
    format!("( {command} ) >/dev/null 2>&1; exit 0")
}

/// The object under `key`, put there in place of anything that is not one.
fn object_entry<'a>(settings: &'a mut Map<String, Value>, key: &str) -> &'a mut Map<String, Value> {
    let entry = settings.entry(key.to_string()).or_insert_with(|| json!({}));
    if !entry.is_object() {
        *entry = json!({});
    }
    entry.as_object_mut().expect("just made an object")
}

/// The private workspace of a Web Shell login launch, beside the session launch files.
pub(super) fn login_workspace(scratch: &Path) -> Result<crate::adapter::account_cli::RunHome> {
    let parent = scratch.parent().context("locating the login workspace")?;
    crate::adapter::account_cli::RunHome::make_private_run_dir(parent, "qwen-login-")
}

/// Writes the launch's settings file into the session's scratch directory and returns its path.
pub(super) fn write(spec: &LaunchSpec<'_>) -> Result<PathBuf> {
    let file = spec.scratch.join(SETTINGS_FILE);
    let settings = settings_json(spec, replaced_settings(spec)?);
    replace_beside(&file, settings.to_string().as_bytes(), Permissions::Private)
        .context("writing the session's settings file")?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::tests::{spec_fixture, SpecFixture};

    /// The events Octoboard's hook is expected to be registered for.
    const REGISTERED: [&str; 8] = [
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PostToolUseFailure",
        "Stop",
        "StopFailure",
        "SubagentStart",
        "SubagentStop",
    ];

    /// A fixture whose shell names `system` as the system settings file, so that no launch reads
    /// the machine's own.
    fn fixture_with_system_file(content: Option<&str>) -> (SpecFixture, PathBuf) {
        let mut fixture = spec_fixture();
        let system = fixture.scratch.join("system-settings.json");
        if let Some(content) = content {
            std::fs::write(&system, content).expect("system settings");
        }
        fixture.shell_env.insert(
            SYSTEM_SETTINGS_VAR.to_string(),
            system.to_string_lossy().into_owned(),
        );
        fixture.shell_env.insert(
            SYSTEM_DEFAULTS_VAR.to_string(),
            fixture
                .scratch
                .join("system-defaults.json")
                .to_string_lossy()
                .into_owned(),
        );
        (fixture, system)
    }

    fn written(fixture: &SpecFixture) -> Value {
        let file = write(&fixture.spec()).expect("settings file");
        serde_json::from_str(&std::fs::read_to_string(file).expect("read back")).expect("JSON")
    }

    /// The commands of the hook groups registered for `event`.
    #[test]
    fn claude_instructions_are_added_only_to_the_launch_that_enables_them() {
        use crate::providers::{ProviderSettings, SettingValue};

        let (mut fixture, system) =
            fixture_with_system_file(Some(r#"{"context":{"fileName":"Guide.md"}}"#));
        assert_eq!(written(&fixture)["context"]["fileName"], "Guide.md");
        fixture.settings = ProviderSettings::new([(
            qwen::CLAUDE_INSTRUCTIONS.to_owned(),
            Some(SettingValue::Switch(true)),
        )]);
        assert_eq!(
            written(&fixture)["context"]["fileName"],
            json!(["Guide.md", "CLAUDE.md"])
        );
        assert_eq!(
            std::fs::read_to_string(system).expect("original system file"),
            r#"{"context":{"fileName":"Guide.md"}}"#
        );
    }

    fn commands(settings: &Value, event: &str) -> Vec<String> {
        settings["hooks"][event]
            .as_array()
            .unwrap_or_else(|| panic!("{event} has a list"))
            .iter()
            .flat_map(|group| group["hooks"].as_array().expect("a group's hooks").iter())
            .map(|hook| hook["command"].as_str().expect("a command").to_string())
            .collect()
    }

    #[test]
    fn the_file_is_written_in_the_scratch_directory() {
        let (fixture, _) = fixture_with_system_file(None);
        let file = write(&fixture.spec()).expect("settings file");
        assert_eq!(file, fixture.scratch.join("qwen-settings.json"));
        assert!(file.is_file());
    }

    /// With no system file to carry over, the file holds only Octoboard's hook for each event it
    /// reads, as a command group Qwen Code runs for every matcher, and nothing for the events
    /// whose waits the terminal title shows nor for `SessionStart`.
    #[test]
    fn hooks_are_registered_for_the_events_that_are_read_and_for_no_other() {
        let (fixture, _) = fixture_with_system_file(None);
        let settings = written(&fixture);
        let hooks = settings["hooks"].as_object().expect("hooks");
        let mut names: Vec<&str> = hooks.keys().map(String::as_str).collect();
        names.sort_unstable();
        let mut expected = REGISTERED.to_vec();
        expected.sort_unstable();
        assert_eq!(names, expected);

        for event in REGISTERED {
            let groups = hooks[event].as_array().expect("a list of groups");
            assert_eq!(groups.len(), 1, "{event}");
            assert_eq!(groups[0]["matcher"], "", "{event}");
            let hook = &groups[0]["hooks"][0];
            assert_eq!(hook["type"], "command", "{event}");
            assert_eq!(hook["timeout"], json!(COMMAND_HOOK_TIMEOUT_SECS), "{event}");
            // What tells the hook, and the file it is in, as Octoboard's.
            assert_eq!(
                hook["env"],
                json!({ "OCTOBOARD_SESSION_HOOK": "1" }),
                "{event}"
            );
            let command = hook["command"].as_str().expect("a command");
            // The command names the session and the daemon it reports to.
            for part in ["hook", "session-1", "4321"] {
                assert!(command.contains(part), "{event}: {command}");
            }
        }
    }

    #[test]
    fn the_interface_settings_the_screen_reading_needs_are_pinned() {
        let (fixture, _) = fixture_with_system_file(None);
        let settings = written(&fixture);
        assert_eq!(
            settings["ui"],
            json!({
                "hideWindowTitle": false,
                "showStatusInTitle": true,
                "enableFollowupSuggestions": false,
                "enableUserFeedback": false,
                "showScrollbar": false,
            })
        );
    }

    /// The system file it stands in for is carried over: its settings stay, the hooks it has are
    /// kept with Octoboard's added after them, and the interface settings are set whatever it
    /// held. The file itself is not touched.
    #[test]
    fn an_existing_system_file_is_carried_over_with_its_hook_lists_appended_to() {
        let original = json!({
            "model": { "name": "stub-model" },
            "hooks": {
                "Stop": [{ "matcher": "", "hooks": [{ "type": "command", "command": "their-stop" }] }],
                "SessionStart": [{ "matcher": "", "hooks": [{ "type": "command", "command": "their-start" }] }],
            },
            "ui": { "theme": "dark", "hideWindowTitle": true, "showStatusInTitle": false, "showScrollbar": true },
        })
        .to_string();
        let (fixture, system) = fixture_with_system_file(Some(&original));
        let settings = written(&fixture);

        assert_eq!(settings["model"], json!({ "name": "stub-model" }));
        assert_eq!(settings["ui"]["theme"], "dark");
        assert_eq!(settings["ui"]["hideWindowTitle"], false);
        assert_eq!(settings["ui"]["showStatusInTitle"], true);
        assert_eq!(settings["ui"]["enableFollowupSuggestions"], false);
        assert_eq!(settings["ui"]["enableUserFeedback"], false);
        assert_eq!(settings["ui"]["showScrollbar"], false);

        let stop = commands(&settings, "Stop");
        assert_eq!(stop.len(), 2);
        assert_eq!(stop[0], "their-stop");
        assert_ne!(stop[1], "their-stop");
        for event in REGISTERED.into_iter().filter(|event| *event != "Stop") {
            assert_eq!(commands(&settings, event).len(), 1, "{event}");
        }
        // An event Octoboard does not register keeps exactly what the file had.
        assert_eq!(commands(&settings, "SessionStart"), ["their-start"]);

        assert_eq!(
            std::fs::read_to_string(system).expect("system file"),
            original
        );
    }

    /// Launching again starts from the system file and not from the file the last launch wrote, so
    /// the hook is never registered twice.
    #[test]
    fn writing_twice_does_not_register_the_hook_twice() {
        let original = json!({ "hooks": { "Stop": [{ "matcher": "", "hooks": [{ "type": "command", "command": "their-stop" }] }] } });
        let (fixture, _) = fixture_with_system_file(Some(&original.to_string()));
        written(&fixture);
        let settings = written(&fixture);
        assert_eq!(commands(&settings, "Stop").len(), 2);
        assert_eq!(commands(&settings, "UserPromptSubmit").len(), 1);
    }

    /// A system file with comments or a byte order mark is carried over as Qwen Code reads it.
    #[test]
    fn a_system_file_with_comments_or_a_byte_order_mark_is_carried_over() {
        let text = "\u{FEFF}{\n  // set by the administrator\n  \"model\": { \"name\": \"kept\" }, /* locked */\n  \"security\": { \"note\": \"// not a comment\" }\n}\n";
        let (fixture, _) = fixture_with_system_file(Some(text));
        let settings = written(&fixture);
        assert_eq!(settings["model"], json!({ "name": "kept" }));
        assert_eq!(settings["security"], json!({ "note": "// not a comment" }));
        assert_eq!(commands(&settings, "Stop").len(), 1);
    }

    /// A system file that is there and cannot be used (it cannot be read, is not JSON once its
    /// comments are off, or is not an object) is an error with the provider's own code and the
    /// file's path, and writes no settings file: a launch file without that layer would drop what an
    /// administrator set in it.
    #[test]
    fn a_system_file_that_cannot_be_used_is_an_error_naming_it() {
        for content in ["{ not json", "[1, 2]", "\"text\"", "", "{\"a\": 1,}"] {
            let (fixture, system) = fixture_with_system_file(Some(content));
            let err = write(&fixture.spec()).expect_err(content);
            let coded = err.downcast_ref::<CodedError>().expect("a coded error");
            assert_eq!(
                coded.code,
                Code::of_provider(qwen::SYSTEM_SETTINGS_UNUSABLE),
                "{content:?}"
            );
            assert_eq!(
                coded.params.get("path").map(String::as_str),
                Some(system.to_string_lossy().as_ref()),
                "{content:?}"
            );
            assert!(
                coded.message.contains(system.to_string_lossy().as_ref()),
                "{content:?}: {}",
                coded.message
            );
            assert!(
                !fixture.scratch.join("qwen-settings.json").exists(),
                "{content:?}"
            );
        }
    }

    /// A system settings file path that is a directory is there, and cannot be read as settings.
    #[test]
    fn a_system_file_that_cannot_be_read_is_an_error_naming_it() {
        let (fixture, system) = fixture_with_system_file(None);
        std::fs::create_dir(&system).expect("a directory in its place");
        let err = write(&fixture.spec()).expect_err("unusable");
        assert_eq!(
            err.downcast_ref::<CodedError>().expect("coded").code,
            Code::of_provider(qwen::SYSTEM_SETTINGS_UNUSABLE)
        );
    }

    /// No system file is an empty layer, not a refusal.
    #[test]
    fn a_missing_system_file_is_an_empty_layer() {
        let (fixture, system) = fixture_with_system_file(None);
        assert!(!system.exists());
        let settings = written(&fixture);
        assert_eq!(commands(&settings, "Stop").len(), 1);
        assert!(settings.get("model").is_none());
    }

    /// A Qwen Code started inside the session loads this settings file too, whatever starts it:
    /// its hooks must not report to the session. Every command carries the launch's MCP
    /// configuration file as the mark of the process that may fire it, reads no process itself,
    /// never tests `QWEN_CODE`, which only the shell tool sets, and drops its output and exits 0,
    /// since Qwen Code shows the user what a failing hook prints.
    #[test]
    fn every_hook_command_posts_only_for_the_launchs_own_qwen_code_and_shows_nothing() {
        let (fixture, _) = fixture_with_system_file(None);
        let settings = written(&fixture);
        let mark = fixture.scratch.join("qwen-mcp.json");
        for event in REGISTERED {
            for command in commands(&settings, event) {
                assert!(!command.contains("QWEN_CODE"), "{event}: {command}");
                for read in ["PPID", "ps -", "/proc/"] {
                    assert!(!command.contains(read), "{event} reads {read}: {command}");
                }
                assert!(
                    command.contains("session-1"),
                    "{event} still reports the session: {command}"
                );
                let marked = command
                    .split_once("--fired-by ")
                    .is_some_and(|(_, mark_on)| mark_on.contains(&*mark.to_string_lossy()));
                assert!(marked, "{event} names the launch's own file: {command}");
                if cfg!(windows) {
                    assert!(command.starts_with("& "), "{event}: {command}");
                    assert!(command.ends_with(" *> $null; exit 0"), "{event}: {command}");
                    assert_eq!(
                        settings["hooks"][event][0]["hooks"][0]["shell"],
                        "powershell"
                    );
                } else {
                    assert!(command.starts_with("( "), "{event}: {command}");
                    assert!(
                        command.ends_with(" ) >/dev/null 2>&1; exit 0"),
                        "{event}: {command}"
                    );
                }
            }
        }
    }

    /// The wrapper run in a POSIX shell around a hook that prints on both streams and fails: the
    /// hook runs, and the command exits 0 and prints nothing.
    #[cfg(unix)]
    #[test]
    fn a_hook_that_prints_and_fails_shows_nothing_and_exits_0() {
        let dir = crate::test_support::ScratchDir::new("qwen-hook-quiet");
        let ran = dir.join("ran");
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(shown_to_nobody(
                "touch \"$RAN\"; echo out; echo err >&2; exit 3",
            ))
            .env("RAN", &ran)
            .output()
            .expect("sh runs");
        assert!(ran.exists());
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stdout.is_empty(), "{:?}", output.stdout);
        assert!(output.stderr.is_empty(), "{:?}", output.stderr);
    }

    /// What the mark rests on, asked of the Qwen Code installed here: the process that runs a
    /// hook's shell is the Qwen Code that fired it, with the launch's arguments on its command
    /// line and nothing of the hook command's own there. A Qwen Code that ran its hooks through a
    /// process of its own in between would leave every hook of every session unposted, with no
    /// message. A headless run in a scratch home, its model endpoint one nothing listens on: the
    /// prompt's hook fires before the request is made.
    #[cfg(unix)]
    #[test]
    #[ignore = "needs Qwen Code on PATH; run with --ignored where it is installed, after each upgrade of it"]
    fn the_installed_qwen_code_runs_a_hook_as_a_child_of_the_process_the_launch_started() {
        let dir = crate::test_support::ScratchDir::new("qwen-hook-parent");
        let home = dir.join("home");
        let project = dir.join("project");
        let scratch = dir.join("run").join("session-1");
        for made in [&home.join(".qwen"), &project, &scratch] {
            std::fs::create_dir_all(made).expect("directory");
        }
        let endpoint = "http://127.0.0.1:9/v1";
        std::fs::write(
            home.join(".qwen").join("settings.json"),
            json!({
                "security": { "auth": { "selectedType": "openai" } },
                "env": { "STUB_KEY": "sk-fake" },
                "modelProviders": { "openai": [
                    { "id": "stub", "name": "stub", "baseUrl": endpoint, "envKey": "STUB_KEY" }
                ] },
                "model": { "name": "stub", "baseUrl": endpoint },
                "general": { "enableAutoUpdate": false },
            })
            .to_string(),
        )
        .expect("the account's settings");
        let mark = scratch.join(super::super::MCP_CONFIG_FILE);
        std::fs::write(&mark, r#"{"mcpServers":{}}"#).expect("the MCP configuration");
        // The hook writes down the command line of the process above its shell, wrapped as the
        // launch's own hook is.
        let seen = scratch.join("seen");
        let hook = shown_to_nobody("ps -ww -o command= -p \"$PPID\" > \"$SEEN\"");
        let settings = scratch.join(SETTINGS_FILE);
        std::fs::write(
            &settings,
            json!({ "hooks": { "UserPromptSubmit": [{ "matcher": "", "hooks": [{
                "type": "command",
                "command": hook,
                "timeout": 5,
                "env": { "SEEN": seen.to_string_lossy() },
            }] }] } })
            .to_string(),
        )
        .expect("the launch's settings");

        let status = std::process::Command::new("qwen")
            .arg("--mcp-config")
            .arg(&mark)
            .args(["-p", "hello"])
            .current_dir(&project)
            .env("HOME", &home)
            .env_remove("QWEN_HOME")
            .env_remove("QWEN_CODE")
            .env(SYSTEM_SETTINGS_VAR, &settings)
            .env(SYSTEM_DEFAULTS_VAR, scratch.join(SYSTEM_DEFAULTS_FILE))
            .env(super::super::NO_RELAUNCH_VAR, "1")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("qwen runs");
        // The request fails, the endpoint being nobody's: only that the run ended matters.
        let _ = status;
        let above = std::fs::read_to_string(&seen).expect("the hook ran and wrote what it saw");
        assert!(
            above.contains(&*mark.to_string_lossy()),
            "a hook's parent is no longer the Qwen Code the launch started: {above}"
        );
    }

    /// The system settings file a session would read: the one the environment names, else the
    /// platform's.
    #[test]
    fn the_system_settings_path_is_the_environments_else_the_platforms() {
        let named = HashMap::from([(
            SYSTEM_SETTINGS_VAR.to_string(),
            "/admin/qwen/settings.json".to_string(),
        )]);
        assert_eq!(
            system_settings_path(&named),
            PathBuf::from("/admin/qwen/settings.json")
        );
        for env in [
            HashMap::new(),
            HashMap::from([(SYSTEM_SETTINGS_VAR.to_string(), String::new())]),
        ] {
            let platform = system_settings_path(&env);
            assert!(platform.is_absolute(), "{platform:?}");
            assert_eq!(platform.file_name().expect("a file"), "settings.json");
            assert_ne!(platform, PathBuf::from("/admin/qwen/settings.json"));
        }
    }

    /// The defaults file: the environment's, else `system-defaults.json` beside the system settings
    /// file the session would read.
    #[test]
    fn the_system_defaults_path_is_the_environments_else_beside_the_settings_file() {
        let env = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(var, value)| (var.to_string(), value.to_string()))
                .collect::<HashMap<_, _>>()
        };
        assert_eq!(
            system_defaults_path(&env(&[(SYSTEM_DEFAULTS_VAR, "/admin/defaults.json")])),
            PathBuf::from("/admin/defaults.json")
        );
        assert_eq!(
            system_defaults_path(&env(&[
                (SYSTEM_SETTINGS_VAR, "/admin/qwen/settings.json"),
                (SYSTEM_DEFAULTS_VAR, "/elsewhere/defaults.json"),
            ])),
            PathBuf::from("/elsewhere/defaults.json")
        );
        assert_eq!(
            system_defaults_path(&env(&[(SYSTEM_SETTINGS_VAR, "/admin/qwen/settings.json")])),
            PathBuf::from("/admin/qwen/system-defaults.json")
        );
        // An empty value names nothing.
        assert_eq!(
            system_defaults_path(&env(&[
                (SYSTEM_SETTINGS_VAR, "/admin/qwen/settings.json"),
                (SYSTEM_DEFAULTS_VAR, ""),
            ])),
            PathBuf::from("/admin/qwen/system-defaults.json")
        );
        // With neither, beside the platform's own file.
        let platform = system_settings_path(&HashMap::new());
        assert_eq!(
            system_defaults_path(&HashMap::new()),
            platform.with_file_name("system-defaults.json")
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let (fixture, _) = fixture_with_system_file(None);
        let file = write(&fixture.spec()).expect("settings file");
        let mode = std::fs::metadata(file)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "{mode:o}");
    }

    /// The settings file a launch of `session` wrote, as a later daemon would find it named by its
    /// shell's variable. The fixture holds its scratch directory, so the caller keeps it, bound to a
    /// name and not to `_`, for as long as the file is read.
    fn launch_file_of(session: &str) -> (SpecFixture, PathBuf) {
        let (mut fixture, _) = fixture_with_system_file(None);
        fixture.session_id = session.to_string();
        let file = write(&fixture.spec()).expect("settings file");
        (fixture, file)
    }

    fn env_naming(file: &Path) -> HashMap<String, String> {
        HashMap::from([(
            SYSTEM_SETTINGS_VAR.to_string(),
            file.to_string_lossy().into_owned(),
        )])
    }

    /// A settings file is a launch's exactly when a hook of any event carries the mark in its own
    /// `env`: the launch's own, however it is kept, and not an administrator's, with its own hooks
    /// and its own hook `env`.
    #[test]
    fn a_readable_file_is_a_launch_file_exactly_when_a_hook_carries_the_mark() {
        let (_kept, written) = launch_file_of("some-session");
        assert!(is_launch_file(&written));

        let dir = crate::test_support::ScratchDir::new("qwen-launch-file");
        let file = |name: &str, content: Value| {
            let path = dir.join(name);
            std::fs::write(&path, content.to_string()).expect("file");
            path
        };
        let hook = |env: Value| json!({ "type": "command", "command": "x", "env": env });
        let group = |hooks: Vec<Value>| json!({ "matcher": "", "hooks": hooks });
        // Marked, of whatever event, and beside hooks that are not.
        assert!(is_launch_file(&file(
            "marked.json",
            json!({ "hooks": { "SessionStart": [
                group(vec![hook(json!({ "A": "1" }))]),
                group(vec![hook(json!({})), hook(json!({ "OCTOBOARD_SESSION_HOOK": "1" }))]),
            ] } })
        )));
        // Whatever the mark's value is, it is there.
        assert!(is_launch_file(&file(
            "marked-empty.json",
            json!({ "hooks": { "Stop": [group(vec![hook(json!({ "OCTOBOARD_SESSION_HOOK": "" }))])] } })
        )));
        // An administrator's file: hooks of its own, with an `env` of its own, and settings.
        assert!(!is_launch_file(&file(
            "admin.json",
            json!({
                "model": { "name": "m" },
                "hooks": { "Stop": [group(vec![hook(json!({ "AUDIT_LEVEL": "2" })), hook(json!(null))])] },
            })
        )));
        // No hooks at all, and hooks of another shape.
        assert!(!is_launch_file(&file(
            "plain.json",
            json!({ "ui": { "theme": "dark" } })
        )));
        assert!(!is_launch_file(&file(
            "no-groups.json",
            json!({ "hooks": { "Stop": "x" } })
        )));
        // The mark in the `env` of a group is no hook's.
        assert!(!is_launch_file(&file(
            "group-env.json",
            json!({ "hooks": { "Stop": [{ "env": { "OCTOBOARD_SESSION_HOOK": "1" }, "hooks": [] }] } })
        )));
    }

    /// A file that is not there cannot be asked, so it is taken for a launch's only where one is
    /// kept: `run/<session>/qwen-settings.json`, the scratch directory of a session that has ended.
    #[test]
    fn a_missing_file_is_a_launch_file_only_where_a_launch_keeps_its_own() {
        let dir = crate::test_support::ScratchDir::new("qwen-launch-missing");
        for (relative, expected) in [
            ("run/an-ended-session/qwen-settings.json", true),
            ("data/run/another/qwen-settings.json", true),
            ("run/an-ended-session/settings.json", false),
            ("run/qwen-settings.json", false),
            ("scratch/an-ended-session/qwen-settings.json", false),
            ("run/a/b/qwen-settings.json", false),
            ("admin/qwen/settings.json", false),
            ("qwen-settings.json", false),
        ] {
            assert_eq!(is_launch_file(&dir.join(relative)), expected, "{relative}");
        }
    }

    /// A file that is there and cannot be used is nobody's launch file, wherever it lies.
    #[test]
    fn an_unusable_file_is_not_a_launch_file() {
        let dir = crate::test_support::ScratchDir::new("qwen-launch-unusable");
        let kept = dir.join("run").join("a-session");
        std::fs::create_dir_all(&kept).expect("directory");
        for (name, content) in [
            ("qwen-settings.json", "{ not json"),
            ("other.json", "[1, 2]"),
        ] {
            let path = kept.join(name);
            std::fs::write(&path, content).expect("file");
            assert!(!is_launch_file(&path), "{name}");
        }
        // A directory in its place.
        let in_place = dir.join("run").join("b-session").join("qwen-settings.json");
        std::fs::create_dir_all(&in_place).expect("directory");
        assert!(!is_launch_file(&in_place));
    }

    /// The system settings file is the variable's unless that is a launch file, in which case it is
    /// the platform's, and the defaults file is then never beside the launch file.
    #[test]
    fn a_launch_file_is_never_taken_for_the_system_layer() {
        let platform = system_settings_path(&HashMap::new());
        let (_kept, launch) = launch_file_of("old-session");
        let named = env_naming(&launch);
        assert_eq!(system_settings_path(&named), platform);
        assert_ne!(system_settings_path(&named), launch);
        let defaults = system_defaults_path(&named);
        assert_eq!(defaults, platform.with_file_name("system-defaults.json"));
        assert_ne!(defaults.parent(), launch.parent());

        // The same where the launch's scratch directory is gone.
        let dir = crate::test_support::ScratchDir::new("qwen-launch-gone");
        let gone = dir
            .join("run")
            .join("old-session")
            .join("qwen-settings.json");
        let named = env_naming(&gone);
        assert_eq!(system_settings_path(&named), platform);
        assert_eq!(
            system_defaults_path(&named),
            platform.with_file_name("system-defaults.json")
        );

        // An administrator's file is the layer.
        let admin = dir.join("admin-settings.json");
        std::fs::write(&admin, "{\"model\": {}}").expect("file");
        let named = env_naming(&admin);
        assert_eq!(system_settings_path(&named), admin);
        assert_eq!(
            system_defaults_path(&named),
            admin.with_file_name("system-defaults.json")
        );
    }

    /// What is carried over has Octoboard's hooks of any session taken out: from every group, a
    /// group dropped only where that emptied it, an event dropped when its list became empty.
    /// Hooks that are not Octoboard's, groups that were empty already and everything else stay.
    #[test]
    fn carrying_a_file_over_takes_octoboards_hooks_out_and_only_those() {
        let own = |id: &str| json!({ "type": "command", "command": id, "env": { "OCTOBOARD_SESSION_HOOK": "1" } });
        let theirs =
            |id: &str| json!({ "type": "command", "command": id, "env": { "AUDIT_LEVEL": "2" } });
        let bare = |id: &str| json!({ "type": "command", "command": id });
        let settings = json!({
            "model": { "name": "kept" },
            "hooks": {
                "Stop": [
                    { "matcher": "a", "hooks": [own("old-1"), theirs("t-1"), bare("t-2")] },
                    { "matcher": "b", "hooks": [own("old-2")] },
                    { "matcher": "c", "hooks": [] },
                    { "matcher": "d", "hooks": [theirs("t-3")] },
                    { "matcher": "e" },
                ],
                "PreToolUse": [{ "matcher": "", "hooks": [own("old-3")] }],
                "SessionStart": [{ "matcher": "", "hooks": [theirs("t-4")] }],
            },
        });
        let Value::Object(map) = settings else {
            unreachable!()
        };
        let carried = Value::Object(without_own_hooks(map));
        assert_eq!(
            carried,
            json!({
                "model": { "name": "kept" },
                "hooks": {
                    "Stop": [
                        { "matcher": "a", "hooks": [theirs("t-1"), bare("t-2")] },
                        { "matcher": "c", "hooks": [] },
                        { "matcher": "d", "hooks": [theirs("t-3")] },
                        { "matcher": "e" },
                    ],
                    "SessionStart": [{ "matcher": "", "hooks": [theirs("t-4")] }],
                },
            })
        );
        // A file with no hooks is left as it is.
        let Value::Object(plain) = json!({ "ui": {} }) else {
            unreachable!()
        };
        assert_eq!(Value::Object(without_own_hooks(plain)), json!({ "ui": {} }));
    }

    /// The system settings path and the hook guard are chosen by the host's operating system, as
    /// the platform layer names it, each of the three its own.
    #[test]
    fn the_system_settings_path_and_the_guard_follow_the_hosts_operating_system() {
        assert_eq!(
            system_settings_on(HostOs::Macos),
            Path::new("/Library/Application Support/QwenCode/settings.json")
        );
        assert_eq!(
            system_settings_on(HostOs::Linux),
            Path::new("/etc/qwen-code/settings.json")
        );
        assert_eq!(
            system_settings_on(HostOs::Windows),
            Path::new(r"C:\ProgramData\qwen-code\settings.json")
        );
        assert_eq!(
            default_system_settings(),
            system_settings_on(HostOs::current())
        );

        let (fixture, _) = fixture_with_system_file(None);
        let spec = fixture.spec();
        let mark = spec.scratch.join("qwen-mcp.json");
        let marked = hook_command_fired_by(&spec, &mark.to_string_lossy(), Dialect::Posix);
        for os in [HostOs::Macos, HostOs::Linux] {
            assert_eq!(
                own_session_only_on(os, &spec),
                format!("( {marked} ) >/dev/null 2>&1; exit 0"),
                "{os:?}"
            );
        }
        let windows = own_session_only_on(HostOs::Windows, &spec);
        assert!(windows.contains(&format!("--fired-by '{}'", mark.display())));
        assert!(windows.ends_with(" *> $null; exit 0"));
    }

    /// A settings file that holds a hook of Octoboard's, carried over the way a launch carries its
    /// system settings file, comes back without it and with the administrator's own hooks and
    /// settings as they were; a file that is not there carries nothing, and one that cannot be
    /// used is an error.
    #[test]
    fn a_file_carried_over_loses_octoboards_hooks_and_keeps_the_administrators() {
        let dir = spec_fixture();
        let file = dir.scratch.join("carried.json");
        let own =
            json!({ "type": "command", "command": "old-session", "env": { OWN_HOOK_MARK: "1" } });
        let theirs = json!({ "type": "command", "command": "audit" });
        let content = json!({
            "permissions": { "deny": ["Bash(rm *)"] },
            "hooks": {
                "Stop": [{ "matcher": "", "hooks": [own.clone(), theirs.clone()] }],
                "PreToolUse": [{ "matcher": "", "hooks": [own] }],
            },
        });
        std::fs::write(&file, content.to_string()).expect("the file");
        assert_eq!(
            Value::Object(carried_settings(&file).expect("carried")),
            json!({
                "permissions": { "deny": ["Bash(rm *)"] },
                "hooks": { "Stop": [{ "matcher": "", "hooks": [theirs] }] },
            })
        );

        let missing = dir.scratch.join("no-such.json");
        assert!(carried_settings(&missing).expect("nothing").is_empty());

        std::fs::write(&file, "{ not json").expect("the file");
        let err = carried_settings(&file).expect_err("unusable");
        assert_eq!(
            err.downcast_ref::<CodedError>().expect("coded").code,
            Code::of_provider(qwen::SYSTEM_SETTINGS_UNUSABLE)
        );
    }

    /// A daemon whose shell names an old session's launch file (started from that session's shell
    /// tool) plans a new session: the new file holds one hook of Octoboard's per event, the new
    /// session's, none of the old one's, and its defaults file is not looked for in the old
    /// session's directory.
    ///
    /// The platform's own file stands behind a launch file then, which a test cannot name, so this
    /// follows the launch file as far as `system_settings_path` goes and, from there, carries the
    /// launch file itself over as a launch carries any file ([`carried_settings`]): reading the
    /// file the platform keeps (`/Library/Application Support/QwenCode`, `/etc/qwen-code`) would
    /// make the outcome the machine's.
    #[test]
    fn a_new_session_carries_none_of_an_old_sessions_hooks() {
        let (_kept, old_file) = launch_file_of("old-session");
        let old = settings_file::read(&old_file, SYSTEM_FILE_LIMIT)
            .settings()
            .expect("the old launch file");
        assert_eq!(commands(&old, "Stop").len(), 1);

        let (mut fixture, _) = fixture_with_system_file(None);
        fixture.session_id = "new-session".to_string();
        fixture.shell_env.insert(
            SYSTEM_SETTINGS_VAR.to_string(),
            old_file.to_string_lossy().into_owned(),
        );
        fixture.shell_env.remove(SYSTEM_DEFAULTS_VAR);
        let spec = fixture.spec();
        assert_eq!(
            system_settings_path(spec.shell_env),
            system_settings_path(&HashMap::new()),
            "the launch file is not the layer"
        );
        assert_ne!(
            system_defaults_path(spec.shell_env).parent(),
            old_file.parent(),
            "nor is its directory where the defaults are looked for"
        );

        let carried = carried_settings(&old_file).expect("the old launch file carried over");
        let new = settings_json(&spec, carried);
        for event in REGISTERED {
            let commands = commands(&new, event);
            assert_eq!(commands.len(), 1, "{event}");
            assert!(
                commands[0].contains("new-session"),
                "{event}: {}",
                commands[0]
            );
            assert!(
                !commands[0].contains("old-session"),
                "{event}: {}",
                commands[0]
            );
        }
    }

    /// A launch with the cross-project guard has a second `PreToolUse` group, for the
    /// file-writing tools alone, with the guard's timeout and Octoboard's mark; one without has
    /// only the status hook's.
    #[test]
    fn the_cross_project_guard_is_a_pre_tool_use_group_for_the_file_writing_tools() {
        let (fixture, _) = fixture_with_system_file(None);
        let settings = written(&fixture);
        assert_eq!(settings["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);

        let list = fixture.scratch.join("cross-project-guard.json");
        let script = fixture.scratch.join("guard-hook");
        let mut spec = fixture.spec();
        spec.guard = Some(GuardHook {
            exe: "/opt/octoboard/bin/octoboard",
            list: &list,
            script: &script,
        });
        let file = write(&spec).expect("settings file");
        let settings: Value =
            serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
        let groups = settings["hooks"]["PreToolUse"].as_array().unwrap();
        let [status, guard] = groups.as_slice() else {
            panic!("the status group and the guard's: {groups:?}");
        };
        assert_eq!(status["matcher"], "");
        assert_eq!(guard["matcher"], GUARD_MATCHER);
        let hook = &guard["hooks"][0];
        assert_eq!(hook["type"], "command");
        assert_eq!(hook["timeout"], json!(GUARD_HOOK_TIMEOUT_SECS));
        assert_eq!(hook["env"], json!({ "OCTOBOARD_SESSION_HOOK": "1" }));
        assert_eq!(
            hook["command"].as_str(),
            Some(
                guard_only_for_own_session_on(
                    HostOs::current(),
                    &spec,
                    spec.guard.as_ref().unwrap()
                )
                .as_str()
            )
        );
        // Only the guard's group is added: the other events keep their one group.
        assert_eq!(settings["hooks"]["Stop"].as_array().unwrap().len(), 1);
    }

    /// The matcher, a regular expression Qwen Code tests the tool's name against, names exactly
    /// its three file-writing tools, anchored so that no longer name matches.
    #[test]
    fn the_guard_matches_the_file_writing_tools_and_no_other() {
        assert_eq!(GUARD_MATCHER, "^(edit|write_file|notebook_edit)$");
    }

    /// The guard checks for the launch's MCP configuration path on the caller's command line.
    #[test]
    fn the_guard_decides_only_for_the_launchs_own_qwen_code() {
        let (fixture, _) = fixture_with_system_file(None);
        let list = fixture.scratch.join("cross-project-guard.json");
        let script = fixture.scratch.join("guard-hook");
        let guard = GuardHook {
            exe: "/opt/octoboard/bin/octoboard",
            list: &list,
            script: &script,
        };
        let spec = fixture.spec();
        let mark = spec.scratch.join("qwen-mcp.json");
        for os in [HostOs::Macos, HostOs::Linux] {
            let command = guard_only_for_own_session_on(os, &spec, &guard);
            assert_eq!(
                command,
                guard_command_fired_by(&guard, &mark.to_string_lossy(), Dialect::Posix),
                "{os:?}"
            );
            for read in ["PPID", "ps -", "/proc/"] {
                assert!(!command.contains(read), "{os:?} reads {read}: {command}");
            }
        }
        let windows = guard_only_for_own_session_on(HostOs::Windows, &spec, &guard);
        assert!(windows.contains(&format!("--fired-by '{}'", mark.display())));
        assert!(windows.ends_with(" 2> $null; exit 0"));
    }

    /// While a launch turns Qwen Code's own memory off, its settings switch the managed memory and
    /// its consolidation off; otherwise they say nothing of memory, and the user's own settings
    /// decide.
    #[test]
    fn the_launch_switches_qwens_own_memory_off_only_when_told_to() {
        let (fixture, _) = fixture_with_system_file(None);
        assert!(written(&fixture).get("memory").is_none());

        let mut spec = fixture.spec();
        spec.native_memory_off = true;
        let file = write(&spec).expect("settings file");
        let settings: Value =
            serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
        assert_eq!(
            settings["memory"],
            json!({ "enableManagedAutoMemory": false, "enableManagedAutoDream": false })
        );
    }

    /// The switch keeps the rest of a memory section the system file it stands in for holds.
    #[test]
    fn switching_memory_off_keeps_the_system_files_other_memory_settings() {
        let (fixture, _) = fixture_with_system_file(Some(r#"{"memory":{"enableAutoSkill":true}}"#));
        let mut spec = fixture.spec();
        spec.native_memory_off = true;
        let file = write(&spec).expect("settings file");
        let settings: Value =
            serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
        assert_eq!(
            settings["memory"],
            json!({
                "enableAutoSkill": true,
                "enableManagedAutoMemory": false,
                "enableManagedAutoDream": false,
            })
        );
    }
}
