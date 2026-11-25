import type { ProviderInfo } from "../protocol";

/** What the daemon serves for Qwen Code (`apps/daemon/src/providers/qwen.rs`), for the tests: a preview provider, which the
 * gallery and the fixture daemon do not serve until it is released (`fixtures/fixtureProviders.ts`). What it offers is
 * the descriptor's to say, and the fields below hold it. */
export const served: ProviderInfo = {
  id: "qwen",
  label: "Qwen Code",
  short_name: "Qwen",
  install_url: "https://github.com/QwenLM/qwen-code",
  default_config_dir: ".qwen",
  detection_markers: [".qwen", "QWEN.md", "AGENTS.md", ".agents"],
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
  quota_windows: { kinds: [], session_window: "session", weekly_all_models: false },
  model_display: { family_version: false, id_prefix: null, aliases: [], retired: [] },
  native_memory: "switchable",
  settings: [
    { id: "approval_mode", kind: { type: "choice", choices: [{ value: "plan", confirm: false }, { value: "default", confirm: false }, { value: "auto_edit", confirm: false }, { value: "auto", confirm: false }, { value: "yolo", confirm: true }] }, exported: true, shown_to_agents: false, compatibility_with: null },
    { id: "claude_instructions", kind: { type: "switch", default: "off" }, exported: true, shown_to_agents: true, compatibility_with: "claude" },
  ],
};
