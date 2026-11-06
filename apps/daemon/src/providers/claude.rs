//! Claude Code's descriptor, with its on-disk layout and the names in it only its own modules use.

use super::layout::AgentLayout;
use super::{
    AgentId, Capabilities, DeliveryQuirks, DetectionMarkers, ModelDisplay, ProviderCode,
    ProviderDescriptor, QuotaWindowKind, QuotaWindowNaming, SessionWindowName, SettingChoice,
    SettingDescriptor, SettingKind,
};

/// Claude Code is not yet trusted with the session's directory, so the project's own `allow`
/// rules are ignored until its trust prompt is answered. No `params`.
pub const WORKSPACE_UNTRUSTED: ProviderCode =
    ProviderCode::notice("provider.claude.workspace_untrusted");

impl AgentId {
    pub const CLAUDE: AgentId = AgentId(DESCRIPTOR.id);
}

/// Where Claude Code keeps its transcripts, one directory per working directory.
pub(crate) const CLAUDE_RECORDS_DIR: &str = "projects";

/// Claude Code: `~/.claude`, a session's transcript at
/// `projects/<working directory slug>/<id>.jsonl` with the directory `<id>` beside it holding its
/// subagents' transcripts and tool results, and its rewind checkpoints in `file-history/<id>`. No
/// sign-in file of it is read; the account's email is in `.claude.json`, which lies outside the
/// directory unless one is exported (`crate::adapter::claude::settings::global_config_file`).
pub(crate) const LAYOUT: AgentLayout = AgentLayout {
    config_dir_var: "CLAUDE_CONFIG_DIR",
    default_dir: ".claude",
    records_dir: CLAUDE_RECORDS_DIR,
    archived_records_dir: None,
    record_level: 2,
    search_depth: 2,
    title_index: None,
    title_summary: None,
};

/// Claude Code's user settings, inside its configuration directory.
pub(crate) const CLAUDE_SETTINGS_FILE: &str = "settings.json";

/// Claude Code's key bindings, inside its configuration directory.
pub(crate) const CLAUDE_KEY_BINDINGS: &str = "keybindings.json";

/// What follows the session id in the name of Claude Code's transcript.
pub(crate) const CLAUDE_TRANSCRIPT_SUFFIX: &str = ".jsonl";

/// Claude Code's rewind checkpoints, a directory per session id.
pub(crate) const CLAUDE_CHECKPOINTS_DIR: &str = "file-history";

/// Claude Code's prompt history, one line per prompt.
pub(crate) const CLAUDE_PROMPT_HISTORY: &str = "history.jsonl";

/// Where Claude Code caches the model catalogue of each account that has used this configuration
/// directory, inside it: one `<organization>-<account hash>-cc.json` file per account.
pub(crate) const CLAUDE_MODEL_CATALOGUE_DIR: &str = "cache/model-catalog";

pub(super) const DESCRIPTOR: ProviderDescriptor = ProviderDescriptor {
    id: "claude",
    label: "Claude Code",
    short_name: "Claude",
    install_url: "https://code.claude.com/docs/en/setup",
    binary: "claude",
    layout: &LAYOUT,
    detection: DetectionMarkers {
        own: &["CLAUDE.md", ".claude"],
        generic: &[],
    },
    // Claude Code reads only this spelling, not `AGENTS.md`.
    instruction_file: "CLAUDE.md",
    tool_name_prefix: "mcp__",
    ask_question_tool: Some("AskUserQuestion"),
    capabilities: Capabilities {
        preallocates_session_id: true,
        hook_script: false,
        status_line: true,
        reset_credits: false,
        per_session_trust: false,
        config_sharing: true,
        delivery: DeliveryQuirks {
            confirms_by_reported_prompt: true,
            holds_report_for_completion_notice: true,
            resume_instruction_as_launch_prompt: true,
        },
    },
    quota_windows: QuotaWindowNaming {
        kinds: &[
            QuotaWindowKind::Session,
            QuotaWindowKind::Weekly,
            QuotaWindowKind::WeeklyModel,
        ],
        session_window: SessionWindowName::Session,
        weekly_all_models: true,
    },
    model_display: ModelDisplay {
        family_version: true,
        id_prefix: Some("claude-"),
        aliases: &["fable", "opus", "sonnet", "haiku"],
        // The alias Claude Code accepted before its list stopped offering it.
        retired: &["best"],
    },
    compatibility_source: true,
    settings: &[SettingDescriptor {
        id: "approval_mode",
        kind: SettingKind::Choice {
            choices: &[
                SettingChoice {
                    value: "auto",
                    confirm: false,
                },
                SettingChoice {
                    value: "manual",
                    confirm: false,
                },
                // Every tool call runs without asking.
                SettingChoice {
                    value: "bypass",
                    confirm: true,
                },
            ],
        },
        // The choice only becomes a launch flag, and no file is written to enable it, so it travels
        // like any other setting; an agent never reads or changes it.
        exported: true,
        shown_to_agents: false,
        compatibility_with: None,
    }],
    codes: &[WORKSPACE_UNTRUSTED],
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_names_are_the_ones_claude_code_writes() {
        assert_eq!(AgentId::CLAUDE.descriptor().layout, &LAYOUT);
        assert_eq!(
            LAYOUT,
            AgentLayout {
                config_dir_var: "CLAUDE_CONFIG_DIR",
                default_dir: ".claude",
                records_dir: "projects",
                archived_records_dir: None,
                record_level: 2,
                search_depth: 2,
                title_index: None,
                title_summary: None,
            }
        );
        assert_eq!(CLAUDE_RECORDS_DIR, "projects");
        assert_eq!(CLAUDE_SETTINGS_FILE, "settings.json");
        assert_eq!(CLAUDE_KEY_BINDINGS, "keybindings.json");
        assert_eq!(CLAUDE_TRANSCRIPT_SUFFIX, ".jsonl");
        assert_eq!(CLAUDE_CHECKPOINTS_DIR, "file-history");
        assert_eq!(CLAUDE_PROMPT_HISTORY, "history.jsonl");
        assert_eq!(CLAUDE_MODEL_CATALOGUE_DIR, "cache/model-catalog");
    }

    /// Its records are matched by position (`RecordStore::indexed` on the last level of the
    /// walk), which is the record's own level only while the search goes exactly that deep.
    #[test]
    fn the_search_goes_exactly_as_deep_as_a_transcript_sits() {
        assert_eq!(LAYOUT.search_depth, LAYOUT.record_level);
    }

    #[test]
    fn its_codes_are_of_the_kind_they_are_used_with() {
        assert_eq!(
            WORKSPACE_UNTRUSTED.kind,
            super::super::ProviderCodeKind::Notice
        );
        assert_eq!(DESCRIPTOR.codes, [WORKSPACE_UNTRUSTED]);
    }
}
