//! Antigravity CLI's descriptor, with its on-disk layout. Its facts were measured on 1.5.14
//! (`docs/agent-cli-reference.md`, section "Antigravity CLI").

use super::layout::{AgentLayout, ConfigDirVarNames};
use super::{
    AgentId, Capabilities, DeliveryQuirks, DetectionMarkers, InteractionKinds, ModelDisplay,
    NativeMemory, ProviderCode, ProviderDescriptor, QuotaWindowKind, QuotaWindowNaming,
    SessionWindowName, SettingChoice, SettingDescriptor, SettingKind,
};

/// The version every fact of this provider was measured on, named to the user where a launch is
/// refused for the CLI not taking what that version takes.
pub(crate) const MEASURED_VERSION: &str = "1.5.14";

/// Entries of the directory Antigravity CLI keeps under the home directory.
pub(crate) const APP_DATA_DIR: &str = "antigravity-cli";
pub(crate) const CONVERSATIONS_DIR: &str = "antigravity-cli/conversations";
pub(crate) const CONFIG_DIR: &str = "config";
pub(crate) const HOOKS_FILE: &str = "hooks.json";
pub(crate) const MCP_CONFIG_FILE: &str = "mcp_config.json";
pub(crate) const GLOBAL_RULE_FILE: &str = "GEMINI.md";
/// The CLI's settings, inside its own data directory.
pub(crate) const SETTINGS_FILE: &str = "settings.json";

/// The CLI did not take the flag that names a launch's directory, so the session could not be
/// given Octoboard's hooks and tools and was not started. `version` is the version measured to
/// take it.
pub const LAUNCH_DIRECTORY_FLAG_NOT_TAKEN: ProviderCode =
    ProviderCode::error("provider.antigravity.launch_directory_flag_not_taken");

/// A file of the user's that a launch has to carry into its own directory (their hooks, their
/// MCP servers or their global rule) is there and cannot be used: starting without it would drop
/// what they set there. `path` is the file.
pub const USER_CONFIG_UNUSABLE: ProviderCode =
    ProviderCode::error("provider.antigravity.user_config_unusable");

/// The user's configuration is still in the layout of an earlier version of the CLI (entries of
/// its data directory that this version keeps in its configuration directory) and the CLI has not
/// yet moved it, which it does on a run of the user's own: a session started now would run without
/// those rules, hooks and servers. No parameter.
pub const EARLIER_LAYOUT_NOT_MIGRATED: ProviderCode =
    ProviderCode::error("provider.antigravity.earlier_layout_not_migrated");

/// The user's configuration is still in an earlier layout, and one of its two files is in both
/// places: the CLI's own migration, which a run of the CLI makes, would replace the one in the
/// configuration directory (`path`) with the earlier layout's (`earlier_path`). Nothing was run.
pub const EARLIER_LAYOUT_FILE_IN_BOTH_PLACES: ProviderCode =
    ProviderCode::error("provider.antigravity.earlier_layout_file_in_both_places");

/// The CLI reads its own settings file (`path`) as nothing, because it does not parse or a key the
/// CLI knows has a value it does not take, and runs the session on its defaults, saying so only in
/// its log: the session has none of the file's permission rules or trusted folders.
pub const SETTINGS_NOT_READ: ProviderCode =
    ProviderCode::notice("provider.antigravity.settings_not_read");

/// `rules` or `plugins`, which the layout of an earlier version kept in the CLI's data directory
/// (`path`), is still there with something in it, and the CLI reads the ones of its configuration
/// directory (`current_path`) alone: the session runs without what is only in the first. `left`
/// says which is left: `rules`, `plugins`, or `both`.
pub const EARLIER_LAYOUT_LEFTOVERS_NOT_READ: ProviderCode =
    ProviderCode::notice("provider.antigravity.earlier_layout_leftovers_not_read");

impl AgentId {
    pub const ANTIGRAVITY: AgentId = AgentId(DESCRIPTOR.id);
}

