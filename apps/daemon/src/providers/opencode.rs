//! OpenCode's descriptor, with its on-disk layout. Its versions were measured on 1.19.6, 2.3.2,
//! 2.4.0 and 2.4.1 (`docs/agent-cli-reference.md`, section "OpenCode"). No version is refused; one of a minor not
//! measured raises a notice.

use super::layout::{AgentLayout, ConfigDirVarNames};
use super::{
    AgentId, Capabilities, DeliveryQuirks, DetectionMarkers, InteractionKinds, ModelDisplay,
    NativeMemory, ProviderCode, ProviderDescriptor, QuotaWindowNaming, SessionWindowName,
    SettingChoice, SettingDescriptor, SettingKind, SwitchDefault,
};

pub const VERSION_UNMEASURED: ProviderCode =
    ProviderCode::notice("provider.opencode.version_unmeasured");
pub const MIGRATION_REQUIRED: ProviderCode =
    ProviderCode::error("provider.opencode.migration_required");

/// The versions measured, each with a captured launch, server and record format. They gate nothing:
/// a launch of any version goes ahead, and only the notice [`VERSION_UNMEASURED`] reads them, for a
/// version whose major and minor none of them shares.
const MEASURED_VERSIONS: [(u32, u32, u32); 4] = [(1, 19, 6), (2, 3, 2), (2, 4, 0), (2, 4, 1)];

pub(crate) fn is_measured((major, minor, _): (u32, u32, u32)) -> bool {
    MEASURED_VERSIONS
        .iter()
        .any(|&(measured_major, measured_minor, _)| {
            (measured_major, measured_minor) == (major, minor)
        })
}

impl AgentId {
    pub const OPENCODE: AgentId = AgentId(DESCRIPTOR.id);
}

/// OpenCode: `~/.local/share/opencode`, named through the parent `XDG_DATA_HOME` names. It keeps its
/// conversations in a database, not in record files.
pub(crate) const LAYOUT: AgentLayout = AgentLayout {
    config_dir_var: "XDG_DATA_HOME",
    config_dir_var_names: ConfigDirVarNames::Parent,
    default_dir: ".local/share/opencode",
    records_dir: "",
    archived_records_dir: None,
    record_level: 0,
    search_depth: 0,
    title_index: None,
    title_summary: None,
};

