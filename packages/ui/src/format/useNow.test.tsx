// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { TICK_MS, useNow } from "./useNow";
import { fakeTimers } from "../test/clock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

beforeEach(() => {
  fakeTimers();
  vi.setSystemTime(1_000_000);
});

function Probe(): null {
  seen.push(useNow());
  return null;
}
const seen: number[] = [];

function mount() {
  seen.length = 0;
  const root = createRoot(document.createElement("div"));
  act(() => root.render(createElement(Probe)));
  onTestFinished(() => act(() => root.unmount()));
  return root;
}

it("moves its value on the clock's tick", () => {
  mount();
  expect(seen.at(-1)).toBe(1_000_000);
  act(() => void vi.advanceTimersByTime(TICK_MS));
  expect(seen.at(-1)).toBe(1_000_000 + TICK_MS);
});

it("runs one timer for every component using it, until the last unmounts", () => {
  const first = mount();
  const second = mount();
  expect(vi.getTimerCount()).toBe(1);
  act(() => first.unmount());
  expect(vi.getTimerCount()).toBe(1);
  act(() => second.unmount());
  expect(vi.getTimerCount()).toBe(0);
});

it("starts from the clock again once nothing has used it", () => {
  const root = mount();
  act(() => root.unmount());
  vi.setSystemTime(1_600_000);
  mount();
  expect(seen.at(-1)).toBe(1_600_000);
});
