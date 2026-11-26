// @vitest-environment jsdom
import { beforeEach, expect, it, vi } from "vitest";

import { sessionOf, standingNoticeOf } from "../fixtures/builders";
import type { Event } from "../protocol";
import { createDaemon, dismissStandingNotice } from ".";
import type { WindowNotification } from "./notifications";
import { shownStandingNotices, standingNoticeKey } from "./standingNotices";

const client = vi.hoisted(() => ({ onEvent: undefined as ((event: Event) => void) | undefined, onEvents: undefined as ((events: Event[]) => void) | undefined }));
vi.mock("../daemon-client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../daemon-client")>()),
  DaemonClient: class {
    onEvent(listener: (event: Event) => void) { client.onEvent = listener; }
    onEvents(listener: (events: Event[]) => void) { client.onEvents = listener; }
    onConnectionChange() {}
    onRetryPendingChange() {}
    request() { return new Promise<Event>(() => {}); }
  },
}));

let daemon: ReturnType<typeof createDaemon>;
let notifications: WindowNotification[];
const receive = (event: Event) => {
  client.onEvents!([event]);
  client.onEvent!(event);
};
const notice = (standing: boolean): Event => ({ type: "session_notice", session: "s", code: "hooks_not_received", params: { agent: "Claude Code" }, message: "No hooks.", ...(standing ? { standing } : {}) });

beforeEach(() => {
  daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  notifications = [];
  daemon.onNotification((notification) => notifications.push(notification));
});

it("raises no notification for a standing notice, which the session's record carries, and still raises one for a plain notice", () => {
  receive(notice(true));
  expect(notifications).toEqual([]);
  receive(notice(false));
  expect(notifications.map((notification) => notification.kind)).toEqual(["notice"]);
});

const hooks = standingNoticeOf("hooks_not_received", { agent: "Claude Code" }, "", 1);
const record = standingNoticeOf("conversation_record_not_found", { agent: "Claude Code" }, "", 2);
const session = sessionOf("s", "c", undefined, "Work", "working", { standing_notices: [hooks, record] });

it("shows a session's standing notices less those dismissed in this window, until one is raised again", () => {
  receive({ type: "session_upserted", session });
  const shown = () => shownStandingNotices(daemon.store.getState().sessions.get("s"), daemon.store.getState().dismissedStandingNotices);
  expect(shown()).toEqual([hooks, record]);

  dismissStandingNotice(daemon.store, standingNoticeKey("s", record));
  expect(shown()).toEqual([hooks]);

  // The daemon raising it again stamps it anew: the dismissal was of the earlier one.
  const again = { ...record, at: 3 };
  receive({ type: "session_upserted", session: { ...session, standing_notices: [hooks, again] } });
  expect(shown()).toEqual([hooks, again]);
});

it("tells notices of one code apart by their detail param", () => {
  const a = standingNoticeOf("session_git_hooks_unavailable", { detail: "a" }, "", 1);
  const b = standingNoticeOf("session_git_hooks_unavailable", { detail: "b" }, "", 1);
  expect(standingNoticeKey("s", a)).not.toBe(standingNoticeKey("s", b));
  expect(standingNoticeKey("s", a)).not.toBe(standingNoticeKey("t", a));
});

it("forgets the dismissals of a session that is deleted", () => {
  receive({ type: "session_upserted", session });
  dismissStandingNotice(daemon.store, standingNoticeKey("s", hooks));
  receive({ type: "session_deleted", session: "s" });
  expect(daemon.store.getState().dismissedStandingNotices.size).toBe(0);
});

it("keeps the state it has when a notice already dismissed is dismissed again", () => {
  dismissStandingNotice(daemon.store, standingNoticeKey("s", hooks));
  const before = daemon.store.getState();
  dismissStandingNotice(daemon.store, standingNoticeKey("s", hooks));
  expect(daemon.store.getState()).toBe(before);
});

it("keeps the dismissals of another session when one is deleted", () => {
  const other = sessionOf("t", "c", undefined, "Other", "working", { standing_notices: [hooks] });
  receive({ type: "session_upserted", session });
  receive({ type: "session_upserted", session: other });
  dismissStandingNotice(daemon.store, standingNoticeKey("s", hooks));
  dismissStandingNotice(daemon.store, standingNoticeKey("t", hooks));
  receive({ type: "session_deleted", session: "s" });
  expect([...daemon.store.getState().dismissedStandingNotices]).toEqual([standingNoticeKey("t", hooks)]);
});

it("shows no notice for a session with none, or none at all", () => {
  expect(shownStandingNotices(undefined, new Set())).toEqual([]);
  expect(shownStandingNotices(sessionOf("s", "c", undefined, "Work", "working"), new Set())).toEqual([]);
});
