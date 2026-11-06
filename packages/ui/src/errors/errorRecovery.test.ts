// @vitest-environment jsdom
import { expect, it } from "vitest";

import { CodedError } from "./codedError";
import { errorAlerts } from "../components/alerts/errorAlerts";
import { errorPresentation } from "./errorPresentation";
import { additionalRecovery, appDataRecovery, classifyError, modelChangeRecovery, pathLinkRecovery } from "./errorRecovery";
import { ActionableError } from "./actionableError";
import { hasErrorRemedy } from "../components/alerts/errorPartPresence";

it.each([
  ["path_not_found", "path"], ["source_unavailable", "sourceGone"], ["path_already_exists", "pathExists"], ["session_locked", "unlock"],
  ["account_name_taken", "uniqueName"], ["invalid_launch_setting", "launchSetting"], ["model_change_did_not_come_up", "modelChange"], ["switch_did_not_come_up", "switchRelaunch"], ["model_change_did_not_stop", "stopProcess"],
  ["connection_closed", "connection"], ["not_a_git_repository", "repository"],
  ["conversation_unresumable", "unresumable"], ["provider.codex.unarchive_failed", "provider.codex.unarchive_failed.remedy"],
  ["session_branches_not_removed", "sessionBranches"], ["project_branches_not_determined", "projectBranches"],
  ["session_working", "wait"], ["session_busy", "wait"], ["session_changing", "wait"], ["session_owner_changing", "ownerChanging"], ["session_binding_changed", "ownerMoved"], ["session_not_bound", "session"], ["quick_create_owner_gone", "ownerGone"], ["icon_not_an_image", "avatarImage"], ["icon_too_large", "avatarSize"], ["console_session_never_bound", "none"], ["session_not_bound_to_owner", "none"], ["handoff_messages_dropped", "lostMessages"], ["sharing_messages_dropped", "sharingLostMessages"], ["message_not_sent", "unsentMessage"],
  ["agent_not_installed", "tool"], ["agent_availability_pending", "availability"], ["added_agents_undecided", "availability"],
  ["account_order_invalid", "refresh"], ["unknown_shell_terminal", "refresh"], ["queued_messages_lost", "refreshMessages"], ["queued_messages_dropped", "refreshMessages"], ["agent_already_added", "addedAgents"], ["agent_order_invalid", "addedAgents"], ["agent_not_added", "agentNotAdded"],
  ["provider.grok.sandbox_unsupported", "provider.grok.sandbox_unsupported.remedy"],
  ["export_empty", "exportSelection"], ["import_accounts_missing", "configuration"], ["import_resolution_invalid", "configuration"], ["import_settings_missing", "configuration"],
  ["default_account_not_shareable", "accountNotShareable"], ["account_sharing_failed", "accountSharing"], ["account_still_sharing", "accountBusy"],
  ["account_in_use", "accountBusy"], ["sharing_session_did_not_stop", "sharingStop"], ["account_sharing_in_progress", "accountChanging"], ["account_disabled", "accountDisabled"], ["account_not_signed_in", "accountNotSignedIn"], ["daemon_shutting_down", "none"],
  ["git_failed", "gitFailure"], ["git_clone_failed", "gitFailure"], ["git_push_failed", "gitFailure"],
  ["git_switch_failed", "gitSwitchFailure"], ["git_restore_failed", "gitRestoreFailure"], ["git_operation_in_progress", "none"], ["push_in_progress", "none"],
  ["terminal_unavailable", "openTerminal"], ["directory_unavailable", "openFileManager"], ["path_open_failed", "openDefaultApplication"],
  ["sharing_resume_failed", "none"], ["terminal_input_lost", "none"], ["trust_not_carried_over", "none"], ["account_sharing_repair_failed", "none"],
  ["provider.codex.login_busy", "none"], ["provider_unsupported", "none"], ["login_already_running", "none"], ["login_not_running", "none"],
  ["reset_credit_in_progress", "none"], ["reset_credit_timed_out", "none"], ["reset_credit_failed", "resetCredit"], ["reset_credit_not_signed_in", "resetCreditLogin"],
])("classifies the known domain code %s with its specific remedy", (code, recovery) => {
  const error = new CodedError("localized text", code, { session: "s1" });
  expect(errorPresentation(error)).toBe(error);
  expect(classifyError(error)).toEqual({ kind: "actionable", recovery });
});

