//! A provider registered only in tests, declaring nothing beyond what every provider must: no
//! capability, no compatibility, one setting and no codes of its own. It proves that a provider
//! is added by its own modules and a line in each registry: every test that iterates the
//! registries runs it through the defaults a provider without anything of its own gets.

use super::layout::AgentLayout;
use super::{
    AgentId, Capabilities, DeliveryQuirks, DetectionMarkers, ModelDisplay, ProviderDescriptor,
    QuotaWindowKind, QuotaWindowNaming, SessionWindowName, SettingChoice, SettingDescriptor,
    SettingKind,
};

impl AgentId {
    pub const TEST: AgentId = AgentId(DESCRIPTOR.id);
}

const LAYOUT: AgentLayout = AgentLayout {
    config_dir_var: "OCTOBOARD_TEST_AGENT_HOME",
    default_dir: ".octoboard-test-agent",
    records_dir: "sessions",
    archived_records_dir: None,
    record_level: 1,
    search_depth: 1,
    title_index: None,
    title_summary: None,
};

pub(super) const DESCRIPTOR: ProviderDescriptor = ProviderDescriptor {
    id: "test_agent",
    label: "Test Agent",
    short_name: "Test",
    install_url: "https://example.com/test-agent",
    binary: "octoboard-test-agent",
    layout: &LAYOUT,
    detection: DetectionMarkers {
        own: &[".test-agent"],
        generic: &[],
    },
    instruction_file: "TEST_AGENT.md",
    instruction_import_prefix: None,
    tool_name_prefix: "mcp__",
    ask_question_tool: None,
    quit_keys: None,
    capabilities: Capabilities {
        preallocates_session_id: false,
        hook_script: false,
        status_line: false,
        reset_credits: false,
        per_session_trust: false,
        config_sharing: false,
        mcp_inventory: false,
        delivery: DeliveryQuirks {
            confirms_by_reported_prompt: false,
            holds_report_for_completion_notice: false,
            resume_instruction_as_launch_prompt: false,
        },
    },
    quota_windows: QuotaWindowNaming {
        kinds: &[QuotaWindowKind::Session],
        session_window: SessionWindowName::Session,
        weekly_all_models: false,
    },
    model_display: ModelDisplay::PLAIN,
    compatibility_source: false,
    settings: &[SettingDescriptor {
        id: "mode",
        kind: SettingKind::Choice {
            choices: &[
                SettingChoice {
                    value: "careful",
                    confirm: false,
                },
                SettingChoice {
                    value: "reckless",
                    confirm: true,
                },
            ],
        },
        exported: false,
        shown_to_agents: false,
        compatibility_with: None,
    }],
    codes: &[],
};
