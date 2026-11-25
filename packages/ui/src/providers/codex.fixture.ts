import type { ProviderInfo } from "../protocol";

/** What the daemon serves for Codex (`apps/daemon/src/providers/codex.rs`), for the fixture daemon and the tests. The
 * permission approval is a choice an export carries and no agent reads; Claude compatibility is a switch, off until
 * chosen. */
export const served: ProviderInfo = {
  id: "codex",
  label: "Codex",
  short_name: "Codex",
  install_url: "https://github.com/openai/codex",
  default_config_dir: ".codex",
  detection_markers: [".codex", "AGENTS.md", ".agents"],
  capabilities: {
    preallocates_session_id: false,
    hook_script: true,
    status_line: false,
    reset_credits: true,
    native_app_scheme: "codex",
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
  quota_windows: { kinds: ["session", "weekly"], session_window: "five_hour", weekly_all_models: false },
  model_display: { family_version: false, id_prefix: null, aliases: [], retired: [] },
  native_memory: "switchable",
  settings: [
    { id: "approval_reviewer", kind: { type: "choice", choices: [{ value: "user", confirm: false }, { value: "auto_review", confirm: false }, { value: "full_access", confirm: true }] }, exported: true, shown_to_agents: false, compatibility_with: null },
    { id: "claude_compatibility", kind: { type: "switch", default: "off" }, exported: true, shown_to_agents: true, compatibility_with: "claude" },
  ],
};
