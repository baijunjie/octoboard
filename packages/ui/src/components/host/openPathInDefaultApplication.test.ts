// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";

import { errorAlerts } from "../../components/alerts/errorAlerts";
import { CodedError } from "../../errors/codedError";
import type { RequestSender } from "../../requests";
import { openPathInDefaultApplication } from "./openPathInDefaultApplication";

afterEach(() => {
  while (errorAlerts.getSnapshot().length > 0) errorAlerts.acknowledge();
});

const path = "/tmp/out/report.pdf";
const sender = (answer: () => Promise<unknown>) => vi.fn(answer) as unknown as RequestSender & ReturnType<typeof vi.fn>;

it("asks the daemon to open the path with the default application and says nothing on success", async () => {
  const request = sender(async () => ({ type: "ack", id: null }));
  await openPathInDefaultApplication({ path, project: "p", request });
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "open_path_link", project: "p", path: "/tmp/out/report.pdf" });
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("sends no project for a session that has none", async () => {
  const request = sender(async () => ({ type: "ack", id: null }));
  await openPathInDefaultApplication({ path, project: undefined, request });
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "open_path_link", project: undefined, path: "/tmp/out/report.pdf" });
});

it("raises a failure under the default application's title, with the remedy of a path that is gone", async () => {
  const request = sender(async () => { throw new CodedError("gone", "file_not_found", { path: "/tmp/out/report.pdf", root: "temporary" }); });
  await openPathInDefaultApplication({ path, project: "p", request });
  const [alert] = errorAlerts.getSnapshot();
  expect(alert.title).toBe("Could not open in the default application");
  expect(alert.classification).toEqual({ kind: "actionable", recovery: "none" });
});

it("words the refusal of a file the system could launch for a button, not for a link", async () => {
  const request = sender(async () => { throw new CodedError("not opened from a link", "path_link_refused", { path: "/tmp/run.sh", cause: "launchable" }); });
  await openPathInDefaultApplication({ path: "/tmp/run.sh", project: "p", request });
  const [alert] = errorAlerts.getSnapshot();
  expect(alert.message.replace(/[⁨⁩]/g, "")).toBe("/tmp/run.sh is something the system would run, so it is not opened in the default application.");
});
