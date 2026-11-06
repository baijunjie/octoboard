// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi, type MockInstance } from "vitest";

import { errorAlerts } from "../../components/alerts/errorAlerts";
import { t } from "../../i18n/language";
import { CodedError } from "../../errors/codedError";
import { revealPathTarget } from "./revealPathTarget";

let report: MockInstance<typeof errorAlerts.report>;
beforeEach(() => { report = vi.spyOn(errorAlerts, "report").mockImplementation(() => {}); });
afterEach(() => { report.mockRestore(); });

it("asks the daemon to reveal the path, whatever a click on it would do", async () => {
  const request = vi.fn(async () => ({ type: "ack", id: null })) as never;
  await revealPathTarget({ target: { path: "/r/a.ts", view: { worktree: "w", path: "a.ts" }, viewerOnly: true }, project: "p", request });
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "reveal_path_link", project: "p", path: "/r/a.ts" });
  expect(report).not.toHaveBeenCalled();
});

it("reports a refusal, with the remedy of finding a path of the project that is gone", async () => {
  const request = vi.fn(() => Promise.reject(new CodedError("gone", "file_not_found", { path: "docs", root: "project" }))) as never;
  await revealPathTarget({ target: { path: "docs", kind: "directory" }, project: "p", request });
  expect(report).toHaveBeenCalledOnce();
  expect(report.mock.lastCall![0].recovery).toBe("hostPathGone");
  expect(report.mock.lastCall![0].title).toBe(t("common.revealFailedTitle"));
});
