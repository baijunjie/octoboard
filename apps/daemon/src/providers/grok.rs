//! Grok Build's descriptor, with its on-disk layout and the names in it only its own modules use.

use super::layout::AgentLayout;
use super::{
    AgentId, Capabilities, DeliveryQuirks, DetectionMarkers, ModelDisplay, ProviderCode,
    ProviderDescriptor, QuotaWindowKind, QuotaWindowNaming, SessionWindowName, SettingChoice,
    SettingDescriptor, SettingKind, SwitchDefault,
};

/// A Grok Build session's pinned source home exists but Grok has never been run against it, so it
/// carries no login and no session history for the per-session home to link. `params` names
/// `path`.
pub const HOME_NOT_INITIALIZED: ProviderCode =
    ProviderCode::error("provider.grok.home_not_initialized");
/// A Grok Build launch would run under Grok's sandbox, which the per-session home cannot
/// support. `params` names the `source` that sets the `profile`: an environment variable, or the
/// file it is read from.
pub const SANDBOX_UNSUPPORTED: ProviderCode =
    ProviderCode::error("provider.grok.sandbox_unsupported");

impl AgentId {
    pub const GROK: AgentId = AgentId(DESCRIPTOR.id);
}

/// Grok Build: `~/.grok`, a session's directory at
/// `sessions/<url-encoded working directory>/<id>/`, marked by its `usage.json`, holding the
/// conversation in `chat_history.jsonl` and its title in `summary.json`.
pub(crate) const LAYOUT: AgentLayout = AgentLayout {
    config_dir_var: "GROK_HOME",
    default_dir: ".grok",
    records_dir: GROK_RECORDS_DIR,
    archived_records_dir: None,
    record_level: 2,
    search_depth: 2,
    title_index: None,
    title_summary: Some(GROK_TITLE_SUMMARY),
};

/// Where Grok Build keeps its sessions' directories.
pub(crate) const GROK_RECORDS_DIR: &str = "sessions";

/// Grok Build's trust store, inside its configuration directory.
pub(crate) const GROK_TRUST_STORE: &str = "trusted_folders.toml";

/// The file inside a Grok Build session's directory that marks it as one.
pub(crate) const GROK_SESSION_MARKER: &str = "usage.json";

/// The file inside a Grok Build session's directory that holds the conversation.
pub(crate) const GROK_CONVERSATION_FILE: &str = "chat_history.jsonl";

/// The file inside a Grok Build session's directory that holds its title and how it was
/// configured.
pub(crate) const GROK_TITLE_SUMMARY: &str = "summary.json";

/// Grok Build's sign-in.
pub(crate) const GROK_SIGN_IN_FILE: &str = "auth.json";

/// Grok Build's own configuration file.
pub(crate) const GROK_CONFIG_FILE: &str = "config.toml";

/// Grok Build's cached model catalogue.
pub(crate) const GROK_MODELS_CACHE: &str = "models_cache.json";

pub(super) const DESCRIPTOR: ProviderDescriptor = ProviderDescriptor {
    id: "grok",
    label: "Grok Build",
    short_name: "Grok",
    install_url: "https://docs.x.ai/build/overview",
    binary: "grok",
    layout: &LAYOUT,
    detection: DetectionMarkers {
        own: &[".grok"],
        generic: &["AGENTS.md", ".agents"],
    },
    instruction_file: "AGENTS.md",
    // A line `@AGENTS.md` reaches the model as written (Grok Build 1.0.77).
    instruction_import_prefix: None,
    // Grok Build presents an MCP tool as `<server>__<tool>`, with nothing before it.
    tool_name_prefix: "",
    ask_question_tool: Some("ask_user_question"),
    quit_keys: None,
    capabilities: Capabilities {
        preallocates_session_id: true,
        hook_script: true,
        status_line: false,
        reset_credits: false,
        per_session_trust: true,
        config_sharing: true,
        mcp_inventory: true,
        delivery: DeliveryQuirks {
            confirms_by_reported_prompt: false,
            holds_report_for_completion_notice: false,
            resume_instruction_as_launch_prompt: false,
        },
    },
    quota_windows: QuotaWindowNaming {
        kinds: &[QuotaWindowKind::CreditPeriod],
        session_window: SessionWindowName::FiveHour,
        weekly_all_models: false,
    },
    model_display: ModelDisplay::PLAIN,
    compatibility_source: false,
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
                        confirm: false,
                    },
                    SettingChoice {
                        value: "always_approve",
                        confirm: true,
                    },
                ],
            },
            // A process-local permission flag travels with the settings, without changing the account's files.
            exported: true,
            shown_to_agents: false,
            compatibility_with: None,
        },
        SettingDescriptor {
            id: "claude_compatibility",
            kind: SettingKind::Switch {
                default: SwitchDefault::On,
            },
            exported: true,
            shown_to_agents: true,
            compatibility_with: Some("claude"),
        },
    ],
    codes: &[HOME_NOT_INITIALIZED, SANDBOX_UNSUPPORTED],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_names_are_the_ones_grok_build_writes() {
        assert_eq!(AgentId::GROK.descriptor().layout, &LAYOUT);
        assert_eq!(
            LAYOUT,
            AgentLayout {
                config_dir_var: "GROK_HOME",
                default_dir: ".grok",
                records_dir: "sessions",
                archived_records_dir: None,
                record_level: 2,
                search_depth: 2,
                title_index: None,
                title_summary: Some("summary.json"),
            }
        );
        assert_eq!(GROK_RECORDS_DIR, "sessions");
        assert_eq!(GROK_TRUST_STORE, "trusted_folders.toml");
        assert_eq!(GROK_SESSION_MARKER, "usage.json");
        assert_eq!(GROK_CONVERSATION_FILE, "chat_history.jsonl");
        assert_eq!(GROK_TITLE_SUMMARY, "summary.json");
        assert_eq!(GROK_SIGN_IN_FILE, "auth.json");
        assert_eq!(GROK_CONFIG_FILE, "config.toml");
        assert_eq!(GROK_MODELS_CACHE, "models_cache.json");
    }

    /// Its session directories are matched by position (`RecordStore::indexed` on the last level
    /// of the walk), which is the record's own level only while the search goes exactly that deep.
    #[test]
    fn the_search_goes_exactly_as_deep_as_a_session_directory_sits() {
        assert_eq!(LAYOUT.search_depth, LAYOUT.record_level);
    }

    #[test]
    fn its_codes_are_of_the_kind_they_are_used_with() {
        use super::super::ProviderCodeKind::Error;
        assert_eq!(
            (HOME_NOT_INITIALIZED.kind, SANDBOX_UNSUPPORTED.kind),
            (Error, Error)
        );
        assert_eq!(
            DESCRIPTOR.codes,
            [HOME_NOT_INITIALIZED, SANDBOX_UNSUPPORTED]
        );
    }
}