it("gives a list of leftovers that are all worktrees kept for a project folder that could not be checked, or as a protected folder, no remedy of its own", () => {
  const untold = { kind: "worktree", name: "/wt", branch: "a", state: "failed", uncommitted: false, detail: "could not be told", cause: "project_folder_unknown" };
  const failed = { kind: "branch", name: "fix/a", branch: "fix/a", state: "failed", uncommitted: false, detail: "error: refused", cause: null };
  const left = (...items: object[]) => new CodedError("text", "session_branches_not_removed", { count: String(items.length), items: JSON.stringify(items) });
  expect(classifyError(left(untold, { ...untold, name: "/wt2" }))).toEqual({ kind: "actionable", recovery: "none" });
  // Nor a worktree kept as the home folder, a disk's root or a folder of the app's data, which is to stay.
  expect(classifyError(left(untold, { ...untold, name: "/home/dev", cause: "protected_folder" }))).toEqual({ kind: "actionable", recovery: "none" });
  // Nor does the message itself carry one, for either list.
  expect(hasErrorRemedy(left(untold))).toBe(false);
  expect(hasErrorRemedy(left(untold, failed))).toBe(false);
  expect(classifyError(left(untold, failed))).toEqual({ kind: "actionable", recovery: "sessionBranches" });
  expect(classifyError(left(failed))).toEqual({ kind: "actionable", recovery: "sessionBranches" });
});

it.each([
  ["missing", "sourceGone"], ["link", "sourceLink"], ["not_a_directory", "sourceNotFolder"], ["other", "sourceUnreadable"],
  ["a_future_cause", "sourceUnreadable"], [undefined, "sourceGone"],
])("gives a source_unavailable whose cause is %s its own remedy", (cause, recovery) => {
  const error = new CodedError("localized text", "source_unavailable", { path: "/p", detail: "x", ...(cause && { cause }) });
  expect(classifyError(error)).toEqual({ kind: "actionable", recovery });
});

it.each([
  ["access", "appDataAccess"], ["disk_full", "appDataDiskSpace"], ["other", "appDataOther"],
  ["a_future_cause", "appDataOther"], [undefined, "appDataOther"],
])("gives a store_write_failed whose cause is %s the remedy for the application's data", (cause, recovery) => {
  const error = new CodedError("localized text", "store_write_failed", { detail: "attempt to write a readonly database: Error code 1544", ...(cause && { cause }) });
  expect(classifyError(error)).toEqual({ kind: "actionable", recovery });
  expect(appDataRecovery(error)).toBe(recovery);
});

it("keeps the cause of a store_write_failed over a signature in its detail and over a remedy a call site names", () => {
  const full = new CodedError("text", "store_write_failed", { cause: "disk_full", detail: "Permission denied (os error 13)" });
  expect(classifyError(full)).toEqual({ kind: "actionable", recovery: "appDataDiskSpace" });
  const denied = new CodedError("text", "store_write_failed", { cause: "access", detail: "No space left on device (os error 28)" });
  expect(classifyError(denied)).toEqual({ kind: "actionable", recovery: "appDataAccess" });
  expect(classifyError(denied, "diskSpace")).toEqual({ kind: "actionable", recovery: "appDataAccess" });
  expect(classifyError(denied, "fileWrite")).toEqual({ kind: "actionable", recovery: "appDataAccess" });
});

it("gives a store_write_failed nested in a failed launch or a failed automatic switch the remedy of its cause", () => {
  const detail = "database or disk is full: Error code 13: Insertion failed because database is full";
  const launch = new CodedError("text", "launch_failed", { reason: detail, reason_code: "store_write_failed", cause: "disk_full", detail });
  expect(classifyError(launch)).toEqual({ kind: "actionable", recovery: "appDataDiskSpace" });
  // The switch has nothing left to retry, so its remedy does not say to.
  const switched = (cause: string) => new CodedError("text", "account_switch_automatically_failed", { agent: "Claude Code", from: "A", account: "B", reason: detail, reason_code: "store_write_failed", cause, detail });
  expect(classifyError(switched("disk_full"))).toEqual({ kind: "actionable", recovery: "appDataDiskSpaceNoRetry" });
  expect(classifyError(switched("access"))).toEqual({ kind: "actionable", recovery: "appDataAccessNoRetry" });
  expect(classifyError(switched("other"))).toEqual({ kind: "actionable", recovery: "appDataOtherNoRetry" });
  // A failed handoff archive nests its cause as a failed launch does, and the work has moved, so it does not say to retry.
  const archive = new CodedError("text", "handoff_archive_failed", { receiving_session: "s2", reason: detail, reason_code: "store_write_failed", cause: "other", detail });
  expect(classifyError(archive)).toEqual({ kind: "actionable", recovery: "appDataOtherNoRetry" });
  // So does a failed move of the place.
  const moved = new CodedError("text", "handoff_move_failed", { receiving_session: "s2", reason: detail, reason_code: "store_write_failed", cause: "disk_full", detail });
  expect(classifyError(moved)).toEqual({ kind: "actionable", recovery: "appDataDiskSpaceNoRetry" });
});

