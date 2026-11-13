import { expect, it } from "vitest";

import { updateNoticeKind } from "./noticeKind";
import type { UpdateState } from "./updateStore";

const IDLE: UpdateState = { phase: "idle", channel: "stable", autoCheck: true, noticeDismissed: false, restartNoticeHidden: false, restarting: false };
const FOUND = { version: "0.2.0" };

it("shows no notice while nothing is found", () => {
  expect(updateNoticeKind(IDLE)).toBeUndefined();
  expect(updateNoticeKind({ ...IDLE, phase: "checking" })).toBeUndefined();
});

it("offers an update found, and offers it by hand where the daemon cannot replace its executable", () => {
  expect(updateNoticeKind({ ...IDLE, update: FOUND })).toBe("available");
  expect(updateNoticeKind({ ...IDLE, update: FOUND, manual: { path: "/opt/octoboardd" } })).toBe("manual");
});

it("hides a dismissed notice, the manual one included", () => {
  expect(updateNoticeKind({ ...IDLE, update: FOUND, noticeDismissed: true })).toBeUndefined();
  expect(updateNoticeKind({ ...IDLE, update: FOUND, manual: { path: "/x" }, noticeDismissed: true })).toBeUndefined();
});

it("shows a download under way, which cannot be dismissed", () => {
  expect(updateNoticeKind({ ...IDLE, phase: "downloading", update: FOUND, noticeDismissed: true })).toBe("downloading");
});

it("shows an installed update until Later hides it", () => {
  expect(updateNoticeKind({ ...IDLE, phase: "installed" })).toBe("installed");
  expect(updateNoticeKind({ ...IDLE, phase: "installed", restartNoticeHidden: true })).toBeUndefined();
});

it("asks for a reload before anything else, and nothing dismissed or hidden withholds it", () => {
  const reload = { ...IDLE, reloadNeeded: true, noticeDismissed: true, restartNoticeHidden: true };
  expect(updateNoticeKind(reload)).toBe("reload");
  expect(updateNoticeKind({ ...reload, phase: "installed" })).toBe("reload");
  expect(updateNoticeKind({ ...reload, phase: "downloading", update: FOUND })).toBe("reload");
  expect(updateNoticeKind({ ...reload, update: FOUND, manual: { path: "/x" } })).toBe("reload");
});
