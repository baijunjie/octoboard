// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { POLL_MS, usePolled, type ProcessPolling } from "./usePolled";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  Reflect.deleteProperty(document, "hidden");
});

async function mount(fetch: (key: string) => Promise<number[]>, options?: { stopsOnFirstFailure?: boolean }) {
  vi.useFakeTimers();
  const ask = vi.fn(fetch);
  let pace: ProcessPolling = "live";
  let data!: ReturnType<typeof usePolled<number[]>>;
  let renders = 0;
  function Probe() {
    renders++;
    data = usePolled("k", ask, pace, options);
    return null;
  }
  const element = document.body.appendChild(document.createElement("div"));
  const root = createRoot(element);
  const render = async () => act(async () => root.render(<Probe />));
  await render();
  onTestFinished(() => { act(() => root.unmount()); element.remove(); vi.useRealTimers(); });
  return {
    ask,
    data: () => data,
    renders: () => renders,
    pace: async (next: ProcessPolling) => { pace = next; await render(); },
    advance: (ms: number) => act(async () => void vi.advanceTimersByTime(ms)),
  };
}

it("asks at the pace it is given, and again at the new one when the pace changes", async () => {
  const poll = await mount(async () => [1]);
  expect(poll.ask).toHaveBeenCalledTimes(1);
  await poll.advance(POLL_MS.live);
  expect(poll.ask).toHaveBeenCalledTimes(2);
  await poll.pace("background");
  expect(poll.ask).toHaveBeenCalledTimes(3);
  await poll.advance(POLL_MS.live);
  expect(poll.ask).toHaveBeenCalledTimes(3);
  await poll.advance(POLL_MS.background - POLL_MS.live);
  expect(poll.ask).toHaveBeenCalledTimes(4);
  await poll.pace("off");
  await poll.advance(POLL_MS.background * 2);
  expect(poll.ask).toHaveBeenCalledTimes(4);
});

it("does not ask while the document is hidden, and asks once when it is shown", async () => {
  const poll = await mount(async () => [1]);
  Object.defineProperty(document, "hidden", { value: true, configurable: true });
  await poll.advance(POLL_MS.live * 3);
  expect(poll.ask).toHaveBeenCalledTimes(1);
  Object.defineProperty(document, "hidden", { value: false, configurable: true });
  await act(async () => void document.dispatchEvent(new Event("visibilitychange")));
  expect(poll.ask).toHaveBeenCalledTimes(2);
});

it("publishes only a changed answer", async () => {
  let answer = [1];
  const poll = await mount(async () => answer);
  const renders = poll.renders();
  await poll.advance(POLL_MS.live);
  expect(poll.renders()).toBe(renders);
  answer = [1, 2];
  await poll.advance(POLL_MS.live);
  expect(poll.data().value).toEqual([1, 2]);
});

it("keeps the last answer after a failure, publishes an unchanged failure once, and clears it on recovery", async () => {
  let failing = false;
  const poll = await mount(async () => {
    if (failing) throw new Error("offline");
    return [1];
  });
  failing = true;
  await poll.advance(POLL_MS.live);
  expect(poll.data().value).toEqual([1]);
  expect(poll.data().error).toBeDefined();
  const renders = poll.renders();
  await poll.advance(POLL_MS.live);
  expect(poll.renders()).toBe(renders);
  failing = false;
  await poll.advance(POLL_MS.live);
  expect(poll.data().error).toBeUndefined();
  expect(poll.data().value).toEqual([1]);
});

it("keeps asking after a first failure, unless it stops there until retried", async () => {
  const keeps = await mount(async () => { throw new Error("offline"); });
  await keeps.advance(POLL_MS.live);
  expect(keeps.ask).toHaveBeenCalledTimes(2);

  const stops = await mount(async () => { throw new Error("offline"); }, { stopsOnFirstFailure: true });
  await stops.advance(POLL_MS.live * 3);
  expect(stops.ask).toHaveBeenCalledTimes(1);
  await act(async () => stops.data().retry());
  expect(stops.ask).toHaveBeenCalledTimes(2);
});

it("revises the answer at once and asks again", async () => {
  let calls = 0;
  const poll = await mount(() => (++calls === 1 ? Promise.resolve([1, 2]) : new Promise<number[]>(() => {})));
  await act(async () => poll.data().revise((value) => value?.filter((item) => item !== 1)));
  expect(poll.data().value).toEqual([2]);
  expect(poll.ask).toHaveBeenCalledTimes(2);
});
