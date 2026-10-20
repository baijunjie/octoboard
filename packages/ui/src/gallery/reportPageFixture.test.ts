// @vitest-environment jsdom
import { expect, it } from "vitest";
import { createFixtureDaemon } from "./fixtureDaemon";
import { reportScenarios } from "./fixtures/report";
import { installReportPageFixture } from "./reportPageFixture";

it("appends a page through the daemon reducer only for known report fixtures, without changing another session", () => {
  const daemon = createFixtureDaemon(reportScenarios[0]);
  const before = daemon.store.getState().pages;
  const remove = installReportPageFixture(document, daemon, reportScenarios[0].id);
  const page = { id: "probe", console_session_id: "s-hub-2", html: "<p>New</p>", created_at: 1 };
  const send = (detail: unknown) => document.dispatchEvent(new CustomEvent("gallery:report-page-created", { detail }));
  send(null);
  send({ ...page, created_at: NaN });
  send({ ...page, console_session_id: "unknown" });
  expect(daemon.store.getState().pages).toBe(before);
  send(page);
  expect(daemon.store.getState().pages.get("s-hub-2")).toEqual([...before.get("s-hub-2")!, page]);
  expect(daemon.store.getState().pages.get("s-hub-1")).toBe(before.get("s-hub-1"));
  const appended = daemon.store.getState().pages;
  send(page);
  expect(daemon.store.getState().pages).toBe(appended);
  remove();
  send({ ...page, id: "after-cleanup" });
  expect(daemon.store.getState().pages).toBe(appended);
  const removeOther = installReportPageFixture(document, daemon, "focus-project");
  send({ ...page, id: "wrong-scenario" });
  expect(daemon.store.getState().pages).toBe(appended);
  removeOther();
});
