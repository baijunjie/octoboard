import { DEFAULT_GIT_WORKFLOW } from "./gitWorkflow";
import type { Settings } from "./protocol";

/** What the store holds before the first settings snapshot arrives, and what a hand-built state starts from. */
export const DEFAULT_SETTINGS: Settings = {
  auto_pull_repositories: false,
  auto_delete_unresumable_archives: true,
  auto_approve_session_requests: false,
  auto_approve_resume_requests: false,
  auto_approve_account_switches: false,
  auto_approve_handoffs: false,
  ask_questions_as_choices: true,
  interface_language: null,
  auto_select_account: false,
  auto_switch_on_usage_limit: false,
  sidebar_order_by_status: true,
  auto_start_quota_windows: false,
  default_clone_dir: "",
  accounts: [],
  account_order: {},
  added_agents: null,
  codex_approval_reviewer: null,
  claude_approval_mode: null,
  claude_compatibility: {},
  default_account_names: {},
  disabled_default_accounts: [],
  git_workflow: DEFAULT_GIT_WORKFLOW,
};
