// @vitest-environment jsdom
import { expect, it, vi } from "vitest";

import { CodedError } from "../errors/codedError";
import { sessionOf } from "../fixtures/builders";
import type { Event } from "../protocol";
import { createDaemon } from ".";
import { currentLanguage } from "../i18n/language";
import { broadcastNotification, errorNotification, type NotifiedBroadcast, type NotificationRequest } from "./notifications";

const client = vi.hoisted(() => ({ onEvent: undefined as ((event: Event) => void) | undefined }));
vi.mock("../daemon-client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../daemon-client")>()),
  DaemonClient: class {
    onEvent(listener: (event: Event) => void) { client.onEvent = listener; }
    onEvents() {}
    onConnectionChange() {}
  },
}));

function notificationsOf(events: Event[]): NotificationRequest[] {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const notifications: NotificationRequest[] = [];
  daemon.onNotification((notification) => notifications.push(notification));
  events.forEach((event) => client.onEvent!(event));
  return notifications;
}

it("shows a failure notice as an error that keeps the daemon's code and params", () => {
  const [notification] = notificationsOf([{ type: "session_notice", session: "s", code: "account_sharing_repair_failed", params: { name: "Work" }, message: "raw" }]);
  expect(notification).toMatchObject({ kind: "error", session: "s", daemonTitle: undefined });
  expect(notification.daemonError).toBeInstanceOf(CodedError);
  expect(notification.daemonError).toMatchObject({ code: "account_sharing_repair_failed", params: { name: "Work" }, message: "raw" });
});

it("shows any other coded notice as a notice that keeps its code", () => {
  const [notification] = notificationsOf([{ type: "session_notice", session: "s", code: "trust_not_carried_over", params: {}, message: "raw" }]);
  expect(notification).toMatchObject({ kind: "notice", session: "s", daemonTitle: undefined });
  expect(notification.daemonError).toMatchObject({ code: "trust_not_carried_over", message: "raw" });
});

it("shows a failed launch as the failure it reports under a title of its own", () => {
  const params = { reason_code: "directory_missing", title: "Task" };
  const [notification] = notificationsOf([{ type: "session_notice", session: "s", code: "launch_failed", params, message: "raw" }]);
  expect(notification).toMatchObject({ kind: "error", session: "s" });
  expect(notification.daemonTitle).toMatchObject({ code: "launch_failed.title", params, message: "raw" });
  expect(notification.daemonError).toMatchObject({ code: "directory_missing", params, message: "raw" });
});

it("shows a broadcast error with no request id as an error with no session or title", () => {
  const [notification] = notificationsOf([{ type: "error", id: null, code: "internal_error", params: {}, message: "raw" }]);
  expect(notification.kind).toBe("error");
  expect(notification.daemonTitle).toBeUndefined();
  expect(notification.session).toBeUndefined();
  expect(notification.daemonError).toMatchObject({ code: "internal_error", message: "raw" });
  expect(notification.message).toBeTruthy();
});

it("leaves a request's own error reply to its caller", () => {
  expect(notificationsOf([{ type: "error", id: "request", code: "internal_error", params: {}, message: "raw" }])).toEqual([]);
});

it("words the message with the names of the records the store holds", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const notifications: NotificationRequest[] = [];
  daemon.onNotification((notification) => notifications.push(notification));
  daemon.store.setState({ sessions: new Map([["s", sessionOf("s", "c", "p", "Named session", "idle")]]) });
  client.onEvent!({ type: "session_notice", session: "s", code: "session_did_not_stop", params: { session: "s" }, message: "raw" });
  expect(notifications[0].message).toContain("Named session");
});

it("raises an error through the notification route as the error request of its call site", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const notifications: NotificationRequest[] = [];
  daemon.onNotification((notification) => notifications.push(notification));
  const error = new CodedError("raw", "internal_error", {});
  daemon.raiseError(error, { title: "Could not save", session: "s", recovery: "refresh" });
  expect(notifications).toEqual([errorNotification(error, { title: "Could not save", session: "s", recovery: "refresh" })]);
});

// A wiring check: the gallery's daemon calls `broadcastNotification` itself, so the store has to be doing the same.
it("is what the store shows for the same broadcast", () => {
  const records = createDaemon({ origin: "http://localhost:1234", key: "k" }).store.getState();
  const notice: NotifiedBroadcast = { type: "session_notice", session: "s", code: "launch_failed", params: { reason_code: "directory_missing", title: "Task" }, message: "raw" };
  const failure: NotifiedBroadcast = { type: "error", id: null, code: "internal_error", params: {}, message: "raw" };
  for (const event of [notice, failure] as const) {
    expect(broadcastNotification(event, records, currentLanguage())).toEqual(notificationsOf([event])[0]);
  }
});
