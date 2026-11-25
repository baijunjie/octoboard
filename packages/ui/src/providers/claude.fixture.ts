import type { ProviderInfo } from "../protocol";

/** What the daemon serves for Claude Code (`apps/daemon/src/providers/claude.rs`), for the fixture daemon and the tests.
 * The permission approval is a choice an export carries and no agent reads; Codex compatibility is a switch, off until
 * chosen. */
export const served: ProviderInfo = {
  id: "claude",
  label: "Claude Code",
  short_name: "Claude",
  install_url: "https://code.claude.com/docs/en/setup",
  default_config_dir: ".claude",
  detection_markers: ["CLAUDE.md", ".claude"],
  capabilities: {
    preallocates_session_id: true,
    hook_script: false,
    status_line: true,
    reset_credits: false,
    native_app_scheme: "claude",
    per_session_trust: false,
    config_sharing: true,
    owned_accounts: true,
    login_flow: true,
    mcp_inventory: true,
    ends_command_groups_on_exit: true,
    interactions: { permission: true, question: true },
    message_images: true,
    slash_commands: true,
    delivery: { confirms_by_reported_prompt: true, holds_report_for_completion_notice: true, resume_instruction_as_launch_prompt: true },
  },
  quota_windows: { kinds: ["session", "weekly", "weekly_model"], session_window: "session", weekly_all_models: true },
  model_display: { family_version: true, id_prefix: "claude-", aliases: ["fable", "opus", "sonnet", "haiku"], retired: ["best"] },
  native_memory: "switchable",
  settings: [
    { id: "approval_mode", kind: { type: "choice", choices: [{ value: "auto", confirm: false }, { value: "manual", confirm: false }, { value: "accept_edits", confirm: false }, { value: "plan", confirm: false }, { value: "bypass", confirm: true }] }, exported: true, shown_to_agents: false, compatibility_with: null },
    { id: "codex_compatibility", kind: { type: "switch", default: "off" }, exported: true, shown_to_agents: true, compatibility_with: "codex" },
    { id: "simple_shell_commands", kind: { type: "switch", default: "off" }, exported: true, shown_to_agents: true, compatibility_with: null },
  ],
};
