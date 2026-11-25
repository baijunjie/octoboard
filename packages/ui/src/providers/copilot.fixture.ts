import type { ProviderInfo } from "../protocol";

/** What the daemon serves for GitHub Copilot CLI (`apps/daemon/src/providers/copilot.rs`), for the tests: a preview
 * provider with a pre-allocated session id, accounts of its own and a login flow, a monthly credit window, its permission
 * prompts and `ask_user` questions answered from the client, images in a message, its slash commands, and its
 * permission setting. The gallery and the fixture daemon do not serve it until it is released
 * (`fixtures/fixtureProviders.ts`). */
export const served: ProviderInfo = {
  id: "copilot",
  label: "GitHub Copilot CLI",
  short_name: "Copilot",
  install_url: "https://github.com/github/copilot-cli",
  default_config_dir: ".copilot",
  detection_markers: ["AGENTS.md"],
  capabilities: {
    preallocates_session_id: true,
    hook_script: false,
    status_line: false,
    reset_credits: false,
    native_app_scheme: null,
    per_session_trust: false,
    config_sharing: true,
    owned_accounts: true,
    login_flow: true,
    mcp_inventory: true,
    ends_command_groups_on_exit: true,
    interactions: { permission: true, question: true },
    message_images: true,
    slash_commands: true,
    delivery: { confirms_by_reported_prompt: false, holds_report_for_completion_notice: false, resume_instruction_as_launch_prompt: false },
  },
  quota_windows: { kinds: ["credit_period"], session_window: "session", weekly_all_models: false, credit_period: "plan" },
  model_display: { family_version: false, id_prefix: null, aliases: [], retired: [] },
  native_memory: "unswitchable",
  settings: [
    { id: "approval_mode", kind: { type: "choice", choices: [{ value: "allow_all", confirm: true }] }, exported: true, shown_to_agents: false, compatibility_with: null },
  ],
};
