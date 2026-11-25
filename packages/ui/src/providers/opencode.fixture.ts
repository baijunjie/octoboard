import type { ProviderInfo } from "../protocol";

/** What the daemon serves for OpenCode (`apps/daemon/src/providers/opencode.rs`), for the tests: a preview provider whose plugin runs
 * the hook script, whose permission prompts and questions are answered from Octoboard, which takes images
 * and slash commands in a message, confirms a delivery by the prompt it reports, and ends its command groups
 * when it exits; with no quota window, no login flow, and settings for approval and Claude compatibility. The gallery
 * and the fixture daemon do not serve it until it is released (`fixtures/fixtureProviders.ts`). */
export const served: ProviderInfo = {
  id: "opencode",
  label: "OpenCode",
  short_name: "OpenCode",
  install_url: "https://opencode.ai",
  default_config_dir: ".local/share/opencode",
  detection_markers: [".opencode", "opencode.json", "opencode.jsonc", "AGENTS.md"],
  capabilities: {
    preallocates_session_id: false,
    hook_script: true,
    status_line: false,
    reset_credits: false,
    native_app_scheme: null,
    per_session_trust: false,
    config_sharing: false,
    owned_accounts: false,
    login_flow: false,
    mcp_inventory: true,
    ends_command_groups_on_exit: true,
    interactions: { permission: true, question: true },
    message_images: true,
    slash_commands: true,
    delivery: { confirms_by_reported_prompt: true, holds_report_for_completion_notice: false, resume_instruction_as_launch_prompt: false },
  },
  quota_windows: { kinds: [], session_window: "session", weekly_all_models: false },
  model_display: { family_version: false, id_prefix: null, aliases: [], retired: [] },
  native_memory: "absent",
  settings: [
    {
      id: "approval_mode",
      kind: { type: "choice", choices: [{ value: "ask", confirm: false }, { value: "auto", confirm: true }] },
      exported: true,
      shown_to_agents: false,
      compatibility_with: null,
    },
    { id: "claude_compatibility", kind: { type: "switch", default: "off" }, exported: true, shown_to_agents: true, compatibility_with: "claude" },
  ],
};
