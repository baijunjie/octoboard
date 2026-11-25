//! Pi's descriptor, with its on-disk layout. Its facts were measured on Pi 1.5.2
//! (`docs/agent-cli-reference.md`, section "Pi").

use super::layout::{AgentLayout, ConfigDirVarNames};
use super::{
    AgentId, Capabilities, DeliveryQuirks, DetectionMarkers, InteractionKinds, ModelDisplay,
    NativeMemory, ProviderDescriptor, QuotaWindowNaming, SessionWindowName,
};

impl AgentId {
    pub const PI: AgentId = AgentId(DESCRIPTOR.id);
}

/// Pi: `~/.pi/agent`, a session's record at `sessions/--<working directory>--/<timestamp>_<id>.jsonl`.
pub(crate) const LAYOUT: AgentLayout = AgentLayout {
    config_dir_var: "PI_CODING_AGENT_DIR",
    config_dir_var_names: ConfigDirVarNames::Directory,
    default_dir: ".pi/agent",
    records_dir: "sessions",
    archived_records_dir: None,
    record_level: 2,
    search_depth: 2,
    title_index: None,
    title_summary: None,
};

pub(super) const DESCRIPTOR: ProviderDescriptor = ProviderDescriptor {
    id: "pi",
    label: "Pi",
    short_name: "Pi",
    install_url: "https://github.com/earendil-works/pi",
    binary: "pi",
    layout: &LAYOUT,
    detection: DetectionMarkers {
        own: &[".pi"],
        generic: &["AGENTS.md", ".agents"],
    },
    // What Pi 1.5.2 sets for the commands it runs: the first two for every one, the rest for those
    // of its shell tool (`PI_SESSION_FILE` only for a session that is saved). It also sets
    // `PI_SKIP_VERSION_CHECK=1` for every one, which is not listed: that one is a setting of Pi's
    // a user may export themselves, and theirs has to reach the session.
    inherited_session_vars: &[
        "AI_AGENT",
        "PI_CODING_AGENT",
        "PI_SESSION_ID",
        "PI_SESSION_FILE",
        "PI_PROVIDER",
        "PI_MODEL",
        "PI_REASONING_LEVEL",
    ],
    command_environment: &[],
    instruction_file: "AGENTS.md",
    instruction_import_prefix: None,
    tool_name_prefix: "mcp__",
    tool_name_separator: "__",
    // Pi 1.5.2 offers no tool for asking the user; its help names `ask_question` only as an
    // example of a tool to exclude.
    ask_question_tool: None,
    quit_keys: None,
    capabilities: Capabilities {
        preallocates_session_id: true,
        // Octoboard's extension runs the session's hook script for the events it forwards.
        hook_script: true,
        status_line: false,
        reset_credits: false,
        native_app_scheme: None,
        per_session_trust: false,
        console_trust: super::ConsoleTrust::PerLaunch,
        config_sharing: true,
        owned_accounts: true,
        login_flow: true,
        mcp_inventory: true,
        // It runs each command in a process group of its own, which it kills as it exits, on a
        // quit and on SIGHUP or SIGTERM alike (1.5.2).
        ends_command_groups_on_exit: true,
        // Pi 1.5.2 asks for no approval and offers no tool for asking the user: neither kind
        // exists. A dialog a user's own extension draws, and a sign-in that waits, are no tool
        // call of either kind; Pi's `blocked` report raises the hand for them.
        interactions: InteractionKinds::NONE,
        // Pi sends a pasted image path as text; Octoboard's extension attaches the images
        // Octoboard pasted, which the model was measured to be given as images (1.5.2).
        message_images: true,
        // A slash command pasted into the box runs on Enter, and draws no completion (1.5.2).
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
    // Pi 1.5.2 keeps nothing of one session for another: a new session's first request holds
    // nothing of an earlier one, and the package has no memory feature.
    native_memory: NativeMemory::Absent,
    settings: &[],
    codes: &[],
};

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    #[test]
    fn the_layout_names_are_the_ones_pi_is_expected_to_use() {
        assert_eq!(AgentId::PI.descriptor().layout, &LAYOUT);
        assert_eq!(
            LAYOUT,
            AgentLayout {
                config_dir_var: "PI_CODING_AGENT_DIR",
                config_dir_var_names: ConfigDirVarNames::Directory,
                default_dir: ".pi/agent",
                records_dir: "sessions",
                archived_records_dir: None,
                record_level: 2,
                search_depth: 2,
                title_index: None,
                title_summary: None,
            }
        );
    }

    #[test]
    fn its_descriptor_names_pi_and_its_tools() {
        let descriptor = AgentId::PI.descriptor();
        assert_eq!(descriptor.id, "pi");
        assert_eq!(descriptor.label, "Pi");
        assert_eq!(descriptor.short_name, "Pi");
        assert_eq!(descriptor.binary, "pi");
        assert_eq!(descriptor.detection.own, &[".pi"]);
        assert_eq!(descriptor.detection.generic, &["AGENTS.md", ".agents"]);
        assert_eq!(descriptor.instruction_file, "AGENTS.md");
        assert_eq!(descriptor.tool_name_prefix, "mcp__");
        assert_eq!(descriptor.tool_name_separator, "__");
        assert_eq!(descriptor.ask_question_tool, None);
    }

    /// The variable names the configuration directory itself, whatever the directory is called.
    #[test]
    fn its_variable_names_the_directory_itself() {
        let layout = &LAYOUT;
        assert_eq!(
            layout.config_dir_export(Path::new("/accounts/work/anything")),
            Some((
                "PI_CODING_AGENT_DIR",
                PathBuf::from("/accounts/work/anything")
            ))
        );
        assert_eq!(
            layout.config_dir_of_export(Path::new("/accounts/work/anything")),
            PathBuf::from("/accounts/work/anything")
        );
    }

    /// Its capabilities are the measured ones: it pre-allocates the session id, runs the session's
    /// hook script, shares the default configuration, ends its commands' process groups on exit, takes
    /// images attached to a message, and runs a slash command written into its box. It offers
    /// xAI device sign-in and configured MCP inventory. It asks
    /// no tool approvals or questions, keeps no memory of its own, and has no quota windows,
    /// settings, codes or model naming of its own.
    #[test]
    fn it_declares_its_measured_capabilities_and_nothing_else() {
        let descriptor = AgentId::PI.descriptor();
        assert_eq!(
            descriptor.capabilities,
            Capabilities {
                preallocates_session_id: true,
                hook_script: true,
                status_line: false,
                reset_credits: false,
                native_app_scheme: None,
                per_session_trust: false,
                console_trust: super::super::ConsoleTrust::PerLaunch,
                config_sharing: true,
                owned_accounts: true,
                login_flow: true,
                mcp_inventory: true,
                ends_command_groups_on_exit: true,
                interactions: InteractionKinds::NONE,
                message_images: true,
                slash_commands: true,
                delivery: DeliveryQuirks {
                    confirms_by_reported_prompt: false,
                    holds_report_for_completion_notice: false,
                    resume_instruction_as_launch_prompt: false,
                },
            }
        );
        assert!(descriptor.quota_windows.kinds.is_empty());
        assert_eq!(descriptor.model_display, ModelDisplay::PLAIN);
        assert!(descriptor.settings.is_empty());
        assert!(descriptor.codes.is_empty());
        assert_eq!(
            descriptor.inherited_session_vars,
            &[
                "AI_AGENT",
                "PI_CODING_AGENT",
                "PI_SESSION_ID",
                "PI_SESSION_FILE",
                "PI_PROVIDER",
                "PI_MODEL",
                "PI_REASONING_LEVEL",
            ]
        );
        assert!(!descriptor.compatibility_source);
        assert_eq!(
            descriptor.native_memory,
            crate::providers::NativeMemory::Absent
        );
        assert!(descriptor.install_url.starts_with("https://"));
    }
}
