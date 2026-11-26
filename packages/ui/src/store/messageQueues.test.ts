import { describe, expect, it } from "vitest";

import { queuedMessage, sessionOf } from "../fixtures/builders";
import type { Event, QueuedMessage } from "../protocol";
import { applyDaemonEvent } from "./actions";
import { createStateStore } from "./state";

const snapshot = (queues?: Array<{ session: string; messages: QueuedMessage[] }>) => ({
  type: "snapshot", hosts: [], consoles: [], projects: [], sessions: [sessionOf("s", "c", "p", "Work", "idle")], trusted_directories: [], settings: {}, git_statuses: [],
  agent_availability: [], account_quotas: [], account_logins: [], account_sharing: [], agent_cli_statuses: [], home_dir: null,
  host: { os: "macos", path_style: "unix", file_links: true, trash: "trash", settings: "system_settings", keep_awake: false }, keep_awake_held: false, shell_terminals: [],
  ...(queues && { message_queues: queues }),
}) as unknown as Event;

describe("the composer queues in the store", () => {
  const first = queuedMessage({ id: "m1", text: "first" });

  it("holds no queues for a daemon whose snapshot carries none, and each session's from one that does", () => {
    const store = createStateStore();
    applyDaemonEvent(store, snapshot());
    expect(store.getState().messageQueues).toBeNull();
    applyDaemonEvent(store, snapshot([{ session: "s", messages: [first] }, { session: "t", messages: [] }]));
    expect(store.getState().messageQueues).toEqual(new Map([["s", [first]]]));
  });

  it("keeps the queues it holds when a snapshot repeats them", () => {
    const store = createStateStore();
    applyDaemonEvent(store, snapshot([{ session: "s", messages: [first] }]));
    const held = store.getState().messageQueues;
    applyDaemonEvent(store, snapshot([{ session: "s", messages: [{ ...first }] }]));
    expect(store.getState().messageQueues).toBe(held);
  });

  it("takes each queue broadcast whole, and drops a session's entry once nothing is queued", () => {
    const store = createStateStore({ messageQueues: new Map() });
    applyDaemonEvent(store, { type: "session_message_queue", session: "s", messages: [first] });
    expect(store.getState().messageQueues?.get("s")).toEqual([first]);
    const held = store.getState();
    applyDaemonEvent(store, { type: "session_message_queue", session: "s", messages: [{ ...first }] });
    expect(store.getState()).toBe(held);
    applyDaemonEvent(store, { type: "session_message_queue", session: "s", messages: [] });
    expect(store.getState().messageQueues).toEqual(new Map());
  });

  it("drops a deleted session's queue", () => {
    const store = createStateStore({ sessions: new Map([["s", sessionOf("s", "c", "p", "Work", "idle")]]), messageQueues: new Map([["s", [first]]]) });
    applyDaemonEvent(store, { type: "session_deleted", session: "s" });
    expect(store.getState().messageQueues).toEqual(new Map());
  });
});
