// @vitest-environment jsdom
import { expect, it, vi } from "vitest";

import type { Event } from "../protocol";
import { createDaemon, createInertDaemon } from ".";
import type { ContentSearchEvent } from "./contentSearchListeners";
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

const batch: Event = { type: "project_content_matches", search: "s-1", project: "p", matches: [] };
const end: Event = { type: "project_content_search_ended", search: "s-1", project: "p", matches: 0, files: 0, complete: true };

it("hands a search's events to the listener of that search alone, never as a notification, and stops at the unsubscribe", () => {
  const daemon = createDaemon({ origin: "http://localhost:1234", key: "k" });
  const heard: ContentSearchEvent[] = [];
  const elsewhere = vi.fn();
  const notifications: WindowNotification[] = [];
  daemon.onNotification((notification) => notifications.push(notification));
  const unsubscribe = daemon.onContentSearch("s-1", (event) => heard.push(event));
  daemon.onContentSearch("s-2", elsewhere);
  client.onEvent!(batch);
  client.onEvent!(end);
  expect(heard).toEqual([batch, end]);
  expect(elsewhere).not.toHaveBeenCalled();
  expect(notifications).toEqual([]);
  unsubscribe();
  client.onEvent!(batch);
  expect(heard).toHaveLength(2);
});

it("drops the events of a search nobody listens to without failing", () => {
  createDaemon({ origin: "http://localhost:1234", key: "k" });
  expect(() => client.onEvent!(batch)).not.toThrow();
});

it("has a subscription on the inert daemon that hears nothing and unsubscribes", () => {
  const unsubscribe = createInertDaemon().onContentSearch("s-1", () => {});
  expect(unsubscribe()).toBeUndefined();
});