it("gives a handoff that failed after the work moved the remedy of its cause without a retry, and archiving the original after a failed archive", () => {
  const params = { receiving_session: "s2", session: "s1" };
  const locked = (code: string) => new CodedError("text", code, { ...params, reason: "locked", reason_code: "session_locked" });
  // The nested session_locked and session_lock_busy would otherwise say to retry; the original is what is left to act on.
  expect(classifyError(locked("handoff_move_failed"))).toEqual({ kind: "actionable", recovery: "unlockOriginal" });
  const busy = new CodedError("text", "handoff_archive_failed", { ...params, reason: "busy", reason_code: "session_lock_busy" });
  expect(classifyError(busy)).toEqual({ kind: "actionable", recovery: "waitOriginal" });
  const running = new CodedError("text", "handoff_move_failed", { ...params, reason: "running", reason_code: "session_has_running_sessions" });
  expect(classifyError(running)).toEqual({ kind: "actionable", recovery: "stopSessionsNoRetry" });
  const denied = (code: string) => new CodedError("text", code, { ...params, reason: "denied (os error 13)", reason_code: "internal_error", detail: "denied (os error 13)" });
  expect(classifyError(denied("handoff_move_failed"))).toEqual({ kind: "actionable", recovery: "accessNoRetry" });
  // A failure it names no code for stays unexpected.
  expect(classifyError(new CodedError("text", "handoff_move_failed", { receiving_session: "s2" }))).toEqual({ kind: "unexpected" });
  expect(additionalRecovery(locked("handoff_archive_failed"))).toBe("archiveOriginal");
  expect(additionalRecovery(locked("handoff_move_failed"))).toBe("handoffBothRunning");
  expect(additionalRecovery(new CodedError("text", "handoff_archive_failed", { receiving_session: "s2" }))).toBe("archiveOriginal");
});

it.each<Record<string, string>>([{}, { session: "s1" }, { shell_terminal: "t1" }, { session: "s1", detail: "write failed" }, { shell_terminal: "t1", detail: "write failed" }])(
  "gives a terminal_input_lost of %j no remedy of the group's own",
  (params) => {
    expect(classifyError(new CodedError("text", "terminal_input_lost", params))).toEqual({ kind: "actionable", recovery: "none" });
  },
);

it("keeps internal, protocol and unknown failures unexpected unless an environmental cause is known", () => {
  for (const code of ["internal_error", "unreadable_request", "field_required", "conflicting_fields", "a_future_code"]) {
    expect(classifyError(new CodedError("failure", code, {}))).toEqual({ kind: "unexpected" });
  }
  expect(classifyError(new Error("Please retry disk space permissions bug"))).toEqual({ kind: "unexpected" });
  expect(classifyError(new Error("Missing asset (os error 2)"))).toEqual({ kind: "unexpected" });
  expect(classifyError(new CodedError("No space left on device (os error 28)", "internal_error", {}))).toEqual({ kind: "actionable", recovery: "diskSpace" });
});

it("preserves local typed remedies and OS causes through presentation", () => {
  const cause = Object.assign(new Error("write failed"), { code: "ENOSPC" });
  const failure = new Error("operation failed", { cause });
  expect(classifyError(errorPresentation(failure))).toEqual({ kind: "actionable", recovery: "diskSpace" });
  expect(classifyError(new ActionableError("translated", "clipboard"))).toEqual({ kind: "actionable", recovery: "clipboard" });
  expect(classifyError(new ActionableError("translated", "fileWrite", { cause }))).toEqual({ kind: "actionable", recovery: "diskSpace" });
});

it("keeps a file read's own remedy over the writing remedy an OS signature names", () => {
  const cause = Object.assign(new Error("Permission denied (os error 13)"), { code: "EACCES" });
  expect(classifyError(new ActionableError("Could not read the file.", "fileRead", { cause, detail: "Permission denied (os error 13)" }))).toEqual({ kind: "actionable", recovery: "fileRead" });
  expect(classifyError(new ActionableError("Could not save the file", "fileWrite", { cause }))).toEqual({ kind: "actionable", recovery: "access" });
});

