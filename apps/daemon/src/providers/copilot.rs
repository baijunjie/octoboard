//! GitHub Copilot CLI's descriptor, with its on-disk layout. Its facts are measured in
//! `docs/agent-cli-reference.md`, "GitHub Copilot CLI".

use super::layout::{AgentLayout, ConfigDirVarNames};
use super::{
    AgentId, Capabilities, CreditPeriodName, DeliveryQuirks, DetectionMarkers, InteractionKinds,
    ModelDisplay, NativeMemory, ProviderCode, ProviderDescriptor, QuotaWindowKind,
    QuotaWindowNaming, SessionWindowName, SettingChoice, SettingDescriptor, SettingKind,
};

/// The file of the configuration directory that Copilot keeps its own state in, the accounts it
/// is signed in as among it. JSON below comment lines.
pub(crate) const COPILOT_STATE_FILE: &str = "config.json";

/// A login whose user declined, at Copilot's question, to keep the token in plain text where no
/// system keychain could hold it: Copilot signed the account in but kept no token, so it is not
/// signed in. `params` names `agent`.
pub const TOKEN_NOT_KEPT: ProviderCode = ProviderCode::error("provider.copilot.token_not_kept");

/// The file of a session's record directory that holds its events, one JSON object a line.
pub(crate) const COPILOT_EVENTS_FILE: &str = "events.jsonl";

impl AgentId {
    pub const COPILOT: AgentId = AgentId(DESCRIPTOR.id);
}

/// GitHub Copilot CLI: `~/.copilot`, a session's record the directory `session-state/<id>/`.
pub(crate) const LAYOUT: AgentLayout = AgentLayout {
    config_dir_var: "COPILOT_HOME",
    config_dir_var_names: ConfigDirVarNames::Directory,
    default_dir: ".copilot",
    records_dir: "session-state",
    archived_records_dir: None,
    record_level: 1,
    search_depth: 1,
    title_index: None,
    title_summary: None,
};

