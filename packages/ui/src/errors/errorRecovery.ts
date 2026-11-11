import { isKeptForItsFolder } from "./branchCleanupItems";
import { pathLinkRefusedCause, relocationCause, sourceUnavailableCause, storeWriteCause, type PathLinkRefusedCause, type RelocationCause, type SourceUnavailableCause, type StoreWriteCause } from "./causes";
import { ActionableError } from "./actionableError";
import { CODE, codeRecord } from "./codes";
import { CodedError } from "./codedError";
import { messageCode } from "./daemonMessage";
import { isUncataloguedFailure } from "./uncataloguedFailure";

/** A remedy a provider's presentation module words itself, for a failure of its own: the key of the module's message
 * (`provider.<id>.<code>.remedy`, `<code>` being a code's own name), whose `{agent}` is the provider's name. */
export type ProviderRemedy = `provider.${string}.${string}.remedy`;

export type ErrorRecovery =
  | ProviderRemedy
  | "diskSpace" | "access" | "connection" | "path" | "account" | "tool" | "installGit" | "installGitNoRetry" | "shell"
  | "unlock" | "stopSessions" | "wait" | "trust" | "conversation" | "session" | "ownerGone"
  | "configuration" | "exportSelection" | "image" | "avatarSize" | "avatarImage" | "clipboard" | "clipboardCopy" | "fileRead" | "fileWrite" | "gitFailure" | "gitSwitchFailure" | "gitRestoreFailure" | "gitHistory" | "branch"
  | "refresh" | "refreshMessages" | "scope" | "limit" | "openExternal" | "openTerminal" | "openFileManager" | "openDefaultApplication" | "issueAddress"
  | "hostPathGone" | "readAccess" | "sourceGone" | "sourceLink" | "sourceNotFolder" | "sourceUnreadable" | "pathExists" | "uniqueName" | "launchSetting" | "changeModelSetting" | "modelChange" | "switchRelaunch" | "accountDirectory" | "repository" | "trash" | "noCommits" | "remote" | "stopProcess" | "existingProject"
  | "sessionBranches" | "projectBranches" | "lostMessages" | "sharingLostMessages" | "unsentMessage" | "submitRetry" | "unresumable" | "resetCredit" | "resetCreditLogin" | "availability" | "addedAgents" | "agentNotAdded" | "quotaUsedUp" | "accountDisabled" | "accountNotSignedIn"
  | "pathLinkAmbiguous" | "pathLinkLaunchable" | "pathLinkNotPrivate"
  | "mcpSignIn" | "mcpSignInTimedOut" | "mcpSignOut"
  | "fileBusy" | "codexConfig" | "sqliteHome" | "symlink" | "notRestored" | "notRestoredAutomatic"
  | "accountNotShareable" | "accountSharing" | "accountBusy" | "accountChanging" | "sharingStop" | "ownerChanging" | "ownerMoved"
  | "diskSpaceNoRetry" | "accessNoRetry" | "appDataAccess" | "appDataDiskSpace" | "appDataAccessNoRetry" | "appDataDiskSpaceNoRetry" | "appDataOther" | "appDataOtherNoRetry" | "unlockOriginal" | "waitOriginal" | "stopSessionsNoRetry" | "archiveOriginal" | "handoffBothRunning"
  | "none";

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

/** The remedy of a `store_write_failed` by its `cause`, as `storeWriteCause` reads it. */
const STORE_WRITE_CAUSE_RECOVERY: Record<StoreWriteCause, ErrorRecovery> = {
  access: "appDataAccess", disk_full: "appDataDiskSpace", other: "appDataOther",
};

/** The remedy of a `path_link_refused` by its `cause`, as `pathLinkRefusedCause` reads it. */
const PATH_LINK_REFUSED_RECOVERY: Record<PathLinkRefusedCause, ErrorRecovery> = {
  ambiguous: "pathLinkAmbiguous", launchable: "pathLinkLaunchable", not_private: "pathLinkNotPrivate", not_a_file_or_folder: "none", unreadable: "none", other: "none",
};

/** Whether every item a `session_branches_not_removed` names is a worktree kept for what its folder is or may hold:
 * one of which whether it is or holds another project's folder could not be told, whose group's note says what to do
 * with it, or the home folder, the root of a disk or a folder holding Octoboard's data, which is to stay. The alert then
 * adds no remedy of its own, which would have the person remove them. A list that also names anything else keeps it. */
