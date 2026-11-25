import type { ProviderInfo } from "../protocol";

/** What the daemon serves for Grok Build (`apps/daemon/src/providers/grok.rs`), for the fixture daemon and the tests.
 * The permission approval is a choice an export carries and no agent reads; Claude and Codex compatibility are switches,
 * off until chosen. */
export const served: ProviderInfo = {
  id: "grok",
  label: "Grok Build",
  short_name: "Grok",
  install_url: "https://docs.x.ai/build/overview",
  default_config_dir: ".grok",
  detection_markers: [".grok", "AGENTS.md", ".agents"],
  capabilities: {
    preallocates_session_id: true,
    hook_script: true,
    status_line: false,
    reset_credits: false,
    native_app_scheme: null,
    per_session_trust: true,
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
  quota_windows: { kinds: ["credit_period"], session_window: "five_hour", weekly_all_models: false },
  model_display: { family_version: false, id_prefix: null, aliases: [], retired: [] },
  native_memory: "switchable",
  settings: [
    { id: "approval_mode", kind: { type: "choice", choices: [{ value: "ask", confirm: false }, { value: "auto", confirm: false }, { value: "always_approve", confirm: true }] }, exported: true, shown_to_agents: false, compatibility_with: null },
    { id: "claude_compatibility", kind: { type: "switch", default: "off" }, exported: true, shown_to_agents: true, compatibility_with: "claude" },
    { id: "codex_compatibility", kind: { type: "switch", default: "off" }, exported: true, shown_to_agents: true, compatibility_with: "codex" },
  ],
};
