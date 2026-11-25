import type { ProviderInfo } from "../protocol";

/** What the daemon serves for Antigravity CLI (`apps/daemon/src/providers/antigravity.rs`), for the tests: a preview
 * provider with the default account only, whose session id comes from the CLI, with weekly quotas per model group, MCP inventory and no login flow,
 * and its permission mode as its one setting. What is built: its permission prompts and its questions answered from the
 * app, images attached to a message, its slash commands, and the background work it leaves running (its commands,
 * timers, cron jobs and subagents). The gallery and the fixture daemon do not serve it until it is released
 * (`fixtures/fixtureProviders.ts`). */
export const served: ProviderInfo = {
  id: "antigravity",
  label: "Antigravity CLI",
  short_name: "Antigravity",
  install_url: "https://antigravity.google/product/antigravity-cli",
  default_config_dir: ".gemini",
  detection_markers: ["GEMINI.md", "AGENTS.md", ".agents"],
  capabilities: {
    preallocates_session_id: false,
    hook_script: false,
    status_line: false,
    reset_credits: false,
    native_app_scheme: null,
    per_session_trust: false,
    config_sharing: false,
    owned_accounts: false,
    login_flow: true,
    mcp_inventory: true,
    ends_command_groups_on_exit: false,
    interactions: { permission: true, question: true },
    message_images: true,
    slash_commands: true,
    delivery: { confirms_by_reported_prompt: false, holds_report_for_completion_notice: false, resume_instruction_as_launch_prompt: false },
  },
  quota_windows: { kinds: ["weekly_model"], session_window: "session", weekly_all_models: false },
  model_display: { family_version: false, id_prefix: null, aliases: [], retired: [] },
  native_memory: "absent",
  settings: [
    {
      id: "approval_mode",
      kind: { type: "choice", choices: [{ value: "accept_edits", confirm: false }, { value: "auto", confirm: false }, { value: "skip_permissions", confirm: true }] },
      exported: true,
      shown_to_agents: false,
      compatibility_with: null,
    },
  ],
};