function onlyKeptForTheirFolder(error: CodedError): boolean {
  if (error.code !== CODE.session_branches_not_removed) return false;
  try {
    const items: unknown = JSON.parse(error.params.items ?? "");
    return Array.isArray(items) && items.length > 0
      && items.every((item) => !!item && typeof item === "object" && isKeptForItsFolder(item as Parameters<typeof isKeptForItsFolder>[0]));
  } catch {
    return false;
  }
}

/** The remedy for a failure of an operation with nothing left to retry, a session that could not be brought back up:
 * the recovery's variant without a step that retries it ("retry the operation", "choose another accessible folder"). */
function withoutRetry(recovery: ErrorRecovery | undefined): ErrorRecovery | undefined {
  if (recovery === "installGit") return "installGitNoRetry";
  if (recovery === "appDataAccess") return "appDataAccessNoRetry";
  if (recovery === "appDataDiskSpace") return "appDataDiskSpaceNoRetry";
  if (recovery === "appDataOther") return "appDataOtherNoRetry";
  return recovery === "diskSpace" ? "diskSpaceNoRetry" : recovery === "access" ? "accessNoRetry" : recovery;
}

/** Whether `error` is a handoff that failed once the receiving session was running and the work had moved
 * (`handoff_archive_failed`, `handoff_move_failed`). */
export function isHandoffAfterMove(error: unknown): error is CodedError {
  return error instanceof CodedError && (error.code === CODE.handoff_archive_failed || error.code === CODE.handoff_move_failed);
}

/** The remedies of a nested failure that name a session to unlock, wait for or archive, worded for a handoff that failed
 * after the work moved: the original is the one left to act on, and nothing is to be retried. */
const HANDOFF_ORIGINAL_RECOVERY: Partial<Record<ErrorRecovery, ErrorRecovery>> = { unlock: "unlockOriginal", wait: "waitOriginal", stopSessions: "stopSessionsNoRetry" };

/** A move whose older item could not be put back (`relocation_failed`'s `cause` `not_restored`). */
function isNotRestored(error: unknown): boolean {
  return error instanceof CodedError && error.code === CODE.relocation_failed && relocationCause(error.params.cause) === "not_restored";
}

/** The cause the system's own text names in a failure that states only its cause (`causeOnly`: no remedy follows it),
 * so its alert can say the cause after the failure's own sentence; the raw text stays in the details. */
export function namedSystemCause(error: unknown): ErrorRecovery | undefined {
  return error instanceof CodedError && codeRecord(error.code)?.causeOnly ? environmentRecovery(error) : undefined;
}

/** The remedy that goes with a failure's own as well, said after it: what a recognised operating-system signature in
 * a failure that kept an older item aside asks for (free space, permissions), which `classifyError` leaves behind the
 * remedy that prevents losing that item; and, for a handoff that failed after the work moved, what the failure left
 * behind (an original still open, or both sessions running), whatever the cause. */
export function additionalRecovery(error: unknown): ErrorRecovery | undefined {
  // What was left still has to be looked at once Git is there, which is the remedy the notice has when Git is.
  if (isProjectBranchesWithoutGit(error)) return "projectBranches";
  // Said whatever the failure's own remedy is, or whether it has one: the original is still open, and after a failed
  // move both sessions are running.
  if (error instanceof CodedError && error.code === CODE.handoff_archive_failed) return "archiveOriginal";
  if (error instanceof CodedError && error.code === CODE.handoff_move_failed) return "handoffBothRunning";
  if (isNotRestored(error)) return environmentRecovery(error);
  const cause = error instanceof CodedError && error.code === CODE.account_switch_automatically_failed ? nestedFailure(error) : undefined;
  return cause && isNotRestored(cause) ? withoutRetry(environmentRecovery(cause)) : undefined;
}

/** Whether `error` is a project's removal that could not tell what went along with its folder because the user's `git`
 * could not be found (`project_branches_not_determined` nesting `binary_not_found` for `git`). */
function isProjectBranchesWithoutGit(error: unknown): boolean {
  return error instanceof CodedError && error.code === CODE.project_branches_not_determined && messageCode(error.code, error.params) === "project_branches_not_determined.git";
}

/** The failure a failed automatic switch or a handoff that failed after the work moved reports, as a failure of its
 * own (its `reason` as the message), or its own failure when it names none. */
function nestedFailure(error: CodedError): CodedError {
  const cause = error.params.reason_code;
  return cause && cause !== error.code ? new CodedError(error.params.reason ?? "", cause, error.params) : error;
}