/// Antigravity CLI: `~/.gemini`, which it shares with Google's other Antigravity programs. No
/// variable of its own names that directory; the home directory is its parent, and only another
/// home moves it.
pub(crate) const LAYOUT: AgentLayout = AgentLayout {
    config_dir_var: "HOME",
    config_dir_var_names: ConfigDirVarNames::Parent,
    default_dir: ".gemini",
    records_dir: CONVERSATIONS_DIR,
    archived_records_dir: None,
    record_level: 1,
    search_depth: 1,
    title_index: None,
    title_summary: None,
};

pub(super) const DESCRIPTOR: ProviderDescriptor = ProviderDescriptor {
    id: "antigravity",
    label: "Antigravity CLI",
    short_name: "Antigravity",
    install_url: "https://antigravity.google/product/antigravity-cli",
    binary: "agy",
    layout: &LAYOUT,
    detection: DetectionMarkers {
        own: &["GEMINI.md"],
        generic: &["AGENTS.md", ".agents"],
    },
    inherited_session_vars: &[],
    command_environment: &[],
    instruction_file: "AGENTS.md",
    instruction_import_prefix: None,
    // An MCP server's tool is named to the user as `<server>/<tool>`; the model calls it through
    // one tool that takes the server and the tool apart.
    tool_name_prefix: "",
    tool_name_separator: "/",
    ask_question_tool: Some("ask_question"),
    quit_keys: None,
    capabilities: Capabilities {
        // A new conversation's id is first seen in the hooks of its first turn.
        preallocates_session_id: false,
        // Octoboard's hooks are commands of the launch's own `hooks.json`.
        hook_script: false,
        status_line: false,
        reset_credits: false,
        native_app_scheme: None,
        per_session_trust: false,
        console_trust: super::ConsoleTrust::PerLaunch,
        config_sharing: false,
        // Its login is one item of the system keychain, so it has the default account only.
        owned_accounts: false,
        // Google OAuth in 1.5.14 asks for a code pasted into the TUI. The account login dialog
        // can only relay yes/no answers.
        login_flow: true,
        mcp_inventory: true,
        // A command's shell is a child of the CLI leading a group of its own, and a hang-up or an
        // exit ended it, but a process of that group that ignores SIGHUP, SIGTERM and SIGINT
        // outlived both (1.5.14): the CLI does not end the groups whatever keeps them.
        ends_command_groups_on_exit: false,
        // Its numbered permission prompts and its `ask_question` form, read from the screen and
        // answered by digits (1.5.14).
        interactions: InteractionKinds::ALL,
        // An image path pasted on its own is attached to the next message (1.5.14).
        message_images: true,
        // Its slash commands run from the input box, with a completion that names the one an
        // Enter runs (1.5.14).
        slash_commands: true,
        delivery: DeliveryQuirks {
            confirms_by_reported_prompt: false,
            holds_report_for_completion_notice: false,
            resume_instruction_as_launch_prompt: false,
        },
    },
    quota_windows: QuotaWindowNaming {
        kinds: &[QuotaWindowKind::WeeklyModel],
        session_window: SessionWindowName::Session,
        weekly_all_models: false,
        credit_period: None,
    },
    model_display: ModelDisplay::PLAIN,
    compatibility_source: false,
    // It keeps no memory of its own across conversations (1.5.14): no memory tool is offered to
    // the model, no setting names one, and a conversation leaves none on disk.
    native_memory: NativeMemory::Absent,
    settings: &[SettingDescriptor {
        id: "approval_mode",
        kind: SettingKind::Choice {
            choices: &[
                // `--mode accept-edits`: file edits are not asked about, commands still are.
                SettingChoice {
                    value: "accept_edits",
                    confirm: false,
                },
                // `--mode auto`: file edits and the commands the CLI judges safe are not asked
                // about (a write, `ls` and `rm -rf` of a file in the workspace ran with no prompt,
                // 1.5.14). It is the CLI's own default for a signed-in account, so it asks no
                // confirmation. Plan mode is not offered: on its first model call it let a file
                // write through to its permission prompt.
                SettingChoice {
                    value: "auto",
                    confirm: false,
                },
                // `--dangerously-skip-permissions`: nothing is asked about.
                SettingChoice {
                    value: "skip_permissions",
                    confirm: true,
                },
            ],
        },
        // Only a launch flag: it travels, and changes none of the account's files.
        exported: true,
        shown_to_agents: false,
        compatibility_with: None,
    }],
    codes: &[
        LAUNCH_DIRECTORY_FLAG_NOT_TAKEN,
        USER_CONFIG_UNUSABLE,
        EARLIER_LAYOUT_NOT_MIGRATED,
        EARLIER_LAYOUT_FILE_IN_BOTH_PLACES,
        SETTINGS_NOT_READ,
        EARLIER_LAYOUT_LEFTOVERS_NOT_READ,
    ],
};

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::providers::ProviderCodeKind;

    /// What the daemon serves for this provider: the default account only, whose conversation id
    /// the CLI gives, with the four codes it can refuse a launch with and the two it tells the
    /// user of at one, worded by the UI under these names.
    #[test]
    fn it_has_the_default_account_only_and_its_six_codes() {
        let descriptor = AgentId::ANTIGRAVITY.descriptor();
        assert_eq!(descriptor.id, "antigravity");
        assert!(!descriptor.capabilities.owned_accounts);
        assert!(!descriptor.capabilities.preallocates_session_id);
        assert_eq!(
            descriptor
                .codes
                .iter()
                .map(|code| code.as_str())
                .collect::<Vec<_>>(),
            [
                "provider.antigravity.launch_directory_flag_not_taken",
                "provider.antigravity.user_config_unusable",
                "provider.antigravity.earlier_layout_not_migrated",
                "provider.antigravity.earlier_layout_file_in_both_places",
                "provider.antigravity.settings_not_read",
                "provider.antigravity.earlier_layout_leftovers_not_read"
            ]
        );
        // Told at a launch that goes on, not refusals.
        assert_eq!(SETTINGS_NOT_READ.kind, ProviderCodeKind::Notice);
        assert_eq!(
            EARLIER_LAYOUT_LEFTOVERS_NOT_READ.kind,
            ProviderCodeKind::Notice
        );
        assert_eq!(USER_CONFIG_UNUSABLE.kind, ProviderCodeKind::Error);
    }

    /// The one setting is the permission mode: accepting edits and the auto mode ask no
    /// confirmation, skipping permissions asks one; it travels with an export and is no business of the agents' own.
    #[test]
    fn its_one_setting_is_the_approval_mode() {
        let descriptor = AgentId::ANTIGRAVITY.descriptor();
        let [setting] = descriptor.settings else {
            panic!("one setting: {}", descriptor.settings.len());
        };
        assert_eq!(setting.id, "approval_mode");
        let SettingKind::Choice { choices } = &setting.kind else {
            panic!("a choice");
        };
        assert_eq!(
            choices
                .iter()
                .map(|choice| (choice.value, choice.confirm))
                .collect::<Vec<_>>(),
            [
                ("accept_edits", false),
                ("auto", false),
                ("skip_permissions", true)
            ]
        );
        assert!(setting.exported);
        assert!(!setting.shown_to_agents);
    }

    /// The CLI keeps its files in `.gemini`, which no variable of its own moves: the home directory
    /// is its parent, with one database per conversation under the records directory.
    #[test]
    fn its_directory_is_gemini_under_the_home_directory() {
        assert_eq!(LAYOUT.config_dir_var, "HOME");
        assert_eq!(LAYOUT.default_dir, ".gemini");
        assert_eq!(
            LAYOUT.config_dir_of_export(Path::new("/home/someone")),
            Path::new("/home/someone").join(".gemini")
        );
        assert_eq!(LAYOUT.records(), Some(CONVERSATIONS_DIR));
    }
}
