// @vitest-environment jsdom
import { expect, it } from "vitest";

import { errorPresentation, hasCodeParams, noticeErrorPresentation } from "./errorPresentation";

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
  }
  const foreign = { message: "foreign denial", stack: "foreign stack", code: "EACCES" };
  expect(errorPresentation(foreign)).toMatchObject({ message: foreign.message, stack: foreign.stack, cause: foreign });
});

it("retains a usable fallback when a recognized rich error lacks a required parameter", () => {
  expect(hasCodeParams({ code: "account_name_taken", params: { name: "Work" } })).toBe(false);
  expect(hasCodeParams({ code: "path_not_absolute", params: {} })).toBe(false);
  expect(hasCodeParams({ code: "models_command_failed", params: { detail: "denied" } })).toBe(false);
});