/** The code whose remedy `error` shows: the failure a `failureAndRemedy` notice nests, or its own. */
function remedyCode(error: CodedError): string | undefined {
  return codeRecord(error.code)?.nested === "failureAndRemedy" ? error.params.reason_code : error.code;
}

/** The remedy of a failed read or write of the application's own database by the daemon's `cause`, which names it whatever the
 * system's text in `detail` says (SQLite's text carries no operating-system signature to read), raised directly or as
 * the cause of a failure that nests one (`failureAndRemedy`). Undefined for any other failure. */
function storeWriteRecovery(error: unknown): ErrorRecovery | undefined {
  if (!(error instanceof CodedError)) return undefined;
  const code = remedyCode(error);
  return code === CODE.store_write_failed ? STORE_WRITE_CAUSE_RECOVERY[storeWriteCause(error.params.cause)] : undefined;
}

export function classifyError(error: unknown, recovery?: ErrorRecovery): ErrorClassification {
  // The work has moved and the receiving session is running, so nothing is left to retry: the remedy of the failure it
  // carries, without a step that retries (for the original session where it is the one to unlock or wait for), ahead of
  // every other, since a nested `store_write_failed` would otherwise give its retrying remedy. A failure it names no code
  // for stays unexpected.
  if (isHandoffAfterMove(error)) {
    const nested = nestedFailure(error);
    if (nested === error) return { kind: "unexpected" };
    const classified = classifyError(nested);
    if (classified.kind !== "actionable") return classified;
    const recovery = withoutRetry(classified.recovery)!;
    return { kind: "actionable", recovery: HANDOFF_ORIGINAL_RECOVERY[recovery] ?? recovery };
  }
  // Ahead of an explicit remedy and of the operating-system signature: the cause names the application's own data, which
  // the explicit remedies of a call site (access, disk space) would word for a folder the person chose.
  const stored = storeWriteRecovery(error);
  if (stored) return { kind: "actionable", recovery: stored };
  const explicit = recovery ?? (error instanceof ActionableError ? error.recovery : undefined);
  if (!explicit && error instanceof CodedError && error.code === CODE.message_not_sent && error.params.submit_retry === "once") {
    return { kind: "actionable", recovery: "submitRetry" };
  }
  // A file that could not be read keeps its own remedy over the one an operating-system signature names: the
  // signature's access and disk-space remedies are about writing, which reading a file never needed. The three remedies
  // for the application's own data (`appDataRecovery`) are outputs of that signature, so it cannot override them.
  if (explicit === "issueAddress" || explicit === "openExternal" || explicit === "fileRead" || explicit === "appDataAccess" || explicit === "appDataDiskSpace" || explicit === "appDataOther") return { kind: "actionable", recovery: explicit };
  if (isNotRestored(error)) return { kind: "actionable", recovery: "notRestored" };
  // A denied read keeps its own remedy over the access remedy its operating-system signature names, which is about
  // writing and choosing another folder.
  if (!explicit && error instanceof CodedError && codeRecord(error.code)?.recovery === "readAccess") return { kind: "actionable", recovery: "readAccess" };
  // A failed automatic switch has nothing left to retry, so none of its remedies says to: the session could not be
  // brought back up, and the nested cause's remedy (wait for a change, correct a path) is not for the person to act
  // on. The exceptions are an older item kept aside, which is lost unless it is copied out in time, and what the
  // system said (a full disk, a denied access). A cause the application has no words for stays unexpected, with
  // the guidance to report it, unless its raw text names what the system said, which the reason line already says.
  if (error instanceof CodedError && error.code === CODE.account_switch_automatically_failed) {
    const nested = nestedFailure(error);
    if (nested === error) return { kind: "unexpected" };
    // Recognised as the cause names itself or as the system's text does, in lockstep with the generic sentence
    // `uncataloguedSummaryKey` says for it; otherwise it is unexpected.
    if (isUncataloguedFailure(nested)) {
      const classified = classifyError(nested);
      return classified.kind === "actionable" ? { kind: "actionable", recovery: withoutRetry(classified.recovery)! } : classified;
    }
    if (isNotRestored(nested)) return { kind: "actionable", recovery: "notRestoredAutomatic" };
    // Octoboard's own data could not be written: the system said so, in words of the daemon's cause.
    const stored = storeWriteRecovery(nested);
    if (stored) return { kind: "actionable", recovery: withoutRetry(stored)! };
    // The domain remedies (wait for a change, correct a path) are not for the person to act on; what the system
    // said (a full disk, a denied access) is.
    return { kind: "actionable", recovery: withoutRetry(environmentRecovery(nested)) ?? "none" };
  }
  // Ahead of the operating-system signature, whose remedy would otherwise be added.
  if (!explicit && error instanceof CodedError && codeRecord(error.code)?.causeOnly) return { kind: "actionable", recovery: "none" };
  const remedy = environmentRecovery(error) ?? explicit;
  if (remedy) return { kind: "actionable", recovery: remedy };
  if (error instanceof CodedError) {
    const code = remedyCode(error);
    if (code === CODE.source_unavailable) return { kind: "actionable", recovery: SOURCE_CAUSE_RECOVERY[sourceUnavailableCause(error.params.cause)] };
    if (code === CODE.path_link_refused) return { kind: "actionable", recovery: PATH_LINK_REFUSED_RECOVERY[pathLinkRefusedCause(error.params.cause)] };
    if (onlyKeptForTheirFolder(error)) return { kind: "actionable", recovery: "none" };
    // Remedied as it is worded: the variant its record names for a missing `git`. A removal that could not tell what
    // went along with its folder for want of one has nothing to try again; what it left follows (`additionalRecovery`).
    if (code && messageCode(code, error.params) === "binary_not_found.git") return { kind: "actionable", recovery: "installGit" };
    if (isProjectBranchesWithoutGit(error)) return { kind: "actionable", recovery: "installGitNoRetry" };
    const domain = code ? codeRecord(code)?.recovery : undefined;
    if (domain) return { kind: "actionable", recovery: domain };
    const relocation = code === CODE.relocation_failed ? RELOCATION_CAUSE_RECOVERY[relocationCause(error.params.cause)] : undefined;
    if (relocation) return { kind: "actionable", recovery: relocation };
  }
  return { kind: "unexpected" };
}

