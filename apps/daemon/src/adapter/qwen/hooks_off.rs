//! Whether Qwen Code will run no hooks in a launch, and so report nothing to Octoboard through
//! them. Measured on 0.28.3 (`docs/agent-cli-reference.md`, section "Qwen Code"): each of the three
//! switches below leaves every hook unrun, the launch's own among them, while the OSC 7501 reports
//! go on as ever. The launch never turns hooks back on that the user turned off; it tells the user
//! instead ([`notice`]).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::settings::{system_defaults_path, system_settings_path, SYSTEM_FILE_LIMIT};
use super::settings_file::{self, SettingsFile};
use crate::adapter::files::{canonical_project, is_agent_config_dir, LARGE_FILE_LIMIT};
use crate::adapter::{account_dir, LaunchSpec};
use crate::protocol::{AgentId, Code, Notice};
use crate::providers::qwen::{HOOKS_OFF, HOOKS_OFF_BY_SETTING, QWEN_SETTINGS_FILE};

/// The setting that switches every hook off, in any settings layer.
const DISABLE_ALL_HOOKS: &str = "disableAllHooks";

/// The variable that starts Qwen Code in safe mode, as `--safe-mode` does.
const SAFE_MODE_VAR: &str = "QWEN_CODE_SAFE_MODE";

/// The variable that starts Qwen Code in bare mode, as `--bare` does: it then loads no settings
/// file at all, the launch's included. Past the hooks nothing Octoboard reads is lost with them
/// (0.28.3): the title keeps its glyph and no next prompt is suggested, both being as Qwen Code
/// has them with no settings; the feedback question can come up, and takes nothing of a message
/// written as a paste.
const BARE_MODE_VAR: &str = "QWEN_CODE_SIMPLE";

/// The directory a project keeps its Qwen Code settings in.
const PROJECT_DIR: &str = ".qwen";

