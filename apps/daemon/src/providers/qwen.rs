//! Qwen Code's descriptor, with its on-disk layout. Its facts are measured in
//! `docs/agent-cli-reference.md`, section "Qwen Code".

use super::layout::{AgentLayout, ConfigDirVarNames};
use super::{
    AgentId, Capabilities, DeliveryQuirks, DetectionMarkers, InteractionKinds, ModelDisplay,
    NativeMemory, ProviderCode, ProviderDescriptor, QuotaWindowNaming, SessionWindowName,
    SettingChoice, SettingDescriptor, SettingKind, SwitchDefault,
};

/// The file under the configuration directory that holds an account's settings, the sign-in
/// method it chose among them.
pub(crate) const QWEN_SETTINGS_FILE: &str = "settings.json";

/// The file under the configuration directory that holds the tokens of an account signed in to a
/// Qwen subscription.
pub(crate) const QWEN_SUBSCRIPTION_FILE: &str = "subscription_credentials.json";

/// A launch refused because the system settings file it would stand in for is there and cannot be
/// read as settings: starting without it would drop what an administrator set there. `path` is
/// the file.
pub const SYSTEM_SETTINGS_UNUSABLE: ProviderCode =
    ProviderCode::error("provider.qwen.system_settings_unusable");

/// A session started with every hook of Qwen Code's switched off by a variable of its environment
/// (safe mode or bare mode), so its status follows only what Qwen Code reports in its terminal.
/// `switch` is the variable.
pub const HOOKS_OFF: ProviderCode = ProviderCode::notice("provider.qwen.hooks_off");

/// The same, switched off by a setting: `switch` is the setting and `path` the settings file whose
/// value decides, where the user finds it.
pub const HOOKS_OFF_BY_SETTING: ProviderCode =
    ProviderCode::notice("provider.qwen.hooks_off_by_setting");

pub(crate) const CLAUDE_INSTRUCTIONS: &str = "claude_instructions";

impl AgentId {
    pub const QWEN: AgentId = AgentId(DESCRIPTOR.id);
}

/// Qwen Code: `~/.qwen`, a session's record at `projects/<sanitized working directory>/chats/<id>.jsonl`.
pub(crate) const LAYOUT: AgentLayout = AgentLayout {
    config_dir_var: "QWEN_HOME",
    config_dir_var_names: ConfigDirVarNames::Directory,
    default_dir: ".qwen",
    records_dir: "projects",
    archived_records_dir: None,
    record_level: 3,
    search_depth: 3,
    title_index: None,
    title_summary: None,
};