it.each(["issueAddress", "openExternal"] as const)("keeps %s recovery specific to opening the external destination", (recovery) => {
  const cause = Object.assign(new Error("opener denied"), { code: "EACCES" });
  expect(classifyError(new ActionableError("Could not open", recovery, { cause }))).toEqual({ kind: "actionable", recovery });
  expect(classifyError(cause, recovery)).toEqual({ kind: "actionable", recovery });
});

it("keeps different recovery metadata and daemon details as separate queued failures", () => {
  try {
    errorAlerts.report({ message: "same", error: new CodedError("same", "session_locked", { session: "a" }) });
    errorAlerts.report({ message: "same", error: new CodedError("same", "session_locked", { session: "b" }) });
    errorAlerts.report({ message: "same", recovery: "clipboard" });
    errorAlerts.report({ message: "same", recovery: "clipboard" });
    expect(errorAlerts.getSnapshot()).toHaveLength(3);
  } finally {
    while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
  }
});

it("does not collapse distinct original exceptions behind the same failure summary", () => {
  try {
    errorAlerts.report({ message: "Rendering failed", error: new Error("worker init failed") });
    errorAlerts.report({ message: "Rendering failed", error: { message: "grammar failed" } });
    errorAlerts.report({ message: "Rendering failed", error: "module load reason" });
    errorAlerts.report({ message: "Rendering failed", error: { message: "grammar failed" } });
    expect(errorAlerts.getSnapshot()).toHaveLength(3);
    const cyclic = { message: "cyclic failure", cause: undefined as unknown };
    cyclic.cause = cyclic;
    errorAlerts.report({ message: "Rendering failed", error: cyclic });
    expect(errorAlerts.getSnapshot()).toHaveLength(4);
  } finally {
    while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
  }
});

it("gives a denied read its own remedy over the writing one its operating-system signature names", () => {
  const error = new CodedError("denied", "permission_denied", { path: "/p", detail: "Permission denied (os error 13)" });
  expect(classifyError(error)).toEqual({ kind: "actionable", recovery: "readAccess" });
  expect(classifyError(error, "fileRead")).toEqual({ kind: "actionable", recovery: "fileRead" });
});

it.each([
  ["permission_denied", { kind: "actionable", recovery: "access" }], ["busy", { kind: "actionable", recovery: "fileBusy" }],
  ["other", { kind: "unexpected" }], ["a_future_cause", { kind: "unexpected" }], ["constructor", { kind: "unexpected" }],
  [undefined, { kind: "unexpected" }],
] as const)("words a conversation move that failed because of %s by its cause, an unknown one as other", (cause, expected) => {
  const error = new CodedError("move failed", "relocation_failed", { path: "/p", stage: "copy", detail: "x", ...(cause && { cause }) });
  expect(classifyError(error)).toEqual(expected);
});

it.each([
  ["ambiguous", "pathLinkAmbiguous"], ["launchable", "pathLinkLaunchable"], ["not_private", "pathLinkNotPrivate"],
  ["not_a_file_or_folder", "none"], ["unreadable", "none"], ["other", "none"], ["a_future_cause", "none"], ["constructor", "none"], [undefined, "none"],
] as const)("gives a path link refused for the cause %s its own remedy, an unknown one as other", (cause, recovery) => {
  const error = new CodedError("refused", "path_link_refused", { path: "a.ts", ...(cause && { cause }) });
  expect(classifyError(error)).toEqual({ kind: "actionable", recovery });
});

it.each([
  [new CodedError("gone", "file_not_found", { path: "a", root: "project" }), "hostPathGone"],
  [new CodedError("gone", "path_not_found", { path: "a" }), "hostPathGone"],
  [new CodedError("gone", "path_not_a_directory", { path: "a" }), "hostPathGone"],
  [new CodedError("gone", "file_not_found", { path: "a", root: "temporary" }), "none"],
  [new CodedError("outside", "outside_scope", { path: "a" }), "none"],
  [new CodedError("outside", "outside_scope", { path: "a", action: "open" }), "none"],
  [new CodedError("denied", "permission_denied", { path: "a" }), undefined],
  [new Error("x"), undefined],
])("decides the remedy of a failed path link click for %s", (error, recovery) => {
  expect(pathLinkRecovery(error)).toBe(recovery);
});

