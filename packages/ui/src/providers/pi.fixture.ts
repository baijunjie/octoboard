import type { ProviderInfo } from "../protocol";

/** What the daemon serves for Pi (`apps/daemon/src/providers/pi.rs`), for the tests: a preview provider, which the
 * gallery and the fixture daemon do not serve until it is released (`fixtures/fixtureProviders.ts`). What it offers is
 * the descriptor's to say, and the fields below hold it. */
export const served: ProviderInfo = {
  id: "pi",
  label: "Pi",
  short_name: "Pi",
  install_url: "https://github.com/earendil-works/pi",
  default_config_dir: ".pi/agent",
  detection_markers: [".pi", "AGENTS.md", ".agents"],
  capabilities: {
    preallocates_session_id: true,
    hook_script: true,
    status_line: false,
    reset_credits: false,
    native_app_scheme: null,
    per_session_trust: false,
    config_sharing: true,
    owned_accounts: true,
    login_flow: true,
    mcp_inventory: true,
    ends_command_groups_on_exit: true,
    interactions: { permission: false, question: false },
    message_images: true,
    slash_commands: true,
    delivery: { confirms_by_reported_prompt: false, holds_report_for_completion_notice: false, resume_instruction_as_launch_prompt: false },
  },
  quota_windows: { kinds: [], session_window: "session", weekly_all_models: false },
  model_display: { family_version: false, id_prefix: null, aliases: [], retired: [] },
  native_memory: "absent",
  settings: [],
};
