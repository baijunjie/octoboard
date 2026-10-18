// @vitest-environment jsdom
import { expect, it, vi } from "vitest";

import { sessionOf } from "./gallery/fixtures/builders";
import type { Event } from "./protocol";
import { accountQuotaOf } from "./quota/accountQuotaOf";
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

it("replaces the account quotas with each update and keeps the entry of an account whose reading did not change", () => {
  const daemon = createDaemon("http://localhost:1234");
  const window = { kind: "session" as const, label: null, used_percent: 10, resets_at: null, window_minutes: 300 };
  const first = { agent: "claude" as const, account: null, fetched_at: 1, plan: "max", windows: [window], unavailable: null };
  const second = { ...first, agent: "codex" as const, plan: "plus" };
  client.onEvents!([{ type: "account_quotas_updated", account_quotas: [first, second] }]);
  const held = daemon.store.getState().accountQuotas;
  expect(accountQuotaOf(held, "codex", null)).toBe(held[1]);
  client.onEvents!([{ type: "account_quotas_updated", account_quotas: [first, { ...second, fetched_at: 2 }] }]);
  const after = daemon.store.getState().accountQuotas;
  expect(after[0]).toBe(held[0]);
  expect(after[1]!.fetched_at).toBe(2);
  client.onEvents!([{ type: "account_quotas_updated", account_quotas: [first] }]);
  expect(daemon.store.getState().accountQuotas).toHaveLength(1);
  expect(accountQuotaOf(daemon.store.getState().accountQuotas, "codex", null)).toBeUndefined();
});
