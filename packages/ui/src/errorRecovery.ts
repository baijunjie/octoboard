import type { MessageParams } from "./protocol";
import { DaemonRequestError } from "./daemon-client";

export type ErrorRecovery =
  | "diskSpace" | "access" | "connection" | "path" | "account" | "tool" | "shell"
  | "unlock" | "stopSessions" | "wait" | "trust" | "conversation" | "session"
  | "configuration" | "image" | "clipboard" | "file" | "git" | "gitHistory" | "branch"
  | "refresh" | "scope" | "limit" | "openExternal" | "issueLink"
  | "contentSyntax" | "pathExists" | "uniqueName" | "launchSetting" | "accountDirectory" | "repository" | "trash" | "noCommits" | "remote" | "stopProcess" | "existingProject"
  | "sessionBranches" | "projectBranches" | "lostMessages" | "sharingLostMessages" | "unsentMessage" | "unresumable" | "unarchive" | "availability" | "addedAgents" | "agentNotAdded" | "quotaUsedUp" | "quotaUsedUpAgent" | "grokSandbox"
  | "accountNotShareable" | "accountSharing" | "accountBusy" | "accountChanging" | "sharingStop"
  | "none";

/** A local failure whose user remedy is known without inspecting its translated message. */
export class ActionableError extends DaemonRequestError {
  constructor(message: string, readonly recovery: ErrorRecovery, options?: ErrorOptions & { code?: string; params?: MessageParams }) {
    super(message, options?.code ?? "ui_actionable", options?.params ?? {});
    this.cause = options?.cause;
  }
}

const GROUPS: Partial<Record<ErrorRecovery, readonly string[]>> = {
  path: ["path_not_absolute", "path_not_found", "path_not_a_directory", "invalid_path", "directory_unreachable", "source_unavailable", "file_not_found", "config_dir_not_absolute", "config_dir_unreachable", "no_repositories_found", "repository_name_missing"],
  repository: ["not_a_git_repository"],
  trash: ["project_directory_trash_failed", "project_directory_shared", "project_directory_protected"],
  stopProcess: ["session_did_not_stop"],
  existingProject: ["all_projects_already_added"],
  pathExists: ["path_already_exists"],
  uniqueName: ["account_name_taken"],
  launchSetting: ["invalid_launch_setting"],
  accountDirectory: ["config_dir_taken"],
  access: ["permission_denied", "directory_unreadable"],
  connection: ["git_remote_unreachable", "connection_closed", "request_timed_out"],
  unlock: ["session_locked"],
  stopSessions: ["session_has_running_sessions", "console_has_running_sessions", "project_has_running_sessions", "session_not_deletable"],
  wait: ["session_lock_busy", "session_already_running", "session_already_starting", "session_working"],
  lostMessages: ["handoff_messages_dropped"],
  sharingLostMessages: ["sharing_messages_dropped"],
  unsentMessage: ["message_not_sent"],
  trust: ["trust_answer_failed", "session_trust_pending", "session_waiting_for_user", "trust_directory_too_broad", "trust_path_not_absolute", "trust_home_unknown"],
  tool: ["binary_not_found", "agent_not_available", "agent_not_installed", "git_unavailable"],
  availability: ["agent_availability_pending", "added_agents_undecided"],
  addedAgents: ["agent_already_added", "agent_order_invalid"],
  agentNotAdded: ["agent_not_added"],
  quotaUsedUp: ["account_quota_exhausted"],
  shell: ["shell_environment_timeout"],
  account: ["unknown_account", "grok_home_not_initialized"],
  grokSandbox: ["grok_sandbox_unsupported"],
  accountNotShareable: ["default_account_not_shareable"],
  accountSharing: ["account_sharing_failed"],
  accountBusy: ["account_still_sharing", "account_in_use"],
  accountChanging: ["account_sharing_in_progress"],
  sharingStop: ["sharing_session_did_not_stop"],
  unresumable: ["conversation_unresumable"],
  unarchive: ["codex_unarchive_failed"],
  conversation: ["conversation_not_found", "conversation_to_resume_not_found", "resume_id_invalid", "conversation_outside_project", "conversation_in_session"],
  session: ["session_archived", "session_not_running", "session_already_on_account", "unknown_session", "session_request_not_waiting", "session_request_answered", "trust_not_waiting"],
  configuration: ["configuration_not_recognised", "configuration_version_unsupported", "configuration_unreadable", "import_choice_unavailable"],
  image: ["icon_not_an_image", "icon_too_large"],
  remote: ["invalid_remote_url"],
  noCommits: ["no_commits_to_push", "no_commits_to_restore"],
  git: ["git_clone_failed", "git_push_failed", "git_switch_failed", "git_restore_failed", "git_operation_in_progress", "no_push_remote", "upstream_not_a_remote"],
  gitHistory: ["no_merge_base", "no_merge_base_shallow"],
  sessionBranches: ["session_branches_not_removed"],
  projectBranches: ["project_branches_not_determined"],
  branch: ["not_on_a_branch", "restore_not_on_branch", "invalid_branch_name", "unknown_branch", "worktree_unavailable"],
  refresh: ["source_changed", "unknown_commit", "page_not_current", "unknown_page", "unknown_project", "unknown_console", "queued_messages_lost", "queued_messages_dropped"],
  scope: ["outside_scope", "unsupported_file_type", "clean_filtered"],
  limit: ["limit_exceeded"],
  openExternal: ["terminal_unavailable", "directory_unavailable", "path_open_failed"],
};
const DOMAIN_RECOVERY = new Map(Object.entries(GROUPS).flatMap(([recovery, codes]) => codes.map((code) => [code, recovery as ErrorRecovery] as const)));
const OS_RECOVERY: Record<string, ErrorRecovery> = {
  ENOSPC: "diskSpace", EDQUOT: "diskSpace", EACCES: "access", EPERM: "access", EROFS: "access",
  ECONNREFUSED: "connection", ECONNRESET: "connection", ENETUNREACH: "connection", ETIMEDOUT: "connection",
};
const ERRNO_RECOVERY: Record<string, ErrorRecovery> = { "28": "diskSpace", "13": "access", "1": "access", "30": "access" };