pub(super) const DESCRIPTOR: ProviderDescriptor = ProviderDescriptor {
    id: "qwen",
    label: "Qwen Code",
    short_name: "Qwen",
    install_url: "https://github.com/QwenLM/qwen-code",
    binary: "qwen",
    layout: &LAYOUT,
    detection: DetectionMarkers {
        own: &[".qwen", "QWEN.md"],
        generic: &["AGENTS.md", ".agents"],
    },
    // What 0.28.0 exports to the commands and MCP servers it starts.
    inherited_session_vars: &[
        "QWEN_CODE",
        "QWEN_CODE_AGENT_ID",
        "QWEN_CODE_CLI",
        "QWEN_CODE_MANAGED_NPM_PIN",
        "QWEN_CODE_MANAGED_NPM_ROOT",
        "QWEN_CODE_MESSAGING_SOCKET",
        "QWEN_CODE_MESSAGING_TOKEN",
        "QWEN_CODE_MODEL",
        "QWEN_CODE_MODEL_IDENTITY",
        "QWEN_CODE_PRIVATE_RELAUNCH_ENV_PROVENANCE",
        "QWEN_CODE_PROJECT_DIR",
        "QWEN_CODE_PROMPT_ID",
        "QWEN_CODE_SESSION_ID",
        "QWEN_CODE_STARTUP_VERSION",
    ],
    command_environment: &[],
    instruction_file: "QWEN.md",
    instruction_import_prefix: None,
    tool_name_prefix: "mcp__",
    tool_name_separator: "__",
    ask_question_tool: Some("ask_user_question"),
    quit_keys: None,
    capabilities: Capabilities {
        preallocates_session_id: true,
        hook_script: false,
        status_line: false,
        reset_credits: false,
        native_app_scheme: None,
        per_session_trust: false,
        console_trust: super::ConsoleTrust::PerLaunch,
        config_sharing: true,
        owned_accounts: true,
        login_flow: true,
        mcp_inventory: true,
        // Measured on 0.28.3: a command's shell leads a process group of its own, and the groups
        // still running, an orphan of the group included, end as Qwen Code exits.
        ends_command_groups_on_exit: true,
        interactions: InteractionKinds::ALL,
        message_images: true,
        slash_commands: true,
        delivery: DeliveryQuirks {
            confirms_by_reported_prompt: false,
            holds_report_for_completion_notice: false,
            resume_instruction_as_launch_prompt: false,
        },
    },
    quota_windows: QuotaWindowNaming {
        kinds: &[],
        session_window: SessionWindowName::Session,
        weekly_all_models: false,
        credit_period: None,
    },
    model_display: ModelDisplay::PLAIN,
    compatibility_source: false,
    // Its managed memory (on by default), switched off for one launch in the launch's system
    // settings file (0.28.3).
    native_memory: NativeMemory::Switchable,
    settings: &[
        SettingDescriptor {
            id: "approval_mode",
            kind: SettingKind::Choice {
                choices: &[
                    SettingChoice {
                        value: "plan",
                        confirm: false,
                    },
                    SettingChoice {
                        value: "default",
                        confirm: false,
                    },
                    SettingChoice {
                        value: "auto_edit",
                        confirm: false,
                    },
                    SettingChoice {
                        value: "auto",
                        confirm: false,
                    },
                    SettingChoice {
                        value: "yolo",
                        confirm: true,
                    },
                ],
            },
            // The choice only becomes a launch flag, so it travels like any other setting; an agent
            // never reads or changes it.
            exported: true,
            shown_to_agents: false,
            compatibility_with: None,
        },
        SettingDescriptor {
            id: CLAUDE_INSTRUCTIONS,
            kind: SettingKind::Switch {
                default: SwitchDefault::Off,
            },
            exported: true,
            shown_to_agents: true,
            compatibility_with: Some("claude"),
        },
    ],
    codes: &[SYSTEM_SETTINGS_UNUSABLE, HOOKS_OFF, HOOKS_OFF_BY_SETTING],
};

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    #[test]
    fn the_layout_names_are_the_ones_qwen_is_expected_to_use() {
        assert_eq!(AgentId::QWEN.descriptor().layout, &LAYOUT);
        assert_eq!(
            LAYOUT,
            AgentLayout {
                config_dir_var: "QWEN_HOME",
                config_dir_var_names: ConfigDirVarNames::Directory,
                default_dir: ".qwen",
                records_dir: "projects",
                archived_records_dir: None,
                record_level: 3,
                search_depth: 3,
                title_index: None,
                title_summary: None,
            }
        );
    }

    #[test]
    fn its_descriptor_states_its_initial_facts() {
        let descriptor = AgentId::QWEN.descriptor();
        assert_eq!(descriptor.id, "qwen");
        assert_eq!(descriptor.label, "Qwen Code");
        assert_eq!(descriptor.short_name, "Qwen");
        assert_eq!(descriptor.binary, "qwen");
        assert_eq!(descriptor.detection.own, &[".qwen", "QWEN.md"]);
        assert_eq!(descriptor.detection.generic, &["AGENTS.md", ".agents"]);
        assert_eq!(descriptor.instruction_file, "QWEN.md");
        assert_eq!(descriptor.tool_name_prefix, "mcp__");
        assert_eq!(descriptor.tool_name_separator, "__");
        assert_eq!(descriptor.ask_question_tool, Some("ask_user_question"));
        assert!(descriptor.capabilities.preallocates_session_id);
        assert!(descriptor.capabilities.owned_accounts);
    }

    /// The capabilities measured on 0.28.3 (`docs/agent-cli-reference.md`, "Qwen Code"): it ends
    /// its commands' process groups as it exits, its permission prompts and questions are
    /// answered from the window, it takes images and slash commands, and its own memory can be
    /// switched off for one launch.
    #[test]
    fn it_declares_the_capabilities_measured() {
        let descriptor = AgentId::QWEN.descriptor();
        let capabilities = &descriptor.capabilities;
        assert!(capabilities.ends_command_groups_on_exit);
        assert_eq!(capabilities.interactions, InteractionKinds::ALL);
        assert!(capabilities.message_images);
        assert!(capabilities.slash_commands);
        assert_eq!(descriptor.native_memory, NativeMemory::Switchable);
    }

    /// The variable names the configuration directory itself, whatever the directory is called.
    #[test]
    fn its_variable_names_the_directory_itself() {
        let layout = &LAYOUT;
        assert_eq!(
            layout.config_dir_export(Path::new("/accounts/work/anything")),
            Some(("QWEN_HOME", PathBuf::from("/accounts/work/anything")))
        );
        assert_eq!(
            layout.config_dir_of_export(Path::new("/accounts/work/anything")),
            PathBuf::from("/accounts/work/anything")
        );
    }

    /// It declares no quota window and no model naming of its own, and is no compatibility source;
    /// its codes are the error of a launch refused over a system settings file it cannot use and
    /// the notice of a session whose hooks the user switched off.
    #[test]
    fn it_declares_no_quota_or_model_naming_and_its_codes() {
        let descriptor = AgentId::QWEN.descriptor();
        assert!(descriptor.quota_windows.kinds.is_empty());
        assert_eq!(descriptor.model_display, ModelDisplay::PLAIN);
        assert_eq!(
            descriptor.codes,
            [SYSTEM_SETTINGS_UNUSABLE, HOOKS_OFF, HOOKS_OFF_BY_SETTING]
        );
        assert_eq!(HOOKS_OFF.as_str(), "provider.qwen.hooks_off");
        assert_eq!(
            HOOKS_OFF_BY_SETTING.as_str(),
            "provider.qwen.hooks_off_by_setting"
        );
        for notice in [HOOKS_OFF, HOOKS_OFF_BY_SETTING] {
            assert_eq!(notice.kind, crate::providers::ProviderCodeKind::Notice);
        }
        assert_eq!(
            SYSTEM_SETTINGS_UNUSABLE.as_str(),
            "provider.qwen.system_settings_unusable"
        );
        assert_eq!(
            SYSTEM_SETTINGS_UNUSABLE.kind,
            crate::providers::ProviderCodeKind::Error
        );
        assert!(!descriptor.compatibility_source);
        assert!(descriptor.install_url.starts_with("https://"));
    }

    /// What Qwen Code 0.28.0 exports to the commands and MCP servers it starts, as
    /// `docs/agent-cli-reference.md` lists them.
    const EXPORTED_BY_QWEN: [&str; 14] = [
        "QWEN_CODE",
        "QWEN_CODE_AGENT_ID",
        "QWEN_CODE_CLI",
        "QWEN_CODE_MANAGED_NPM_PIN",
        "QWEN_CODE_MANAGED_NPM_ROOT",
        "QWEN_CODE_MESSAGING_SOCKET",
        "QWEN_CODE_MESSAGING_TOKEN",
        "QWEN_CODE_MODEL",
        "QWEN_CODE_MODEL_IDENTITY",
        "QWEN_CODE_PRIVATE_RELAUNCH_ENV_PROVENANCE",
        "QWEN_CODE_PROJECT_DIR",
        "QWEN_CODE_PROMPT_ID",
        "QWEN_CODE_SESSION_ID",
        "QWEN_CODE_STARTUP_VERSION",
    ];

    /// A session's commands inherit exactly the variables Qwen Code itself exports, so a command
    /// the agent starts is told apart from a shell the user opened.
    #[test]
    fn it_names_the_session_variables_qwen_exports_to_what_it_starts() {
        let mut declared = AgentId::QWEN.descriptor().inherited_session_vars.to_vec();
        declared.sort_unstable();
        assert_eq!(declared, EXPORTED_BY_QWEN);
        // The variables Octoboard itself sets for a launch are not among them.
        for own in [
            "QWEN_HOME",
            "QWEN_CODE_NO_RELAUNCH",
            "QWEN_CODE_SYSTEM_SETTINGS_PATH",
        ] {
            assert!(!declared.contains(&own), "{own}");
        }
    }

    /// Permission mode is a choice of Qwen Code's five modes in the order the
    /// settings page lists them; only the one that stops asking needs a confirmation. It is sent
    /// to the daemon's other ends, and no agent reads or changes it.
    #[test]
    fn its_approval_mode_asks_for_confirmation_for_yolo() {
        let setting = AgentId::QWEN
            .descriptor()
            .setting("approval_mode")
            .expect("approval mode");
        assert_eq!(setting.id, "approval_mode");
        assert!(setting.exported);
        assert!(!setting.shown_to_agents);
        assert!(setting.compatibility_with.is_none());
        let SettingKind::Choice { choices } = &setting.kind else {
            panic!("a choice");
        };
        let listed: Vec<(&str, bool)> = choices
            .iter()
            .map(|choice| (choice.value, choice.confirm))
            .collect();
        assert_eq!(
            listed,
            [
                ("plan", false),
                ("default", false),
                ("auto_edit", false),
                ("auto", false),
                ("yolo", true),
            ]
        );
    }

    /// Only one of its values is accepted, and unset stays unset rather than falling to a default.
    #[test]
    fn its_approval_mode_takes_only_its_own_values_and_has_no_default() {
        use crate::providers::SettingValue;
        let setting = AgentId::QWEN
            .descriptor()
            .setting("approval_mode")
            .expect("approval mode");
        for value in ["plan", "default", "auto_edit", "auto", "yolo"] {
            assert!(
                setting.accepts(&SettingValue::Choice(value.to_string())),
                "{value}"
            );
        }
        for value in ["auto-edit", "bypass", ""] {
            assert!(
                !setting.accepts(&SettingValue::Choice(value.to_string())),
                "{value}"
            );
        }
        assert!(!setting.accepts(&SettingValue::Switch(true)));
        assert_eq!(setting.in_effect(None), None);
        assert_eq!(
            setting.in_effect(Some(&SettingValue::Choice("bypass".to_string()))),
            None
        );
    }
}
