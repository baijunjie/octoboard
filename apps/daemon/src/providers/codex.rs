//! Codex's descriptor, with its on-disk layout and the names in it only its own modules use.

use super::layout::AgentLayout;
use super::{
    AgentId, Capabilities, DeliveryQuirks, DetectionMarkers, ModelDisplay, ProviderCode,
    ProviderDescriptor, QuotaWindowKind, QuotaWindowNaming, SessionWindowName, SettingChoice,
    SettingDescriptor, SettingKind, SwitchDefault,
};

/// A `start_account_login` for a Codex account while another Codex account is being signed in:
/// Codex's browser flow holds one fixed local port. No `params`.
pub const LOGIN_BUSY: ProviderCode = ProviderCode::error("provider.codex.login_busy");
/// Reopening a Codex session was refused because `codex unarchive` could not bring its archived
/// rollout back. The session is left as it was. `params` names `detail`.
pub const UNARCHIVE_FAILED: ProviderCode = ProviderCode::error("provider.codex.unarchive_failed");

impl AgentId {
    pub const CODEX: AgentId = AgentId(DESCRIPTOR.id);
}

/// Codex: `~/.codex`, a thread's rollouts at
/// `sessions/<year>/<month>/<day>/rollout-<start>-<thread id>.jsonl` (`.jsonl.zst` once compressed;
/// a later history segment adds `_<segment id>`), moved under `archived_sessions/` by
/// `codex archive`; the thread names in `session_index.jsonl`, the thread state in
/// `state_<generation>.sqlite`.
pub(crate) const LAYOUT: AgentLayout = AgentLayout {
    config_dir_var: "CODEX_HOME",
    default_dir: ".codex",
    records_dir: CODEX_RECORDS_DIR,
    archived_records_dir: Some(CODEX_ARCHIVED_DIR),
    record_level: 4,
    search_depth: 6,
    title_index: Some(CODEX_TITLE_INDEX),
    title_summary: None,
};

/// Where Codex keeps a thread's rollouts.
pub(crate) const CODEX_RECORDS_DIR: &str = "sessions";

/// Where `codex archive` moves a thread's rollouts, filed as under `sessions/`.
pub(crate) const CODEX_ARCHIVED_DIR: &str = "archived_sessions";

/// Codex's index of thread names, one line per name given.
pub(crate) const CODEX_TITLE_INDEX: &str = "session_index.jsonl";

/// The start of every Codex rollout's name.
pub(crate) const CODEX_ROLLOUT_PREFIX: &str = "rollout-";

/// The end of a Codex rollout's name.
pub(crate) const CODEX_ROLLOUT_SUFFIX: &str = ".jsonl";

/// The end of a Codex rollout's name once Codex has compressed it (it does so for rollouts older
/// than a week).
pub(crate) const CODEX_COMPRESSED_ROLLOUT_SUFFIX: &str = ".jsonl.zst";

/// Codex's prompt history, one line per prompt.
pub(crate) const CODEX_PROMPT_HISTORY: &str = "history.jsonl";

/// What a Codex thread state database's name starts and ends with, around its schema generation,
/// which a newer Codex may raise.
pub(crate) const CODEX_STATE_DATABASE: (&str, &str) = ("state_", ".sqlite");

/// Codex's sign-in.
pub(crate) const CODEX_SIGN_IN_FILE: &str = "auth.json";

/// Codex's own configuration file.
pub(crate) const CODEX_CONFIG_FILE: &str = "config.toml";

/// Codex's cached model catalogue.
pub(crate) const CODEX_MODELS_CACHE: &str = "models_cache.json";

pub(super) const DESCRIPTOR: ProviderDescriptor = ProviderDescriptor {
    id: "codex",
    label: "Codex",
    short_name: "Codex",
    install_url: "https://github.com/openai/codex",
    binary: "codex",
    layout: &LAYOUT,
    detection: DetectionMarkers {
        own: &[".codex"],
        generic: &["AGENTS.md", ".agents"],
    },
    instruction_file: "AGENTS.md",
    tool_name_prefix: "mcp__",
    // Its structured-question features are off in the measured version.
    ask_question_tool: None,
    capabilities: Capabilities {
        // Its id becomes known from the first hook payload.
        preallocates_session_id: false,
        hook_script: true,
        status_line: false,
        reset_credits: true,
        per_session_trust: false,
        config_sharing: true,
        delivery: DeliveryQuirks {
            confirms_by_reported_prompt: false,
            holds_report_for_completion_notice: false,
            resume_instruction_as_launch_prompt: false,
        },
    },
    quota_windows: QuotaWindowNaming {
        kinds: &[QuotaWindowKind::Session, QuotaWindowKind::Weekly],
        session_window: SessionWindowName::FiveHour,
        weekly_all_models: false,
    },
    model_display: ModelDisplay::PLAIN,
    compatibility_source: false,
    settings: &[
        SettingDescriptor {
            id: "approval_reviewer",
            kind: SettingKind::Choice {
                choices: &[
                    SettingChoice {
                        value: "user",
                        confirm: false,
                    },
                    SettingChoice {
                        value: "auto_review",
                        confirm: false,
                    },
                    SettingChoice {
                        value: "full_access",
                        confirm: true,
                    },
                ],
            },
            // Only a launch flag, like Claude Code's `approval_mode`: it travels, and an agent
            // never reads or changes it.
            exported: true,
            shown_to_agents: false,
            compatibility_with: None,
        },
        SettingDescriptor {
            id: "claude_compatibility",
            kind: SettingKind::Switch {
                default: SwitchDefault::WhileAdded("claude"),
            },
            exported: true,
            shown_to_agents: true,
            compatibility_with: Some("claude"),
        },
    ],
    codes: &[LOGIN_BUSY, UNARCHIVE_FAILED],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_names_are_the_ones_codex_writes() {
        assert_eq!(AgentId::CODEX.descriptor().layout, &LAYOUT);
        assert_eq!(
            LAYOUT,
            AgentLayout {
                config_dir_var: "CODEX_HOME",
                default_dir: ".codex",
                records_dir: "sessions",
                archived_records_dir: Some("archived_sessions"),
                record_level: 4,
                search_depth: 6,
                title_index: Some("session_index.jsonl"),
                title_summary: None,
            }
        );
        assert_eq!(CODEX_RECORDS_DIR, "sessions");
        assert_eq!(CODEX_ARCHIVED_DIR, "archived_sessions");
        assert_eq!(CODEX_TITLE_INDEX, "session_index.jsonl");
        assert_eq!(CODEX_ROLLOUT_PREFIX, "rollout-");
        assert_eq!(CODEX_ROLLOUT_SUFFIX, ".jsonl");
        assert_eq!(CODEX_COMPRESSED_ROLLOUT_SUFFIX, ".jsonl.zst");
        assert_eq!(CODEX_PROMPT_HISTORY, "history.jsonl");
        assert_eq!(CODEX_STATE_DATABASE, ("state_", ".sqlite"));
        assert_eq!(CODEX_SIGN_IN_FILE, "auth.json");
        assert_eq!(CODEX_CONFIG_FILE, "config.toml");
        assert_eq!(CODEX_MODELS_CACHE, "models_cache.json");
    }

    #[test]
    fn its_codes_are_of_the_kind_they_are_used_with() {
        use super::super::ProviderCodeKind::Error;
        assert_eq!((LOGIN_BUSY.kind, UNARCHIVE_FAILED.kind), (Error, Error));
        assert_eq!(DESCRIPTOR.codes, [LOGIN_BUSY, UNARCHIVE_FAILED]);
    }
}