pub(super) const DESCRIPTOR: ProviderDescriptor = ProviderDescriptor {
    id: "opencode",
    label: "OpenCode",
    short_name: "OpenCode",
    install_url: "https://opencode.ai",
    binary: "opencode",
    layout: &LAYOUT,
    detection: DetectionMarkers {
        own: &[".opencode", "opencode.json", "opencode.jsonc"],
        generic: &["AGENTS.md"],
    },
    // Set by the CLI on its own process as it starts, so every program it runs inherits them.
    inherited_session_vars: &["AGENT", "OPENCODE", "OPENCODE_PID"],
    command_environment: &[],
    instruction_file: "AGENTS.md",
    instruction_import_prefix: None,
    // An MCP server's tool reaches the model as `<server>_<tool>`.
    tool_name_prefix: "",
    tool_name_separator: "_",
    // On by default in the TUI.
    ask_question_tool: Some("question"),
    // It ends its turn and the command it runs, and exits, on the host's request to end.
    quit_keys: None,
    capabilities: Capabilities {
        preallocates_session_id: false,
        // Octoboard's plugin inside OpenCode runs the script for every event it forwards.
        hook_script: true,
        status_line: false,
        reset_credits: false,
        native_app_scheme: None,
        per_session_trust: false,
        console_trust: super::ConsoleTrust::Answered,
        config_sharing: false,
        // No variable moves its data directory, so it has the default account only.
        owned_accounts: false,
        login_flow: false,
        mcp_inventory: true,
        // Every command it runs leads a process group of its own, a child of OpenCode's process,
        // and it ends those groups as it exits on the host's request to end, a process the command
        // left behind included (1.19.6).
        ends_command_groups_on_exit: true,
        // Its permission prompt and its question are read off the screen, matched to the calls
        // its plugin forwards, and answered by keys (1.19.6).
        interactions: InteractionKinds::ALL,
        // An image file's path pasted on its own becomes an attachment, `[Image <n>]` in the box.
        message_images: true,
        // A command pasted with `/` first and submitted runs as the command.
        slash_commands: true,
        delivery: DeliveryQuirks {
            // Its plugin reports every prompt submitted at the idle prompt (`turn.started`, with
            // the prompt, 0.1 to 0.6 s after the Enter, measured), and a write can land in a dialog
            // the screen reader does not recognise (one drawn narrower than 40 columns): a write
            // no prompt follows is told to its sender as unsent, never taken as sent.
            confirms_by_reported_prompt: true,
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
    // It keeps no memory of its own across sessions (1.19.6): what it carries from one session to
    // the next is the instruction files the user keeps, `AGENTS.md` among them, which it writes
    // only when the user runs its `/init` command.
    native_memory: NativeMemory::Absent,
    settings: &[
        SettingDescriptor {
            id: "approval_mode",
            kind: SettingKind::Choice {
                choices: &[
                    SettingChoice {
                        value: "ask",
                        confirm: false,
                    },
                    SettingChoice {
                        value: "auto",
                        confirm: true,
                    },
                ],
            },
            // A launch flag (`auto`) or a default Octoboard's plugin writes into the configuration
            // OpenCode merged, in memory (`ask`): it travels, and changes none of the account's
            // files. With no choice OpenCode's own configuration decides.
            exported: true,
            shown_to_agents: false,
            compatibility_with: None,
        },
        SettingDescriptor {
            id: "claude_compatibility",
            kind: SettingKind::Switch {
                default: SwitchDefault::Off,
            },
            exported: true,
            shown_to_agents: true,
            compatibility_with: Some("claude"),
        },
    ],
    codes: &[VERSION_UNMEASURED, MIGRATION_REQUIRED],
};

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    #[test]
    fn the_layout_names_are_the_ones_opencode_is_expected_to_use() {
        assert_eq!(AgentId::OPENCODE.descriptor().layout, &LAYOUT);
        assert_eq!(
            LAYOUT,
            AgentLayout {
                config_dir_var: "XDG_DATA_HOME",
                config_dir_var_names: ConfigDirVarNames::Parent,
                default_dir: ".local/share/opencode",
                records_dir: "",
                archived_records_dir: None,
                record_level: 0,
                search_depth: 0,
                title_index: None,
                title_summary: None,
            }
        );
    }

    #[test]
    fn its_descriptor_states_its_initial_facts() {
        let descriptor = AgentId::OPENCODE.descriptor();
        assert_eq!(descriptor.id, "opencode");
        assert_eq!(descriptor.label, "OpenCode");
        assert_eq!(descriptor.short_name, "OpenCode");
        assert_eq!(descriptor.binary, "opencode");
        assert_eq!(
            descriptor.detection.own,
            &[".opencode", "opencode.json", "opencode.jsonc"]
        );
        assert_eq!(descriptor.detection.generic, &["AGENTS.md"]);
        assert_eq!(descriptor.instruction_file, "AGENTS.md");
        assert_eq!(descriptor.tool_name_prefix, "");
        assert_eq!(descriptor.tool_name_separator, "_");
        assert_eq!(descriptor.ask_question_tool, Some("question"));
        assert!(!descriptor.capabilities.preallocates_session_id);
        assert!(!descriptor.capabilities.owned_accounts);
    }

    /// The variable names the parent of the configuration directory, so only a directory named
    /// `opencode` can be exported, and what the CLI then uses is that entry of the exported value.
    #[test]
    fn its_variable_names_the_parent_of_the_directory() {
        let layout = &LAYOUT;
        assert_eq!(
            layout.config_dir_export(Path::new("/accounts/work/opencode")),
            Some(("XDG_DATA_HOME", PathBuf::from("/accounts/work")))
        );
        assert_eq!(
            layout.config_dir_export(Path::new("/accounts/work/other")),
            None
        );
        assert_eq!(
            layout.config_dir_of_export(Path::new("/accounts/work")),
            PathBuf::from("/accounts/work/opencode")
        );
    }

    /// The hook script its plugin runs, the end of its commands' process groups at exit, image
    /// messages and slash commands. The descriptor also declares its launch refusal codes.
    #[test]
    fn it_declares_the_capabilities_measured_on_its_cli() {
        let descriptor = AgentId::OPENCODE.descriptor();
        assert_eq!(
            descriptor.capabilities,
            Capabilities {
                preallocates_session_id: false,
                hook_script: true,
                status_line: false,
                reset_credits: false,
                native_app_scheme: None,
                per_session_trust: false,
                console_trust: super::super::ConsoleTrust::Answered,
                config_sharing: false,
                owned_accounts: false,
                login_flow: false,
                mcp_inventory: true,
                ends_command_groups_on_exit: true,
                interactions: InteractionKinds::ALL,
                message_images: true,
                slash_commands: true,
                delivery: DeliveryQuirks {
                    confirms_by_reported_prompt: true,
                    holds_report_for_completion_notice: false,
                    resume_instruction_as_launch_prompt: false,
                },
            }
        );
        assert!(descriptor.quota_windows.kinds.is_empty());
        assert_eq!(descriptor.model_display, ModelDisplay::PLAIN);
        assert_eq!(descriptor.codes, &[VERSION_UNMEASURED, MIGRATION_REQUIRED]);
        assert_eq!(
            descriptor.inherited_session_vars,
            &["AGENT", "OPENCODE", "OPENCODE_PID"]
        );
        assert!(!descriptor.compatibility_source);
        assert_eq!(descriptor.native_memory, NativeMemory::Absent);
        assert!(descriptor.install_url.starts_with("https://"));
    }

    /// Permission approval offers `ask` and `auto`, the second behind a
    /// confirmation. It reaches a launch only, so it travels, and no agent is shown it.
    #[test]
    fn approving_tools_automatically_asks_for_confirmation() {
        let settings = AgentId::OPENCODE.descriptor().settings;
        assert_eq!(settings.len(), 2);
        let setting = &settings[0];
        assert_eq!(setting.id, "approval_mode");
        assert_eq!(
            setting.kind,
            SettingKind::Choice {
                choices: &[
                    SettingChoice {
                        value: "ask",
                        confirm: false
                    },
                    SettingChoice {
                        value: "auto",
                        confirm: true
                    },
                ]
            }
        );
        assert!(setting.exported);
        assert!(!setting.shown_to_agents);
        assert_eq!(setting.compatibility_with, None);
    }

    /// An Octoboard tool reaches the model as `octoboard_<tool>`, which is also the name OpenCode
    /// asks permission for; the ask-the-user tool it has on by default is `question`.
    #[test]
    fn an_octoboard_tool_is_named_octoboard_underscore_tool() {
        assert_eq!(
            crate::session_tools::catalogue::qualified_tool_name(AgentId::OPENCODE, "get_session"),
            "octoboard_get_session"
        );
        assert_eq!(
            AgentId::OPENCODE.descriptor().ask_question_tool,
            Some("question")
        );
    }

    /// It ends on the host's request to end, so it names no key that quits it at the prompt.
    #[test]
    fn it_names_no_quit_key() {
        assert!(AgentId::OPENCODE.descriptor().quit_keys.is_none());
    }
}
