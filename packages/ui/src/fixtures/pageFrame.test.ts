// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { afterFrames, applyDocumentTheme } from "./pageFrame";

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

it("sets the theme class and attribute, replacing a class the page was served with", () => {
  document.documentElement.className = "dark";
  applyDocumentTheme(document, "light");
  expect(document.documentElement.className).toBe("light");
  expect(document.documentElement.dataset.theme).toBe("light");
});

it("resolves two frames on, without waiting for the cap", async () => {
  const frames: FrameRequestCallback[] = [];
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => frames.push(callback));
  const done = vi.fn();
  void afterFrames(200).then(done);
  frames.shift()?.(0);
  frames.shift()?.(0);
  await vi.advanceTimersByTimeAsync(0);
  expect(done).toHaveBeenCalledOnce();
  expect(vi.getTimerCount()).toBe(0);
});

it("resolves at the cap when the page draws no frames", async () => {
  vi.stubGlobal("requestAnimationFrame", () => 0);
  const stalled = vi.fn();
  void afterFrames(200).then(stalled);
  await vi.advanceTimersByTimeAsync(199);
  expect(stalled).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(1);
  expect(stalled).toHaveBeenCalledOnce();
  expect(vi.getTimerCount()).toBe(0);
});
