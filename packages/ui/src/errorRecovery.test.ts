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
  ["session_branches_not_removed", "sessionBranches"], ["project_branches_not_determined", "projectBranches"],
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
  expect(classifyError(new ActionableError("translated", "file", { cause }))).toEqual({ kind: "actionable", recovery: "diskSpace" });
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
