// @vitest-environment jsdom
import { expect, it } from "vitest";

import { errorPresentation, hasCodeParams, isStructuredFailure, noticeErrorPresentation, noticeKind } from "./errorPresentation";

const timeout = { shell: "/bin/zsh", command: "env -0", timeout: "10s" };

it("retains launch failure codes even without presentation params and leaves unrelated notices alone", () => {
  const error = noticeErrorPresentation("launch_failed", { reason_code: "shell_environment_timeout", ...timeout }, "m");
  expect(error?.code).toBe("shell_environment_timeout");
  expect(error?.params.shell).toBe("/bin/zsh");
  expect(noticeErrorPresentation("launch_failed", { reason_code: "binary_not_found", binary: "x" }, "m")?.code).toBe("binary_not_found");
  expect(noticeErrorPresentation("launch_failed", { reason_code: "path_not_found", path: "/missing" }, "m")?.params.path).toBe("/missing");
  expect(noticeErrorPresentation("launch_failed", { reason_code: "binary_not_found" }, "m")?.code).toBe("binary_not_found");
  expect(noticeErrorPresentation("launch_failed", { reason_code: "shell_environment_timeout" }, "m")?.code).toBe("shell_environment_timeout");
  expect(noticeErrorPresentation("queued_messages_dropped", { reason_code: "shell_environment_timeout", ...timeout }, "m")).toBeUndefined();
});

it("preserves rejected strings, primitives and foreign Error diagnostics", () => {
  for (const value of ["native invocation rejected", 42, null, undefined]) {
    expect(errorPresentation(value)).toMatchObject({ message: String(value), cause: value });
    // Nothing came with a stack, and the wrapping's own would say nothing about the failure.
    expect(errorPresentation(value).stack).toBeUndefined();
  }
  const foreign = { message: "foreign denial", stack: "foreign stack", code: "EACCES" };
  expect(errorPresentation(foreign)).toMatchObject({ message: foreign.message, stack: foreign.stack, cause: foreign });
});

it("retains a usable fallback when a recognized rich error lacks a required parameter", () => {
  expect(hasCodeParams({ code: "account_name_taken", params: { name: "Work" } })).toBe(false);
  expect(hasCodeParams({ code: "path_not_absolute", params: {} })).toBe(false);
  expect(hasCodeParams({ code: "models_command_failed", params: { detail: "denied" } })).toBe(false);
});

it.each([
  ["sharing_resume_failed", { account: "a1", name: "Work", detail: "no such file" }, "error"],
  ["trust_not_carried_over", { agent: "Grok Build", path: "/p", reason: "x", reason_code: "store_io_failed", detail: "Permission denied" }, "error"],
  ["trust_not_carried_over", { agent: "Grok Build", path: "/p", reason: "a newer daemon's reason", reason_code: "a_newer_reason" }, "error"],
  ["trust_not_carried_over", { agent: "Grok Build", path: "/p", reason: "x", reason_code: "store_locked" }, "notice"],
  ["account_sharing_repair_failed", { account: "a1", name: "Work", items: "[]", cause: "permission_denied" }, "error"],
  ["claude_workspace_untrusted", {}, "notice"],
])("shows the session notice %s with %j as an %s", (code, params, kind) => {
  expect(noticeKind(code, params)).toBe(kind);
});

it.each([
  ["a message with chips", "path_not_found", { path: "/p" }, true],
  ["a shell timeout with its details", "shell_environment_timeout", { shell: "zsh", command: "env", timeout: "5" }, true],
  ["a failure the catalog has no words for", "a_newer_code", {}, true],
  ["a failure missing the params its message shows", "path_not_found", {}, false],
])("tells whether %s is worded in a structured way", (_case, code, params, structured) => {
  expect(isStructuredFailure({ code, params })).toBe(structured);
});
