import { isProjectFolderUnknown, pathLinkRefusedCause, relocationCause, sourceUnavailableCause, type PathLinkRefusedCause, type RelocationCause, type SourceUnavailableCause, NESTED_REMEDY_CODES } from "./protocol";
import { ActionableError } from "./actionableError";
import { DaemonRequestError } from "./daemon-client";
import { isUncataloguedFailure } from "./uncataloguedFailure";

export type ErrorRecovery =
  | "diskSpace" | "access" | "connection" | "path" | "account" | "tool" | "shell"
  | "unlock" | "stopSessions" | "wait" | "trust" | "conversation" | "session" | "ownerGone"
  | "configuration" | "exportSelection" | "image" | "avatarSize" | "avatarImage" | "clipboard" | "fileRead" | "fileWrite" | "gitFailure" | "gitSwitchFailure" | "gitRestoreFailure" | "gitHistory" | "branch"
  | "refresh" | "scope" | "limit" | "openExternal" | "openTerminal" | "openFileManager" | "openDefaultApplication" | "issueAddress"
  | "hostPathGone" | "readAccess" | "sourceGone" | "sourceLink" | "sourceNotFolder" | "sourceUnreadable" | "pathExists" | "uniqueName" | "launchSetting" | "changeModelSetting" | "modelChange" | "accountDirectory" | "repository" | "trash" | "noCommits" | "remote" | "stopProcess" | "existingProject"
  | "sessionBranches" | "projectBranches" | "lostMessages" | "sharingLostMessages" | "unsentMessage" | "submitRetry" | "unresumable" | "unarchive" | "availability" | "addedAgents" | "agentNotAdded" | "quotaUsedUp" | "accountDisabled" | "accountNotSignedIn" | "grokSandbox"
  | "pathLinkAmbiguous" | "pathLinkLaunchable" | "pathLinkNotPrivate"
  | "fileBusy" | "codexConfig" | "sqliteHome" | "symlink" | "notRestored" | "notRestoredAutomatic"
  | "accountNotShareable" | "accountSharing" | "accountBusy" | "accountChanging" | "sharingStop" | "ownerChanging" | "ownerMoved"
  | "diskSpaceNoRetry" | "accessNoRetry"
  | "none";

/** Removing an account with its directory, and a login refused because the account is being removed: the alert says
 * only why it was refused or failed, never what to do. */
const CAUSE_ONLY = new Set(["account_directory_trash_failed", "account_directory_shared", "account_directory_protected", "account_has_running_sessions", "account_being_removed"]);

