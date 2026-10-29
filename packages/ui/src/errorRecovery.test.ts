// @vitest-environment jsdom
import { expect, it } from "vitest";

import { DaemonRequestError } from "./daemon-client";
import { errorAlerts } from "./errorAlerts";
import { errorPresentation } from "./errorPresentation";
import { classifyError, pathLinkRecovery } from "./errorRecovery";
import { ActionableError } from "./actionableError";
import { hasErrorRemedy } from "./components/errorPartPresence";

it.each([
  ["path_not_found", "path"], ["source_unavailable", "sourceGone"], ["path_already_exists", "pathExists"], ["session_locked", "unlock"],
  ["account_name_taken", "uniqueName"], ["invalid_launch_setting", "launchSetting"], ["model_change_did_not_come_up", "launchSetting"], ["model_change_did_not_stop", "stopProcess"],
  ["connection_closed", "connection"], ["not_a_git_repository", "repository"],
  ["conversation_unresumable", "unresumable"], ["codex_unarchive_failed", "unarchive"],
  ["session_branches_not_removed", "sessionBranches"], ["project_branches_not_determined", "projectBranches"],
  ["session_working", "wait"], ["session_busy", "wait"], ["session_changing", "wait"], ["session_owner_changing", "ownerChanging"], ["session_binding_changed", "ownerMoved"], ["session_not_bound", "session"], ["quick_create_owner_gone", "ownerGone"], ["icon_not_an_image", "avatarImage"], ["icon_too_large", "avatarSize"], ["console_session_never_bound", "none"], ["session_not_bound_to_owner", "none"], ["handoff_messages_dropped", "lostMessages"], ["sharing_messages_dropped", "sharingLostMessages"], ["message_not_sent", "unsentMessage"],
  ["agent_not_installed", "tool"], ["agent_availability_pending", "availability"], ["added_agents_undecided", "availability"],
  ["account_order_invalid", "refresh"], ["agent_already_added", "addedAgents"], ["agent_order_invalid", "addedAgents"], ["agent_not_added", "agentNotAdded"],
  ["grok_sandbox_unsupported", "grokSandbox"],
  ["default_account_not_shareable", "accountNotShareable"], ["account_sharing_failed", "accountSharing"], ["account_still_sharing", "accountBusy"],
  ["account_in_use", "accountBusy"], ["sharing_session_did_not_stop", "sharingStop"], ["account_sharing_in_progress", "accountChanging"], ["account_disabled", "accountDisabled"], ["account_not_signed_in", "accountNotSignedIn"], ["daemon_shutting_down", "none"],
  ["git_failed", "gitFailure"], ["git_clone_failed", "gitFailure"], ["git_push_failed", "gitFailure"],
  ["git_switch_failed", "gitSwitchFailure"], ["git_restore_failed", "gitRestoreFailure"], ["git_operation_in_progress", "none"],
  ["terminal_unavailable", "openTerminal"], ["directory_unavailable", "openFileManager"], ["path_open_failed", "openDefaultApplication"],
  ["sharing_resume_failed", "none"], ["trust_not_carried_over", "none"], ["account_sharing_repair_failed", "none"],
])("classifies the known domain code %s with its specific remedy", (code, recovery) => {
  const error = new DaemonRequestError("localized text", code, { session: "s1" });
  expect(errorPresentation(error)).toBe(error);
  expect(classifyError(error)).toEqual({ kind: "actionable", recovery });
});

it("gives a list of leftovers that are all worktrees kept for a project folder that could not be checked no remedy of its own", () => {
  const untold = { kind: "worktree", name: "/wt", branch: "a", state: "failed", uncommitted: false, detail: "could not be told", cause: "project_folder_unknown" };
  const failed = { kind: "branch", name: "fix/a", branch: "fix/a", state: "failed", uncommitted: false, detail: "error: refused", cause: null };
  const left = (...items: object[]) => new DaemonRequestError("text", "session_branches_not_removed", { count: String(items.length), items: JSON.stringify(items) });
  expect(classifyError(left(untold, { ...untold, name: "/wt2" }))).toEqual({ kind: "actionable", recovery: "none" });
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
  const error = new DaemonRequestError("localized text", "source_unavailable", { path: "/p", detail: "x", ...(cause && { cause }) });
  expect(classifyError(error)).toEqual({ kind: "actionable", recovery });
});