pub(super) const DESCRIPTOR: ProviderDescriptor = ProviderDescriptor {
    id: "copilot",
    label: "GitHub Copilot CLI",
    short_name: "Copilot",
    install_url: "https://github.com/github/copilot-cli",
    binary: "copilot",
    layout: &LAYOUT,
    detection: DetectionMarkers {
        own: &[],
        generic: &["AGENTS.md"],
    },
    // What Copilot sets for the commands its sessions run, both seen in a shell command's
    // environment (`docs/agent-cli-reference.md`, "GitHub Copilot CLI").
    inherited_session_vars: &[
        "COPILOT_AGENT_SESSION_ID",
        "COPILOT_CLI",
        "COPILOT_CLI_BINARY_VERSION",
        "COPILOT_CLI_PATH",
        "COPILOT_CLI_RESOLVED_DIST_DIR",
        "COPILOT_HOOK_DEPTH",
        "COPILOT_LOADER_PID",
    ],
    command_environment: &[],
    instruction_file: "AGENTS.md",
    instruction_import_prefix: None,
    // An MCP server's tool reaches the model as `<server>-<tool>`.
    tool_name_prefix: "",
    tool_name_separator: "-",
    ask_question_tool: Some("ask_user"),
    quit_keys: None,
    capabilities: Capabilities {
        preallocates_session_id: true,
        hook_script: false,
        status_line: false,
        reset_credits: false,
        native_app_scheme: None,
        per_session_trust: false,
        console_trust: super::ConsoleTrust::Answered,
        config_sharing: true,
        owned_accounts: true,
        login_flow: true,
        mcp_inventory: true,
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
        kinds: &[QuotaWindowKind::CreditPeriod],
        session_window: SessionWindowName::Session,
        weekly_all_models: false,
        credit_period: Some(CreditPeriodName::Plan),
    },
    model_display: ModelDisplay::PLAIN,
    compatibility_source: false,
    // Copilot Memory, kept on GitHub and on by default; only the `memory` setting of the
    // account's own settings turns it off, and no flag or variable does for an interactive
    // launch (1.0.114; `docs/agent-cli-reference.md`, "GitHub Copilot CLI", "Memory").
    native_memory: NativeMemory::Unswitchable,
    settings: &[SettingDescriptor {
        id: "approval_mode",
        kind: SettingKind::Choice {
            // The one permission mode a flag sets for an interactive session.
            choices: &[SettingChoice {
                value: "allow_all",
                confirm: true,
            }],
        },
        // A launch flag, which changes none of the account's files.
        exported: true,
        shown_to_agents: false,
        compatibility_with: None,
    }],
    codes: &[TOKEN_NOT_KEPT],
};

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    /// The variable names the configuration directory itself, whatever the directory is called.
    #[test]
    fn its_variable_names_the_directory_itself() {
        let layout = &LAYOUT;
        assert_eq!(
            layout.config_dir_export(Path::new("/accounts/work/anything")),
            Some(("COPILOT_HOME", PathBuf::from("/accounts/work/anything")))
        );
        assert_eq!(
            layout.config_dir_of_export(Path::new("/accounts/work/anything")),
            PathBuf::from("/accounts/work/anything")
        );
    }

    /// Its one setting is the permission mode: a choice that is exported, kept from agents, and
    /// confirmed, since it lets a session run every action without asking.
    #[test]
    fn its_permission_setting_is_a_confirmed_choice_kept_from_agents() {
        let [setting] = AgentId::COPILOT.descriptor().settings else {
            panic!("one setting");
        };
        assert_eq!(setting.id, "approval_mode");
        assert!(setting.exported);
        assert!(!setting.shown_to_agents);
        assert_eq!(setting.compatibility_with, None);
        let SettingKind::Choice { choices } = setting.kind else {
            panic!("a choice");
        };
        let values: Vec<_> = choices
            .iter()
            .map(|choice| (choice.value, choice.confirm))
            .collect();
        assert_eq!(values, [("allow_all", true)]);
    }

    /// An MCP server's tool reaches the model as `<server>-<tool>`, as measured
    /// (`octoboard-report`), and `ask_user` is the tool that asks the user a question.
    #[test]
    fn an_octoboard_tool_is_named_server_dash_tool_and_ask_user_is_the_question_tool() {
        assert_eq!(
            crate::session_tools::catalogue::qualified_tool_name(AgentId::COPILOT, "report"),
            "octoboard-report"
        );
        let descriptor = AgentId::COPILOT.descriptor();
        assert!(descriptor.is_ask_question_tool(Some("ask_user")));
        assert!(!descriptor.is_ask_question_tool(Some("bash")));
        assert!(!descriptor.is_ask_question_tool(None));
    }

    /// The permission setting takes its one choice and nothing else: no other value of the choice,
    /// no switch, and no value of a setting that is not declared.
    #[test]
    fn its_permission_setting_accepts_its_one_choice_alone() {
        use crate::providers::SettingValue;
        let descriptor = AgentId::COPILOT.descriptor();
        let setting = descriptor.setting("approval_mode").expect("the setting");
        assert!(setting.accepts(&SettingValue::Choice("allow_all".to_string())));
        assert!(!setting.accepts(&SettingValue::Choice("manual".to_string())));
        assert!(!setting.accepts(&SettingValue::Switch(true)));
        assert!(!setting.overridable());
        assert!(descriptor.setting("other").is_none());
    }

    /// Its one code is the login's, which an `error` carries.
    #[test]
    fn its_code_is_the_logins_and_of_the_kind_it_is_used_with() {
        assert_eq!(TOKEN_NOT_KEPT.kind, super::super::ProviderCodeKind::Error);
        assert_eq!(TOKEN_NOT_KEPT.as_str(), "provider.copilot.token_not_kept");
        assert_eq!(DESCRIPTOR.codes, [TOKEN_NOT_KEPT]);
    }
}
