// @vitest-environment jsdom
import { expect, it } from "vitest";

import { DaemonRequestError } from "./daemon-client";
import { errorAlerts } from "./errorAlerts";
import { errorPresentation } from "./errorPresentation";
import { ActionableError, classifyError } from "./errorRecovery";

it.each([
  ["path_not_found", "path"], ["path_already_exists", "pathExists"], ["session_locked", "unlock"],
  ["account_name_taken", "uniqueName"], ["invalid_launch_setting", "launchSetting"],
  ["connection_closed", "connection"], ["not_a_git_repository", "repository"],
  ["conversation_unresumable", "unresumable"], ["codex_unarchive_failed", "unarchive"],
  ["session_branches_not_removed", "sessionBranches"], ["project_branches_not_determined", "projectBranches"],
  ["session_working", "wait"], ["session_busy", "wait"], ["session_changing", "wait"], ["session_owner_changing", "ownerChanging"], ["session_binding_changed", "ownerMoved"], ["session_not_bound", "session"], ["quick_create_owner_gone", "ownerGone"], ["icon_not_an_image", "avatarImage"], ["icon_too_large", "avatarSize"], ["console_session_never_bound", "none"], ["session_not_bound_to_owner", "none"], ["handoff_messages_dropped", "lostMessages"], ["sharing_messages_dropped", "sharingLostMessages"], ["message_not_sent", "unsentMessage"],
  ["agent_not_installed", "tool"], ["agent_availability_pending", "availability"], ["added_agents_undecided", "availability"],
  ["agent_already_added", "addedAgents"], ["agent_order_invalid", "addedAgents"], ["agent_not_added", "agentNotAdded"],
  ["grok_sandbox_unsupported", "grokSandbox"],
  ["default_account_not_shareable", "accountNotShareable"], ["account_sharing_failed", "accountSharing"], ["account_still_sharing", "accountBusy"],
  ["account_in_use", "accountBusy"], ["sharing_session_did_not_stop", "sharingStop"], ["account_sharing_in_progress", "accountChanging"], ["account_disabled", "accountDisabled"],
  ["git_failed", "gitFailure"], ["git_clone_failed", "gitFailure"],
  ["sharing_resume_failed", "none"], ["trust_not_carried_over", "none"], ["account_sharing_repair_failed", "none"],
])("classifies the known domain code %s with its specific remedy", (code, recovery) => {
  const error = new DaemonRequestError("localized text", code, { session: "s1" });
  expect(errorPresentation(error)).toBe(error);
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

it.each(["issueLink", "openExternal"] as const)("keeps %s recovery specific to opening the external destination", (recovery) => {
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
