import { expect, it, vi } from "vitest";

import { createMemoryListeners, isMemoryChangeEvent, type MemoryChangeEvent } from "./memoryListeners";

it("hands every change to every listener until it unsubscribes", () => {
  const listeners = createMemoryListeners();
  const first = vi.fn();
  const second = vi.fn();
  const stop = listeners.subscribe(first);
  listeners.subscribe(second);
  const event: MemoryChangeEvent = { type: "memories_deleted", scope: { kind: "console", console_id: "c" }, memories: ["a"] };
  listeners.deliver(event);
  stop();
  listeners.deliver(event);
  expect(first).toHaveBeenCalledTimes(1);
  expect(second).toHaveBeenCalledTimes(2);
});

it("tells a memory change from another event", () => {
  expect(isMemoryChangeEvent({ type: "memories_deleted", scope: { kind: "console", console_id: "c" }, memories: [] })).toBe(true);
  expect(isMemoryChangeEvent({ type: "ack", id: null })).toBe(false);
});
