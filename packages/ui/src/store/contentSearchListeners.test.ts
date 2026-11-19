import { expect, it, vi } from "vitest";

import type { Event } from "../protocol";
import { createContentSearchListeners, isContentSearchEvent, type ContentSearchEvent } from "./contentSearchListeners";

const matches = (search: string): ContentSearchEvent => ({
  type: "project_content_matches",
  search,
  project: "p",
  matches: [{ path: "a.ts", line: 1, text: "foo", text_start: 0, ranges: [{ start: 0, end: 3 }], clipped_end: false }],
});
const ended = (search: string): ContentSearchEvent => ({ type: "project_content_search_ended", search, project: "p", matches: 1, files: 1, complete: true });

it("tells the two events of a content search from every other event", () => {
  expect(isContentSearchEvent(matches("a"))).toBe(true);
  expect(isContentSearchEvent(ended("a"))).toBe(true);
  const others: Event[] = [
    { type: "ack", id: null },
    { type: "error", id: null, code: "internal_error", params: {}, message: "raw" },
    { type: "session_notice", session: "s", code: "trust_not_carried_over", params: {}, message: "raw" },
  ];
  expect(others.map(isContentSearchEvent)).toEqual([false, false, false]);
});

it("delivers an event only to the listeners of the search it names, in order", () => {
  const listeners = createContentSearchListeners();
  const mine = vi.fn();
  const other = vi.fn();
  listeners.subscribe("a", mine);
  listeners.subscribe("b", other);
  listeners.deliver(matches("a"));
  listeners.deliver(ended("a"));
  expect(mine.mock.calls).toEqual([[matches("a")], [ended("a")]]);
  expect(other).not.toHaveBeenCalled();
});

it("delivers nothing once the listener has unsubscribed, and drops an event no listener holds", () => {
  const listeners = createContentSearchListeners();
  const listener = vi.fn();
  const unsubscribe = listeners.subscribe("a", listener);
  unsubscribe();
  expect(() => listeners.deliver(matches("a"))).not.toThrow();
  expect(() => listeners.deliver(matches("unheard"))).not.toThrow();
  expect(listener).not.toHaveBeenCalled();
});

it("keeps the other listeners of a search as one of them unsubscribes", () => {
  const listeners = createContentSearchListeners();
  const first = vi.fn();
  const second = vi.fn();
  const unsubscribe = listeners.subscribe("a", first);
  listeners.subscribe("a", second);
  unsubscribe();
  listeners.deliver(ended("a"));
  expect(first).not.toHaveBeenCalled();
  expect(second).toHaveBeenCalledExactlyOnceWith(ended("a"));
});
