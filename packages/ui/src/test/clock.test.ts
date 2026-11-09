// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { framesAsTimers, settleFrames } from "../components/dialog/dialogTestSupport";
import { afterQueuedWork, fakeTimersKeepingFrames, framesFaked, fakeTimers, flushPromises, runOutTimers, runPendingTimers, FAKE_FRAME_MS } from "./clock";

it("installs re-entrantly and runs every beforeDrain before draining and restoring", () => {
  const order: string[] = [];
  onTestFinished(() => {
    expect(vi.isFakeTimers()).toBe(false);
    expect(order).toEqual(["first cleanup", "second cleanup", "timer"]);
  });
  fakeTimers({ beforeDrain: () => order.push("first cleanup") });
  const dropped = vi.fn();
  setTimeout(dropped, FAKE_FRAME_MS);
  fakeTimers({ beforeDrain: () => order.push("second cleanup") });
  expect(vi.getTimerCount()).toBe(0);
  setTimeout(() => { expect(dropped).not.toHaveBeenCalled(); order.push("timer"); }, FAKE_FRAME_MS);
});

it("drains timers and frames armed in turn, leaving zero timers", () => {
  fakeTimers();
  const finished = vi.fn();
  setTimeout(() => requestAnimationFrame(() => setTimeout(finished, FAKE_FRAME_MS)), FAKE_FRAME_MS);
  runOutTimers();
  expect(finished).toHaveBeenCalledOnce();
  expect(vi.getTimerCount()).toBe(0);
});

it("throws when a timer keeps re-arming", () => {
  fakeTimers();
  let timer: ReturnType<typeof setTimeout>;
  const repeat = () => { timer = setTimeout(repeat, FAKE_FRAME_MS); };
  repeat();
  try {
    expect(() => runOutTimers()).toThrow("re-arms itself");
  } finally {
    clearTimeout(timer!);
    runOutTimers();
  }
  expect(vi.getTimerCount()).toBe(0);
});

it("ticks through newly armed timers due before the last pending timer", () => {
  fakeTimers();
  const calls: string[] = [];
  setTimeout(() => { calls.push("first"); setTimeout(() => calls.push("nested"), FAKE_FRAME_MS); }, FAKE_FRAME_MS);
  setTimeout(() => calls.push("last"), FAKE_FRAME_MS * 3);
  runPendingTimers();
  expect(calls).toEqual(["first", "nested", "last"]);
  expect(vi.getTimerCount()).toBe(0);
});

it("lets queued promises finish even while the clock is fake", async () => {
  fakeTimers();
  let finished = false;
  void Promise.resolve().then(() => Promise.resolve()).then(() => { finished = true; });
  await afterQueuedWork();
  expect(finished).toBe(true);
  await flushPromises();
  runOutTimers();
  expect(vi.getTimerCount()).toBe(0);
});

it.each([false, true])("settles a pending focus frame when frames are timer-backed: %s", async (timerBacked) => {
  framesAsTimers();
  if (timerBacked) fakeTimersKeepingFrames();
  else fakeTimers();
  expect(framesFaked()).toBe(!timerBacked);
  const target = document.body.appendChild(document.createElement("button"));
  onTestFinished(() => target.remove());
  act(() => { requestAnimationFrame(() => target.focus()); });
  expect(document.activeElement).not.toBe(target);
  await settleFrames();
  expect(document.activeElement).toBe(target);
  runOutTimers();
  expect(vi.getTimerCount()).toBe(0);
});

it("recognizes missing animation frames without throwing", () => {
  vi.stubGlobal("requestAnimationFrame", undefined);
  onTestFinished(() => { vi.unstubAllGlobals(); });
  expect(framesFaked()).toBe(false);
});