/// What switches the hooks of a launch off.
#[derive(Debug, PartialEq)]
pub(super) enum Switch {
    /// A variable of the environment the session starts in, by its name.
    Variable(&'static str),
    /// `disableAllHooks`, in the settings file whose value decides.
    Setting(PathBuf),
}

/// Whether Qwen Code reads `value`, a variable's, as switched on: `1`, `true`, `yes` or `on`,
/// whatever the case and the space around it.
fn is_truthy(value: &str) -> bool {
    ["1", "true", "yes", "on"].contains(&value.trim().to_ascii_lowercase().as_str())
}

/// Whether JavaScript takes `value` for true, as Qwen Code takes the setting once the layers are
/// merged (0.28.3): the string `"no"` switched the hooks off, and `0` and `null` did not, each
/// over a lower layer that said otherwise.
fn js_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(on) => *on,
        Value::Number(number) => number.as_f64().is_some_and(|number| number.abs() > 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// What switches the hooks of a session started as `spec` off; `None` where nothing does, or
/// where it cannot be told.
///
/// - Bare mode and safe mode, by the variables of the environment the session starts in. The
///   launch passes neither flag.
/// - `disableAllHooks` as the settings layers leave it ([`disabled_in_settings`]).
pub(super) fn switched_off_by(spec: &LaunchSpec<'_>) -> Option<Switch> {
    let on = |variable: &str| spec.shell_env.get(variable).is_some_and(|v| is_truthy(v));
    if on(BARE_MODE_VAR) {
        return Some(Switch::Variable(BARE_MODE_VAR));
    }
    if on(SAFE_MODE_VAR) {
        return Some(Switch::Variable(SAFE_MODE_VAR));
    }
    disabled_in_settings(spec.cwd, spec.config_dir, spec.shell_env).map(Switch::Setting)
}

/// The settings file that leaves `disableAllHooks` on for a session in `cwd`, on the account in
/// `config_dir`: the highest layer that holds the setting at all decides, whatever the kind of
/// its value (the system settings file, the project's, the account's, the system defaults), and
/// it is that file when its value is on. A layer that is missing or cannot be read holds nothing.
fn disabled_in_settings(
    cwd: &Path,
    config_dir: Option<&Path>,
    shell_env: &HashMap<String, String>,
) -> Option<PathBuf> {
    let account_dir = account_dir(AgentId::QWEN, config_dir, shell_env);
    let project = canonical_project(cwd);
    let project_dir = project.join(PROJECT_DIR);
    // A home directory opened as the project holds the account's own directory there, whose file
    // is the user's wherever it leads; any other is the project's to keep inside itself.
    let kept_by_project = !is_agent_config_dir(&project_dir, &account_dir);
    // From the layer that wins down; the two system files within the size the launch reads them in.
    let layers = [
        (system_settings_path(shell_env), SYSTEM_FILE_LIMIT, false),
        (
            project_dir.join(QWEN_SETTINGS_FILE),
            LARGE_FILE_LIMIT,
            kept_by_project,
        ),
        (
            account_dir.join(QWEN_SETTINGS_FILE),
            LARGE_FILE_LIMIT,
            false,
        ),
        (system_defaults_path(shell_env), SYSTEM_FILE_LIMIT, false),
    ];
    for (file, limit, in_project) in layers {
        let layer = if in_project {
            settings_file::read_in_project(&file, &project, limit)
        } else {
            settings_file::read(&file, limit)
        };
        let SettingsFile::Settings(layer) = layer else {
            continue;
        };
        if let Some(value) = layer.get(DISABLE_ALL_HOOKS) {
            return js_truthy(value).then_some(file);
        }
    }
    None
}

/// Whether `notice` is the one a launch found with its hooks switched off is given ([`notice`]).
pub(super) fn is_notice(notice: &Notice) -> bool {
    notice.code == Code::of_provider(HOOKS_OFF)
        || notice.code == Code::of_provider(HOOKS_OFF_BY_SETTING)
}

/// What the user is told of a session whose hooks `switch` turned off: its status is read from
/// Qwen Code's reports instead (`AgentProvider::launch_has_status_integration`); the cross-project
/// guard, a `PreToolUse` hook, does not run; its prompts are not answered from the window, whose
/// reader matches a prompt to the call a `PreToolUse` reported (`super::interaction`); only the
/// user can turn the hooks back on, and where: the variable by its name, the setting by its name
/// and the file that holds it.
pub(super) fn notice(switch: &Switch) -> Notice {
    let consequence = "so this session's status follows what its terminal reports. Edits to \
                       another project's files are not blocked, and its prompts can be answered \
                       only in its terminal";
    let notice = match switch {
        Switch::Variable(variable) => Notice::new(
            Code::of_provider(HOOKS_OFF),
            format!(
                "Qwen Code hooks are turned off by `{variable}`, {consequence}. Unset that \
                 variable in the environment where sessions run, then launch the session again."
            ),
            &[("switch", variable)],
        ),
        Switch::Setting(file) => {
            let path = file.to_string_lossy();
            Notice::new(
                Code::of_provider(HOOKS_OFF_BY_SETTING),
                format!(
                    "Qwen Code hooks are turned off by `{DISABLE_ALL_HOOKS}` in `{path}`, \
                     {consequence}. Remove that setting from the file, then launch the session \
                     again."
                ),
                &[("switch", DISABLE_ALL_HOOKS), ("path", &path)],
            )
        }
    };
    // Standing: the hooks stay off for the whole launch.
    notice.standing()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::tests::{spec_fixture, SpecFixture};

    /// A launch fixture whose system layers and account are in its scratch directory, so that
    /// nothing of the machine's own is read.
    fn isolated() -> SpecFixture {
        let mut fixture = spec_fixture();
        for (variable, name) in [
            (super::super::settings::SYSTEM_SETTINGS_VAR, "system.json"),
            (super::super::settings::SYSTEM_DEFAULTS_VAR, "defaults.json"),
        ] {
            fixture.shell_env.insert(
                variable.to_string(),
                fixture.scratch.join(name).to_string_lossy().into_owned(),
            );
        }
        let account = fixture.scratch.join("account");
        std::fs::create_dir_all(&account).expect("account");
        fixture.config_dir = Some(account);
        fixture
    }

    fn write(path: &Path, content: &str) {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("directory");
        std::fs::write(path, content).expect("file");
    }

    /// The four layers of `fixture`, lowest first: defaults, account, project, system.
    fn layers(fixture: &SpecFixture) -> [std::path::PathBuf; 4] {
        [
            fixture.scratch.join("defaults.json"),
            fixture.scratch.join("account").join("settings.json"),
            fixture.cwd.join(".qwen").join("settings.json"),
            fixture.scratch.join("system.json"),
        ]
    }

    /// The setting found on in `layer` of `fixture`, as [`switched_off_by`] names it: the project's
    /// file by the project's canonical path.
    fn setting_in(fixture: &SpecFixture, layer: usize) -> Option<Switch> {
        let file = match layer {
            2 => canonical_project(&fixture.cwd)
                .join(".qwen")
                .join("settings.json"),
            layer => layers(fixture)[layer].clone(),
        };
        Some(Switch::Setting(file))
    }

    #[test]
    fn nothing_set_leaves_the_hooks_on() {
        let fixture = isolated();
        assert_eq!(switched_off_by(&fixture.spec()), None);
    }

    /// `disableAllHooks` in any one layer switches them off, read as Qwen Code reads the file,
    /// and the layer's file is named.
    #[test]
    fn the_setting_in_any_layer_switches_them_off() {
        for layer in 0..4 {
            let fixture = isolated();
            write(
                &layers(&fixture)[layer],
                "{\n // by hand\n \"disableAllHooks\": true\n}",
            );
            assert_eq!(
                switched_off_by(&fixture.spec()),
                setting_in(&fixture, layer),
                "layer {layer}"
            );
        }
    }

    /// The highest layer that sets it decides, on or off, and a layer that does not set it, is not
    /// there or cannot be read decides nothing.
    #[test]
    fn the_highest_layer_that_sets_it_decides() {
        for higher in 1..4 {
            for lower in 0..higher {
                let fixture = isolated();
                write(&layers(&fixture)[lower], r#"{"disableAllHooks": true}"#);
                write(&layers(&fixture)[higher], r#"{"disableAllHooks": false}"#);
                assert_eq!(switched_off_by(&fixture.spec()), None, "{lower} {higher}");

                let fixture = isolated();
                write(&layers(&fixture)[lower], r#"{"disableAllHooks": false}"#);
                write(&layers(&fixture)[higher], r#"{"disableAllHooks": true}"#);
                assert_eq!(
                    switched_off_by(&fixture.spec()),
                    setting_in(&fixture, higher),
                    "{lower} {higher}"
                );
            }
        }
        for unsaid in [r#"{"ui": {}}"#, "{ not json"] {
            let fixture = isolated();
            write(&layers(&fixture)[1], r#"{"disableAllHooks": true}"#);
            write(&layers(&fixture)[2], unsaid);
            assert_eq!(
                switched_off_by(&fixture.spec()),
                setting_in(&fixture, 1),
                "{unsaid}"
            );
        }
    }

    /// A value that is no boolean decides as JavaScript takes it, in the layer that holds it and
    /// over a lower layer that says otherwise: a string that is not empty is on, whatever it
    /// says, and `0`, `null` and the empty string are off.
    #[test]
    fn a_value_that_is_no_boolean_decides_as_javascript_takes_it() {
        for on in [r#""no""#, r#""false""#, "1", "[]", "{}"] {
            let fixture = isolated();
            write(&layers(&fixture)[1], r#"{"disableAllHooks": false}"#);
            write(
                &layers(&fixture)[3],
                &format!(r#"{{"disableAllHooks": {on}}}"#),
            );
            assert_eq!(
                switched_off_by(&fixture.spec()),
                setting_in(&fixture, 3),
                "{on}"
            );
        }
        for off in ["0", "null", "\"\"", "0.0"] {
            let fixture = isolated();
            write(&layers(&fixture)[1], r#"{"disableAllHooks": true}"#);
            write(
                &layers(&fixture)[3],
                &format!(r#"{{"disableAllHooks": {off}}}"#),
            );
            assert_eq!(switched_off_by(&fixture.spec()), None, "{off}");
        }
    }

    /// The project's settings file is the project's to keep inside itself: one that is a link out
    /// of the project is not followed, and decides nothing.
    #[cfg(unix)]
    #[test]
    fn a_project_settings_file_that_links_out_of_the_project_is_not_read() {
        let fixture = isolated();
        let outside = fixture.scratch.join("outside.json");
        write(&outside, r#"{"disableAllHooks": true}"#);
        let project_file = &layers(&fixture)[2];
        std::fs::create_dir_all(project_file.parent().expect("a parent")).expect("directory");
        std::os::unix::fs::symlink(&outside, project_file).expect("a link");
        assert_eq!(switched_off_by(&fixture.spec()), None);

        // The same file as the project's own is read.
        std::fs::remove_file(project_file).expect("the link");
        std::fs::copy(&outside, project_file).expect("a copy");
        assert_eq!(switched_off_by(&fixture.spec()), setting_in(&fixture, 2));
    }

    /// Safe mode and bare mode come from the environment the session starts in, by the values Qwen
    /// Code takes as on; bare mode is named first, since it also loads no settings.
    #[test]
    fn safe_mode_and_bare_mode_switch_them_off_by_their_variables() {
        for (variable, others) in [
            ("QWEN_CODE_SAFE_MODE", &[][..]),
            ("QWEN_CODE_SIMPLE", &["QWEN_CODE_SAFE_MODE"][..]),
        ] {
            for value in ["1", "true", "TRUE", " yes ", "on"] {
                let mut fixture = isolated();
                fixture
                    .shell_env
                    .insert(variable.to_string(), value.to_string());
                for other in others {
                    fixture
                        .shell_env
                        .insert((*other).to_string(), "1".to_string());
                }
                assert_eq!(
                    switched_off_by(&fixture.spec()),
                    Some(Switch::Variable(variable)),
                    "{value:?}"
                );
            }
            for value in ["", "0", "false", "no", "off", "2"] {
                let mut fixture = isolated();
                fixture
                    .shell_env
                    .insert(variable.to_string(), value.to_string());
                assert_eq!(
                    switched_off_by(&fixture.spec()),
                    None,
                    "{variable}={value:?}"
                );
            }
        }
    }

    /// The notice of a variable carries the provider's code and names the variable, in its text
    /// and as a parameter; the notice of the setting has a code of its own and names the file too.
    /// Each says that the cross-project guard does not run and that prompts are answered only in
    /// the terminal, in the window's words.
    #[test]
    fn the_notice_names_the_switch_and_the_file_of_a_setting() {
        let notice = notice(&Switch::Variable("QWEN_CODE_SAFE_MODE"));
        assert_eq!(notice.code, Code::of_provider(HOOKS_OFF));
        assert!(notice.standing, "the hooks stay off for the whole launch");
        assert_eq!(
            notice.params.get("switch").map(String::as_str),
            Some("QWEN_CODE_SAFE_MODE")
        );
        assert_eq!(notice.params.get("path"), None);
        assert!(notice.message.contains("`QWEN_CODE_SAFE_MODE`"));
        assert!(!notice.message.contains("  "), "{}", notice.message);
        // What else goes with the hooks: the cross-project guard and answering from the window.
        // Worded as the window's copy: the hooks "turned off", the session "launched again".
        for consequence in [
            "hooks are turned off by",
            "Edits to another project's files are not blocked",
            "its prompts can be answered only in its terminal",
            "then launch the session again.",
        ] {
            assert!(notice.message.contains(consequence), "{}", notice.message);
        }

        let file = Path::new("/proj/.qwen/settings.json");
        let notice = super::notice(&Switch::Setting(file.to_path_buf()));
        assert_eq!(notice.code, Code::of_provider(HOOKS_OFF_BY_SETTING));
        assert!(notice.standing);
        assert_eq!(
            notice.params.get("switch").map(String::as_str),
            Some("disableAllHooks")
        );
        assert_eq!(
            notice.params.get("path").map(String::as_str),
            Some("/proj/.qwen/settings.json")
        );
        assert!(notice.message.contains("`/proj/.qwen/settings.json`"));
        assert!(!notice.message.contains("  "), "{}", notice.message);
        // What else goes with the hooks: the cross-project guard and answering from the window.
        // Worded as the window's copy: the hooks "turned off", the session "launched again".
        for consequence in [
            "hooks are turned off by",
            "Edits to another project's files are not blocked",
            "its prompts can be answered only in its terminal",
            "then launch the session again.",
        ] {
            assert!(notice.message.contains(consequence), "{}", notice.message);
        }
        assert!(is_notice(&notice));
        assert!(is_notice(&super::notice(&Switch::Variable(
            "QWEN_CODE_SIMPLE"
        ))));
        // Another notice, of whatever code, is none of them.
        assert!(!is_notice(&Notice::new(
            Code::of_provider(crate::providers::qwen::SYSTEM_SETTINGS_UNUSABLE),
            "another".to_string(),
            &[("path", "/x")],
        )));
    }
}
