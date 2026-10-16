// @vitest-environment jsdom
import { expect, it, vi } from "vitest";

import { sessionOf } from "./gallery/fixtures/builders";
import type { Event } from "./protocol";
import { createDaemon } from "./store";

const client = vi.hoisted(() => ({
  onEvent: undefined as ((event: Event) => void) | undefined,
  onEvents: undefined as ((events: Event[]) => void) | undefined,
}));
vi.mock("./daemon-client", () => ({
  DaemonClient: class {
    onEvent(listener: (event: Event) => void) { client.onEvent = listener; }
    onEvents(listener: (events: Event[]) => void) { client.onEvents = listener; }
    onConnectionChange() {}
  },
  DaemonRequestError: class extends Error {},
}));

it.each(["reply-first", "broadcast-first"] as const)("keeps the authoritative session title when opening a session: %s", (order) => {
  const daemon = createDaemon("http://localhost:1234");
  const original = sessionOf("s", "c", "p", "Created name", "idle");
  const current = { ...original, terminal_title: "Current task", status: "working" as const };
  const reply: Event = { type: "session_opened", id: "request", session: original };
  const broadcast: Event = { type: "session_upserted", session: current };
  for (const event of order === "reply-first" ? [reply, broadcast] : [broadcast, reply]) {
    client.onEvents!([event]);
    if (event === reply && order === "reply-first") expect(daemon.store.getState().sessions.get(original.id)).toBe(original);
  }
  expect(daemon.store.getState().sessions.get(original.id)).toBe(current);
});

it("keeps the stored session, and does not notify, when an upsert repeats what it already holds", () => {
  const daemon = createDaemon("http://localhost:1234");
  const session = sessionOf("s", "c", "p", "Same", "working");
  client.onEvents!([{ type: "session_upserted", session }]);
  const before = daemon.store.getState();
  const notified = vi.fn();
  daemon.store.subscribe(notified);
  client.onEvents!([{ type: "session_upserted", session: { ...session } }]);
  expect(daemon.store.getState()).toBe(before);
  expect(daemon.store.getState().sessions.get("s")).toBe(session);
  expect(notified).not.toHaveBeenCalled();
});

it("applies the events of one batch as a single update", () => {
  const daemon = createDaemon("http://localhost:1234");
  const notified = vi.fn();
  daemon.store.subscribe(notified);
  client.onEvents!([
    { type: "session_upserted", session: sessionOf("a", "c", "p", "A", "working") },
    { type: "session_upserted", session: sessionOf("b", "c", "p", "B", "idle") },
    { type: "session_deleted", session: "a" },
  ]);
  expect(notified).toHaveBeenCalledTimes(1);
  expect(Array.from(daemon.store.getState().sessions.keys())).toEqual(["b"]);
});