/** The failures whose answer is the state of what was asked about — branches with no shared history,
 * not a repository, outside the project, too large, not a valid path or configuration document — so asking again gives the same
 * answer and no Try again is offered. Whether asking again can change the answer to `error`: false for such a
 * failure, where a Try again would only fail the same way. */
export function retryCanHelp(error: unknown): boolean {
  if (!(error instanceof CodedError)) return true;
  const recovery = codeRecord(error.code)?.recovery;
  return !(recovery === "gitHistory" || recovery === "repository" || recovery === "scope" || recovery === "limit" || recovery === "configuration" || error.code === CODE.invalid_path);
}

/** The remedy for a file or folder an action on the project's files no longer finds: it is found again in the project's
 * files, not by the general remedy of its codes, which picks one with Browse in a form. Undefined for any other failure,
 * which keeps its own. */
export function hostPathRecovery(error: unknown): ErrorRecovery | undefined {
  return error instanceof CodedError && codeRecord(error.code)?.hostPathGone ? "hostPathGone" : undefined;
}

/** The remedy for a failed read or write of the application's own data (a setting, the order of the agents): a
 * `store_write_failed`'s by its cause, or what an operating-system signature asks for there (access, disk space), which
 * is about the folder holding the application's data, not a folder the person chose or the project's. Undefined for any
 * other failure, which keeps its own. */
export function appDataRecovery(error: unknown): ErrorRecovery | undefined {
  const stored = storeWriteRecovery(error);
  if (stored) return stored;
  const recovery = environmentRecovery(error);
  return recovery === "access" ? "appDataAccess" : recovery === "diskSpace" ? "appDataDiskSpace" : undefined;
}

/** The remedy for a failed Change model whose model or reasoning effort the agent does not offer: Change model is where
 * to pick another, not the launch settings of a new session that the code's own remedy points to. Undefined for any
 * other failure, which keeps its own. */
export function modelChangeRecovery(error: unknown): ErrorRecovery | undefined {
  return error instanceof CodedError && error.code === CODE.invalid_launch_setting ? "changeModelSetting" : undefined;
}

/** The remedy for a failed click on a path link, at either step of it (reading the file for the viewer, or opening the
 * path on the host): a project file or folder gone since the link was drawn is found again in the project's files, a
 * gone temporary one has nowhere to be found, and a path that leads outside what the step may read (`outside_scope`) is
 * no use to the browse views' `scope` remedy, so the sentence alone says what is wrong. Undefined for any other
 * failure, which keeps its own. */
export function pathLinkRecovery(error: unknown): ErrorRecovery | undefined {
  if (!(error instanceof CodedError)) return undefined;
  if (error.code === CODE.outside_scope) return "none";
  const recovery = hostPathRecovery(error);
  return recovery && error.params.root === "temporary" ? "none" : recovery;
}
