// @vitest-environment jsdom
import { expect, it, vi } from "vitest";

import { sessionOf } from "./gallery/fixtures/builders";
import type { Event } from "./protocol";
import { createDaemon } from "./store";

const client = vi.hoisted(() => ({ onEvent: undefined as ((event: Event) => void) | undefined }));
vi.mock("./daemon-client", () => ({
  DaemonClient: class {
    onEvent(listener: (event: Event) => void) { client.onEvent = listener; }
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
    client.onEvent!(event);
    if (event === reply && order === "reply-first") expect(daemon.store.getState().sessions.get(original.id)).toBe(original);
  }
  expect(daemon.store.getState().sessions.get(original.id)).toBe(current);
});