function environmentRecovery(error: unknown, seen = new Set<object>()): ErrorRecovery | undefined {
  if (error && typeof error === "object") {
    if (seen.has(error)) return undefined;
    seen.add(error);
    const record = error as { code?: unknown; errno?: unknown; cause?: unknown; params?: Record<string, string>; message?: unknown };
    const coded = typeof record.code === "string" ? OS_RECOVERY[record.code] : undefined;
    const errno = typeof record.errno === "number" ? ERRNO_RECOVERY[String(Math.abs(record.errno))] : undefined;
    const paramErrno = record.params?.errno ? ERRNO_RECOVERY[record.params.errno] : undefined;
    if (coded || errno || paramErrno) return coded ?? errno ?? paramErrno;
    const caused = environmentRecovery(record.cause, seen);
    if (caused) return caused;
    for (const value of [record.params?.detail, record.params?.reason, record.message]) {
      if (typeof value !== "string") continue;
      const signature = /\(os error (\d+)\)/.exec(value);
      if (signature && ERRNO_RECOVERY[signature[1]]) return ERRNO_RECOVERY[signature[1]];
    }
  } else if (typeof error === "string") {
    const signature = /\(os error (\d+)\)/.exec(error);
    if (signature) return ERRNO_RECOVERY[signature[1]];
  }
  return undefined;
}

export type ErrorClassification = { kind: "actionable"; recovery: ErrorRecovery } | { kind: "unexpected" };

export function classifyError(error: unknown, recovery?: ErrorRecovery): ErrorClassification {
  const explicit = recovery ?? (error instanceof ActionableError ? error.recovery : undefined);
  if (explicit === "issueLink" || explicit === "openExternal") return { kind: "actionable", recovery: explicit };
  const remedy = environmentRecovery(error) ?? explicit;
  if (remedy) return { kind: "actionable", recovery: remedy };
  if (error instanceof DaemonRequestError) {
    const code = error.code === "launch_failed" ? error.params.reason_code : error.code;
    const domain = code && DOMAIN_RECOVERY.get(code);
    if (domain) return { kind: "actionable", recovery: domain };
  }
  return { kind: "unexpected" };
}