it("keeps internal, protocol and unknown failures unexpected unless an environmental cause is known", () => {
  for (const code of ["internal_error", "unreadable_request", "field_required", "conflicting_fields", "a_future_code"]) {
    expect(classifyError(new DaemonRequestError("failure", code, {}))).toEqual({ kind: "unexpected" });
  }
  expect(classifyError(new Error("Please retry disk space permissions bug"))).toEqual({ kind: "unexpected" });
  expect(classifyError(new Error("Missing asset (os error 2)"))).toEqual({ kind: "unexpected" });
  expect(classifyError(new DaemonRequestError("No space left on device (os error 28)", "internal_error", {}))).toEqual({ kind: "actionable", recovery: "diskSpace" });
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
    errorAlerts.report({ message: "same", error: new DaemonRequestError("same", "session_locked", { session: "a" }) });
    errorAlerts.report({ message: "same", error: new DaemonRequestError("same", "session_locked", { session: "b" }) });
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
  const error = new DaemonRequestError("denied", "permission_denied", { path: "/p", detail: "Permission denied (os error 13)" });
  expect(classifyError(error)).toEqual({ kind: "actionable", recovery: "readAccess" });
  expect(classifyError(error, "fileRead")).toEqual({ kind: "actionable", recovery: "fileRead" });
});

it.each([
  ["permission_denied", { kind: "actionable", recovery: "access" }], ["busy", { kind: "actionable", recovery: "fileBusy" }],
  ["other", { kind: "unexpected" }], ["a_future_cause", { kind: "unexpected" }], ["constructor", { kind: "unexpected" }],
  [undefined, { kind: "unexpected" }],
] as const)("words a conversation move that failed because of %s by its cause, an unknown one as other", (cause, expected) => {
  const error = new DaemonRequestError("move failed", "relocation_failed", { path: "/p", stage: "copy", detail: "x", ...(cause && { cause }) });
  expect(classifyError(error)).toEqual(expected);
});

it.each([
  ["ambiguous", "pathLinkAmbiguous"], ["launchable", "pathLinkLaunchable"], ["not_private", "pathLinkNotPrivate"],
  ["not_a_file_or_folder", "none"], ["unreadable", "none"], ["other", "none"], ["a_future_cause", "none"], ["constructor", "none"], [undefined, "none"],
] as const)("gives a path link refused for the cause %s its own remedy, an unknown one as other", (cause, recovery) => {
  const error = new DaemonRequestError("refused", "path_link_refused", { path: "a.ts", ...(cause && { cause }) });
  expect(classifyError(error)).toEqual({ kind: "actionable", recovery });
});

it.each([
  [new DaemonRequestError("gone", "file_not_found", { path: "a", root: "project" }), "hostPathGone"],
  [new DaemonRequestError("gone", "path_not_found", { path: "a" }), "hostPathGone"],
  [new DaemonRequestError("gone", "path_not_a_directory", { path: "a" }), "hostPathGone"],
  [new DaemonRequestError("gone", "file_not_found", { path: "a", root: "temporary" }), "none"],
  [new DaemonRequestError("outside", "outside_scope", { path: "a" }), "none"],
  [new DaemonRequestError("outside", "outside_scope", { path: "a", action: "open" }), "none"],
  [new DaemonRequestError("denied", "permission_denied", { path: "a" }), undefined],
  [new Error("x"), undefined],
])("decides the remedy of a failed path link click for %s", (error, recovery) => {
  expect(pathLinkRecovery(error)).toBe(recovery);
});

it.each([
  ["a full disk", "No space left on device (os error 28)", "diskSpaceNoRetry"],
  ["a denied access", "Permission denied (os error 13)", "accessNoRetry"],
])("gives a failed automatic switch whose cause is %s a remedy without the operation to retry", (_name, detail, recovery) => {
  const error = new DaemonRequestError("fallback", "account_switch_automatically_failed", { agent: "Claude Code", from: "A", account: "B", reason: detail, reason_code: "internal_error", detail });
  expect(classifyError(error)).toEqual({ kind: "actionable", recovery });
});

it("keeps a missing path's remedy and an operating-system cause over the remedy of the application that failed to open", () => {
  expect(classifyError(new DaemonRequestError("gone", "path_open_failed", {}), "hostPathGone")).toEqual({ kind: "actionable", recovery: "hostPathGone" });
  expect(classifyError(new DaemonRequestError("denied", "directory_unavailable", { detail: "Permission denied (os error 13)" }))).toEqual({ kind: "actionable", recovery: "access" });
});