it.each([
  ["a full disk", "No space left on device (os error 28)", "diskSpaceNoRetry"],
  ["a denied access", "Permission denied (os error 13)", "accessNoRetry"],
])("gives a failed automatic switch whose cause is %s a remedy without the operation to retry", (_name, detail, recovery) => {
  const error = new CodedError("fallback", "account_switch_automatically_failed", { agent: "Claude Code", from: "A", account: "B", reason: detail, reason_code: "internal_error", detail });
  expect(classifyError(error)).toEqual({ kind: "actionable", recovery });
});

it.each([
  ["a denied access", "Permission denied (os error 13)", "appDataAccess"],
  ["a full disk", "No space left on device (os error 28)", "appDataDiskSpace"],
])("words a failed write to the application's own data whose cause is %s for no folder to choose", (_name, detail, recovery) => {
  const error = new CodedError(detail, "internal_error", { detail });
  expect(appDataRecovery(error)).toBe(recovery);
  expect(classifyError(error, appDataRecovery(error))).toEqual({ kind: "actionable", recovery });
});

it("lets an explicit remedy for the application's own data win over an operating-system signature", () => {
  const denied = new CodedError("denied", "internal_error", { detail: "Permission denied (os error 13)" });
  expect(classifyError(denied, "appDataDiskSpace")).toEqual({ kind: "actionable", recovery: "appDataDiskSpace" });
  const full = Object.assign(new Error("x"), { code: "ENOSPC" });
  expect(classifyError(full, "appDataAccess")).toEqual({ kind: "actionable", recovery: "appDataAccess" });
});

it("gives no remedy for the application's own data to a failure without an access or disk-space signature, which keeps its own", () => {
  const coded = new CodedError("no", "invalid_launch_setting", { field: "model", value: "x", agent: "Codex" });
  expect(appDataRecovery(coded)).toBeUndefined();
  expect(classifyError(coded, appDataRecovery(coded))).toEqual(classifyError(coded));
  const refused = Object.assign(new Error("x"), { code: "ECONNREFUSED" });
  expect(appDataRecovery(refused)).toBeUndefined();
  expect(classifyError(refused, appDataRecovery(refused))).toEqual({ kind: "actionable", recovery: "connection" });
  expect(appDataRecovery(new Error("x"))).toBeUndefined();
  expect(classifyError(new Error("x"), appDataRecovery(new Error("x")))).toEqual({ kind: "unexpected" });
});

it("keeps the folder remedy of a denied access where a folder is chosen", () => {
  const detail = "Permission denied (os error 13)";
  expect(classifyError(new CodedError(detail, "internal_error", { detail }))).toEqual({ kind: "actionable", recovery: "access" });
  expect(classifyError(new CodedError("denied", "relocation_failed", { cause: "permission_denied" }))).toEqual({ kind: "actionable", recovery: "access" });
});

it("sends a launch setting the agent does not offer back to Change model when the change came from there, and leaves other failures alone", () => {
  const unoffered = new CodedError("no", "invalid_launch_setting", { field: "model", value: "x", agent: "Codex" });
  expect(modelChangeRecovery(unoffered)).toBe("changeModelSetting");
  expect(classifyError(unoffered, modelChangeRecovery(unoffered))).toEqual({ kind: "actionable", recovery: "changeModelSetting" });
  expect(classifyError(unoffered)).toEqual({ kind: "actionable", recovery: "launchSetting" });
  expect(modelChangeRecovery(new CodedError("no", "model_change_did_not_stop", {}))).toBeUndefined();
  expect(modelChangeRecovery(new Error("x"))).toBeUndefined();
});

it("keeps a missing path's remedy and an operating-system cause over the remedy of the application that failed to open", () => {
  expect(classifyError(new CodedError("gone", "path_open_failed", {}), "hostPathGone")).toEqual({ kind: "actionable", recovery: "hostPathGone" });
  expect(classifyError(new CodedError("denied", "directory_unavailable", { detail: "Permission denied (os error 13)" }))).toEqual({ kind: "actionable", recovery: "access" });
});

it("gives removing an account with its directory no remedy, even where the system's text names one", () => {
  for (const code of ["account_directory_trash_failed", "account_directory_shared", "account_directory_protected", "account_has_running_sessions", "account_being_removed"]) {
    expect(classifyError(new CodedError("x", code, { detail: "Permission denied (os error 13)" }))).toEqual({ kind: "actionable", recovery: "none" });
  }
});
