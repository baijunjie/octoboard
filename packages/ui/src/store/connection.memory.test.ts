// @vitest-environment jsdom
import { expect, it, vi } from "vitest";

import type { Event } from "../protocol";
import { createDaemon, createInertDaemon } from ".";
import type { MemoryChangeEvent } from "./memoryListeners";
import type { WindowNotification } from "./notifications";

const client = vi.hoisted(() => ({ onEvent: undefined as ((event: Event) => void) | undefined }));
vi.mock("../daemon-client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../daemon-client")>()),
  DaemonClient: class {
    onEvent(listener: (event: Event) => void) { client.onEvent = listener; }
    onEvents() {}
    onConnectionChange() {}
    onRetryPendingChange() {}
  },
}));

const deleted: Event = { type: "memories_deleted", scope: { kind: "console", console_id: "c-1" }, memories: ["a"] };

it("hands the changes to a console's memory to every memory listener in order, never as a notification, and stops at the unsubscribe", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const heard: MemoryChangeEvent[] = [];
  const notifications: WindowNotification[] = [];
  daemon.onNotification((notification) => notifications.push(notification));
  const unsubscribe = daemon.onMemoryChange((event) => heard.push(event));
  client.onEvent!(deleted);
  client.onEvent!({ ...deleted, memories: ["b"] });
  expect(heard.map((event) => (event.type === "memories_deleted" ? event.memories : []))).toEqual([["a"], ["b"]]);
  expect(notifications).toEqual([]);
  unsubscribe();
  client.onEvent!(deleted);
  expect(heard).toHaveLength(2);
});

it("has a subscription on the inert daemon that hears nothing and unsubscribes", () => {
  expect(createInertDaemon().onMemoryChange(() => {})()).toBeUndefined();
});
