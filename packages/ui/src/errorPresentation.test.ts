// @vitest-environment jsdom
import { expect, it } from "vitest";

import { noticeErrorPresentation } from "./errorPresentation";

const timeout = { shell: "/bin/zsh", command: "env -0", timeout: "10s" };

it("retains structured launch failures and leaves unrelated or incomplete notices alone", () => {
  const error = noticeErrorPresentation("launch_failed", { reason_code: "shell_environment_timeout", ...timeout }, "m");
  expect(error?.code).toBe("shell_environment_timeout");
  expect(error?.params.shell).toBe("/bin/zsh");
  expect(noticeErrorPresentation("launch_failed", { reason_code: "binary_not_found", binary: "x" }, "m")?.code).toBe("binary_not_found");
  expect(noticeErrorPresentation("launch_failed", { reason_code: "path_not_found", path: "/missing" }, "m")?.params.path).toBe("/missing");
  expect(noticeErrorPresentation("launch_failed", { reason_code: "binary_not_found" }, "m")).toBeUndefined();
  expect(noticeErrorPresentation("launch_failed", { reason_code: "shell_environment_timeout" }, "m")).toBeUndefined();
  expect(noticeErrorPresentation("queued_messages_dropped", { reason_code: "shell_environment_timeout", ...timeout }, "m")).toBeUndefined();
});