const GROUPS: Partial<Record<ErrorRecovery, readonly string[]>> = {
  path: ["path_not_absolute", "path_not_found", "path_not_a_directory", "invalid_path", "directory_unreachable", "file_not_found", "config_dir_not_absolute", "config_dir_unreachable", "no_repositories_found", "repository_name_missing"],
  // The project's directory, or its place in a worktree, that a request is scoped to: nothing the user typed or picked
  // with Browse, so it is put back rather than corrected. Its `cause` picks a closer remedy (`SOURCE_CAUSE_RECOVERY`).
  sourceGone: ["source_unavailable"],
  repository: ["not_a_git_repository"],
  trash: ["project_directory_trash_failed", "project_directory_shared", "project_directory_protected"],
  stopProcess: ["session_did_not_stop", "model_change_did_not_stop"],
  existingProject: ["all_projects_already_added"],
  pathExists: ["path_already_exists"],
  uniqueName: ["account_name_taken"],
  launchSetting: ["invalid_launch_setting"],
  // A relaunch that ends at once with a new model most likely does not take that model, and leaves the session not
  // running.
  modelChange: ["model_change_did_not_come_up"],
  accountDirectory: ["config_dir_taken"],
  access: ["directory_unreadable"],
  // A read of the project's files the system denied, where there is no folder to choose instead and nothing is written.
  readAccess: ["permission_denied"],
  connection: ["git_remote_unreachable", "connection_closed", "request_timed_out"],
  unlock: ["session_locked"],
  stopSessions: ["session_has_running_sessions", "console_has_running_sessions", "project_has_running_sessions", "session_not_deletable"],
  wait: ["session_lock_busy", "session_already_running", "session_already_starting", "session_working", "session_busy", "session_changing"],
  ownerChanging: ["session_owner_changing"],
  ownerMoved: ["session_binding_changed"],
  lostMessages: ["handoff_messages_dropped"],
  sharingLostMessages: ["sharing_messages_dropped"],
  unsentMessage: ["message_not_sent"],
  trust: ["trust_answer_failed", "session_trust_pending", "session_waiting_for_user", "trust_directory_too_broad", "trust_directory_unreachable", "trust_path_not_absolute", "trust_home_unknown"],
  tool: ["binary_not_found", "agent_not_available", "agent_not_installed", "git_unavailable"],
  availability: ["agent_availability_pending", "added_agents_undecided"],
  addedAgents: ["agent_already_added", "agent_order_invalid"],
  agentNotAdded: ["agent_not_added"],
  quotaUsedUp: ["account_quota_exhausted"],
  accountDisabled: ["account_disabled"],
  accountNotSignedIn: ["account_not_signed_in"],
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
  ownerGone: ["quick_create_owner_gone"],
  session: ["session_archived", "session_not_running", "session_already_on_account", "unknown_session", "session_request_not_waiting", "session_request_answered", "trust_not_waiting", "session_not_bound"],
  configuration: ["configuration_not_recognised", "configuration_version_unsupported", "configuration_unreadable", "import_choice_unavailable", "import_resolution_invalid", "import_settings_missing", "import_accounts_missing"],
  // The export dialog keeps Export unavailable while nothing is chosen, so only a client that does not guard it meets this.
  exportSelection: ["export_empty"],
  // The console dialog draws a picked image into a data URL scaled down to the limit, so only an imported configuration
  // carries an avatar that is not an image or is too large.
  avatarImage: ["icon_not_an_image"],
  avatarSize: ["icon_too_large"],
  remote: ["invalid_remote_url"],
  noCommits: ["no_commits_to_push", "no_commits_to_restore"],
  // Switching a branch and restoring files are local: their remedies do not point at remote access. A restore discards
  // uncommitted changes by design, so its remedy names what can really refuse it.
  gitSwitchFailure: ["git_switch_failed"],
  gitRestoreFailure: ["git_restore_failed"],
  gitFailure: ["git_failed", "git_clone_failed", "git_push_failed", "no_push_remote", "upstream_not_a_remote"],
  gitHistory: ["no_merge_base", "no_merge_base_shallow"],
  sessionBranches: ["session_branches_not_removed"],
  projectBranches: ["project_branches_not_determined"],
  branch: ["not_on_a_branch", "restore_not_on_branch", "invalid_branch_name", "unknown_branch", "worktree_unavailable"],
  refresh: ["account_order_invalid", "source_changed", "unknown_commit", "page_not_current", "unknown_page", "unknown_project", "unknown_console", "queued_messages_lost", "queued_messages_dropped"],
  scope: ["outside_scope", "unsupported_file_type", "clean_filtered"],
  limit: ["limit_exceeded"],
  // One group per application the host opens, so each remedy names only the one that failed. `openExternal` is the
  // browser link the UI itself fails to open, raised as an explicit recovery, so no code maps to it.
  openTerminal: ["terminal_unavailable"],
  openFileManager: ["directory_unavailable"],
  openDefaultApplication: ["path_open_failed"],
  // Expected failures whose sentence already says what follows and what to do, with no remedy of their kind to add.
  // `git_operation_in_progress` is only a restore refused before it starts, and says to finish or abort the operation.
  // `push_in_progress` is a push refused because the project's previous one is still running, and says so.
  // `console_session_never_bound` and `session_not_bound_to_owner` cannot come from the UI: the window sends
  // release_session without an owner, and offers Unbind only on a bound project session. `path_link_refused` is a path
  // link the daemon will not open; its `cause` picks the remedy (`PATH_LINK_REFUSED_RECOVERY`), which is none for the
  // causes whose sentence says it all.
  none: ["quick_create_unavailable", "daemon_shutting_down", "git_operation_in_progress", "push_in_progress", "path_link_refused", "sharing_resume_failed", "trust_not_carried_over", "account_sharing_repair_failed", "console_session_never_bound", "session_not_bound_to_owner"],
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

/** The remedy of a conversation move that failed for a cause the daemon names, as `relocationCause` reads it; `other`
 * names none. */
const RELOCATION_CAUSE_RECOVERY: Record<RelocationCause, ErrorRecovery | undefined> = {
  permission_denied: "access", busy: "fileBusy", no_space: "diskSpace", config_unreadable: "codexConfig",
  sqlite_home: "sqliteHome", symlink: "symlink", not_restored: "notRestored", other: undefined,
};

/** The remedy of a `source_unavailable` by its `cause`, as `sourceUnavailableCause` reads it. */
const SOURCE_CAUSE_RECOVERY: Record<SourceUnavailableCause, ErrorRecovery> = {
  missing: "sourceGone", link: "sourceLink", not_a_directory: "sourceNotFolder", other: "sourceUnreadable",
};

/** The remedy of a `path_link_refused` by its `cause`, as `pathLinkRefusedCause` reads it. */
const PATH_LINK_REFUSED_RECOVERY: Record<PathLinkRefusedCause, ErrorRecovery> = {
  ambiguous: "pathLinkAmbiguous", launchable: "pathLinkLaunchable", not_private: "pathLinkNotPrivate", not_a_file_or_folder: "none", unreadable: "none", other: "none",
};

/** Whether every item a `session_branches_not_removed` names is a worktree kept because whether it is or holds another
 * project's folder could not be told: their group's note says what to do with them, so the alert adds no remedy of its
 * own. A list that also names anything else keeps it. */
function onlyProjectFolderUnknown(error: DaemonRequestError): boolean {
  if (error.code !== "session_branches_not_removed") return false;
  try {
    const items: unknown = JSON.parse(error.params.items ?? "");
    return Array.isArray(items) && items.length > 0 && items.every((item) => !!item && typeof item === "object" && isProjectFolderUnknown(item as Parameters<typeof isProjectFolderUnknown>[0]));
  } catch {
    return false;
  }
}

/** The remedy for a failure of an operation with nothing left to retry, a session that could not be brought back up:
 * the recovery's variant without a step that retries it ("retry the operation", "choose another accessible folder"). */
function withoutRetry(recovery: ErrorRecovery | undefined): ErrorRecovery | undefined {
  return recovery === "diskSpace" ? "diskSpaceNoRetry" : recovery === "access" ? "accessNoRetry" : recovery;
}

/** A move whose older item could not be put back (`relocation_failed`'s `cause` `not_restored`). */
function isNotRestored(error: unknown): boolean {
  return error instanceof DaemonRequestError && error.code === "relocation_failed" && relocationCause(error.params.cause) === "not_restored";
}

/** The cause the system's own text names in a failure that states only its cause (`CAUSE_ONLY`: no remedy follows it),
 * so its alert can say the cause after the failure's own sentence; the raw text stays in the details. */
export function namedSystemCause(error: unknown): ErrorRecovery | undefined {
  return error instanceof DaemonRequestError && CAUSE_ONLY.has(error.code) ? environmentRecovery(error) : undefined;
}

/** The remedy that goes with a failure's own as well, said after it: what a recognised operating-system signature in
 * a failure that kept an older item aside asks for (free space, permissions), which `classifyError` leaves behind the
 * remedy that prevents losing that item. */
export function additionalRecovery(error: unknown): ErrorRecovery | undefined {
  if (isNotRestored(error)) return environmentRecovery(error);
  const cause = error instanceof DaemonRequestError && error.code === "account_switch_automatically_failed" ? failedSwitchCause(error) : undefined;
  return cause && isNotRestored(cause) ? withoutRetry(environmentRecovery(cause)) : undefined;
}

/** The failure a failed automatic switch reports, as a failure of its own (its `reason` as the message), or the
 * switch's own failure when it names none. */
function failedSwitchCause(error: DaemonRequestError): DaemonRequestError {
  const cause = error.params.reason_code;
  return cause && cause !== error.code ? new DaemonRequestError(error.params.reason ?? "", cause, error.params) : error;
}

export function classifyError(error: unknown, recovery?: ErrorRecovery): ErrorClassification {
  const explicit = recovery ?? (error instanceof ActionableError ? error.recovery : undefined);
  if (!explicit && error instanceof DaemonRequestError && error.code === "message_not_sent" && error.params.submit_retry === "once") {
    return { kind: "actionable", recovery: "submitRetry" };
  }
  // A file that could not be read keeps its own remedy over the one an operating-system signature names: the
  // signature's access and disk-space remedies are about writing, which reading a file never needed.
  if (explicit === "issueAddress" || explicit === "openExternal" || explicit === "fileRead") return { kind: "actionable", recovery: explicit };
  if (isNotRestored(error)) return { kind: "actionable", recovery: "notRestored" };
  // A denied read keeps its own remedy over the access remedy its operating-system signature names, which is about
  // writing and choosing another folder.
  if (!explicit && error instanceof DaemonRequestError && DOMAIN_RECOVERY.get(error.code) === "readAccess") return { kind: "actionable", recovery: "readAccess" };
  // A failed automatic switch has nothing left to retry, so none of its remedies says to: the session could not be
  // brought back up, and the nested cause's remedy (wait for a change, correct a path) is not for the person to act
  // on. The exceptions are an older item kept aside, which is lost unless it is copied out in time, and what the
  // system said (a full disk, a denied access). A cause the application has no words for stays unexpected, with
  // the guidance to report it, unless its raw text names what the system said, which the reason line already says.
  if (error instanceof DaemonRequestError && error.code === "account_switch_automatically_failed") {
    const nested = failedSwitchCause(error);
    if (nested === error) return { kind: "unexpected" };
    // Recognised as the cause names itself or as the system's text does, in lockstep with the generic sentence
    // `uncataloguedSummaryKey` says for it; otherwise it is unexpected.
    if (isUncataloguedFailure(nested)) {
      const classified = classifyError(nested);
      return classified.kind === "actionable" ? { kind: "actionable", recovery: withoutRetry(classified.recovery)! } : classified;
    }
    if (isNotRestored(nested)) return { kind: "actionable", recovery: "notRestoredAutomatic" };
    // The domain remedies (wait for a change, correct a path) are not for the person to act on; what the system
    // said (a full disk, a denied access) is.
    return { kind: "actionable", recovery: withoutRetry(environmentRecovery(nested)) ?? "none" };
  }
  // Ahead of the operating-system signature, whose remedy would otherwise be added.
  if (!explicit && error instanceof DaemonRequestError && CAUSE_ONLY.has(error.code)) return { kind: "actionable", recovery: "none" };
  const remedy = environmentRecovery(error) ?? explicit;
  if (remedy) return { kind: "actionable", recovery: remedy };
  if (error instanceof DaemonRequestError) {
    const code = NESTED_REMEDY_CODES.has(error.code) ? error.params.reason_code : error.code;
    if (code === "source_unavailable") return { kind: "actionable", recovery: SOURCE_CAUSE_RECOVERY[sourceUnavailableCause(error.params.cause)] };
    if (code === "path_link_refused") return { kind: "actionable", recovery: PATH_LINK_REFUSED_RECOVERY[pathLinkRefusedCause(error.params.cause)] };
    if (onlyProjectFolderUnknown(error)) return { kind: "actionable", recovery: "none" };
    const domain = code && DOMAIN_RECOVERY.get(code);
    if (domain) return { kind: "actionable", recovery: domain };
    const relocation = code === "relocation_failed" ? RELOCATION_CAUSE_RECOVERY[relocationCause(error.params.cause)] : undefined;
    if (relocation) return { kind: "actionable", recovery: relocation };
  }
  return { kind: "unexpected" };
}

/** The failures whose answer is the state of what was asked about — branches with no shared history,
 * not a repository, outside the project, too large, not a valid path — so asking again gives the same
 * answer and no Try again is offered. */
const STATE_IS_THE_ANSWER = new Set(["gitHistory", "repository", "scope", "limit"].flatMap((recovery) => GROUPS[recovery as ErrorRecovery] ?? []).concat("invalid_path"));

/** Whether asking again can change the answer to `error`: false for a failure that is the state of the
 * thing asked about (see `STATE_IS_THE_ANSWER`), where a Try again would only fail the same way. */
export function retryCanHelp(error: unknown): boolean {
  return !(error instanceof DaemonRequestError && STATE_IS_THE_ANSWER.has(error.code));
}

/** The codes of a file or folder in a project that is not there any more, a folder that has become a file included. */
const GONE = new Set(["file_not_found", "path_not_found", "path_not_a_directory"]);

/** The remedy for a file or folder an action on the project's files no longer finds: it is found again in the project's
 * files, not by the general remedy of its codes, which picks one with Browse in a form. Undefined for any other failure,
 * which keeps its own. */
export function hostPathRecovery(error: unknown): ErrorRecovery | undefined {
  return error instanceof DaemonRequestError && GONE.has(error.code) ? "hostPathGone" : undefined;
}

/** The remedy for a failed Change model whose model or reasoning effort the agent does not offer: Change model is where
 * to pick another, not the launch settings of a new session that the code's own remedy points to. Undefined for any
 * other failure, which keeps its own. */
export function modelChangeRecovery(error: unknown): ErrorRecovery | undefined {
  return error instanceof DaemonRequestError && error.code === "invalid_launch_setting" ? "changeModelSetting" : undefined;
}

/** The remedy for a failed click on a path link, at either step of it (reading the file for the viewer, or opening the
 * path on the host): a project file or folder gone since the link was drawn is found again in the project's files, a
 * gone temporary one has nowhere to be found, and a path that leads outside what the step may read (`outside_scope`) is
 * no use to the browse views' `scope` remedy, so the sentence alone says what is wrong. Undefined for any other
 * failure, which keeps its own. */
export function pathLinkRecovery(error: unknown): ErrorRecovery | undefined {
  if (!(error instanceof DaemonRequestError)) return undefined;
  if (error.code === "outside_scope") return "none";
  const recovery = hostPathRecovery(error);
  return recovery && error.params.root === "temporary" ? "none" : recovery;
}
