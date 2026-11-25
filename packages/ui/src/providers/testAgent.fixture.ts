import type { ProviderInfo } from "../protocol";

/** The test-only provider, as the daemon's registry describes it (`apps/daemon/src/providers/test_provider.rs`): no
 * capability but `owned_accounts`, no compatibility, no quota window, no login flow, and one choice setting, `mode`,
 * that is neither exported nor shown to agents (`reckless` asks for confirmation). Its presentation module is
 * registered under the test runner (`providers/registry.ts`). */
export const served: ProviderInfo = {
  id: "test_agent",
  label: "Test Agent",
  short_name: "Test",
  install_url: "https://example.com/test-agent",
  default_config_dir: ".octoboard-test-agent",
  detection_markers: [".test-agent"],
  capabilities: {
    preallocates_session_id: false,
    hook_script: false,
    status_line: false,
    reset_credits: false,
    native_app_scheme: null,
    per_session_trust: false,
    config_sharing: false,
    owned_accounts: true,
    login_flow: false,
    mcp_inventory: false,
    ends_command_groups_on_exit: false,
    interactions: { permission: false, question: false },
    message_images: false,
    slash_commands: false,
    delivery: { confirms_by_reported_prompt: false, holds_report_for_completion_notice: false, resume_instruction_as_launch_prompt: false },
  },
  quota_windows: { kinds: [], session_window: "session", weekly_all_models: false },
  model_display: { family_version: false, id_prefix: null, aliases: [], retired: [] },
  native_memory: "absent",
  settings: [
    { id: "mode", kind: { type: "choice", choices: [{ value: "careful", confirm: false }, { value: "reckless", confirm: true }] }, exported: false, shown_to_agents: false, compatibility_with: null },
  ],
};
