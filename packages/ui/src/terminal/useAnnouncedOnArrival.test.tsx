// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";

import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { windowAttention } from "../components/announce/windowTestSupport";
import { fakeTimers } from "../test/clock";
import { useAnnouncedOnArrival } from "./useAnnouncedOnArrival";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

type Props = { subject?: string; keys: readonly string[]; deferred?: boolean };

/** The sentence a caller words from the newest item, as the terminal pane words the newest standing notice's. */
const sentenceOf = (keys: readonly string[]) => (keys.length > 0 ? `Notice ${keys[keys.length - 1]}` : "");

function Probe({ subject = "s", keys, deferred }: Props): string {
  return useAnnouncedOnArrival(subject, keys, sentenceOf(keys), deferred) ?? "";
}

/** Mounts the probe on a fake clock started before the hook arms its timer; whatever is still pending is run out
 * and counted before the real clock comes back. */
function setup(initial: Props) {
  windowAttention();
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  fakeTimers({ beforeDrain: () => { root.unmount(); host.remove(); } });
  const render = (props: Props) => act(() => root.render(<Probe {...props} />));
  const advance = (ms: number) => act(() => void vi.advanceTimersByTime(ms));
  render(initial);
  return { render, advance, said: () => host.textContent };
}

it("says nothing of the items found as the view opens", () => {
  const { advance, said } = setup({ keys: ["a", "b"] });
  advance(AFTER_FOCUS_MOVE_DELAY_MS * 2);
  expect(said()).toBe("");
  expect(vi.getTimerCount()).toBe(0);
});

it("says an item that arrives afterwards, once the focus-move wait of attended time has passed", () => {
  const { render, advance, said } = setup({ keys: ["a"] });
  render({ keys: ["a", "b"] });
  expect(said()).toBe("");
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  expect(said()).toBe("");
  advance(1);
  expect(said()).toBe("Notice b");
});

it("says only the newest when several arrive before one is said", () => {
  const { render, advance, said } = setup({ keys: ["a"] });
  render({ keys: ["a", "b"] });
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  render({ keys: ["a", "b", "c"] });
  advance(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(said()).toBe("Notice c");
  advance(AFTER_FOCUS_MOVE_DELAY_MS * 2);
  expect(said()).toBe("Notice c");
});

it("says an arrival once: the same items published again, or another one withdrawn, say nothing more", () => {
  const { render, advance, said } = setup({ keys: ["a"] });
  render({ keys: ["a", "b"] });
  advance(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(said()).toBe("Notice b");
  render({ keys: ["a", "b"] });
  expect(vi.getTimerCount()).toBe(0);
  expect(said()).toBe("Notice b");
  // A notice withdrawn that is not the one said leaves it as it was, with nothing queued.
  render({ keys: ["b"] });
  expect(vi.getTimerCount()).toBe(0);
  expect(said()).toBe("Notice b");
});

it("drops an arrival that leaves the items before it is said", () => {
  const { render, advance, said } = setup({ keys: [] });
  render({ keys: ["a"] });
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  render({ keys: [] });
  advance(AFTER_FOCUS_MOVE_DELAY_MS * 2);
  expect(said()).toBe("");
  expect(vi.getTimerCount()).toBe(0);
});

it("empties once the item that was said is dismissed or withdrawn", () => {
  const { render, advance, said } = setup({ keys: [] });
  render({ keys: ["a"] });
  advance(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(said()).toBe("Notice a");
  render({ keys: [] });
  expect(said()).toBe("");
});

it("says nothing of the items of another subject, and drops what was queued for the one it left", () => {
  const { render, advance, said } = setup({ subject: "s", keys: ["a"] });
  render({ subject: "s", keys: ["a", "b"] });
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  render({ subject: "t", keys: ["x"] });
  advance(AFTER_FOCUS_MOVE_DELAY_MS * 2);
  expect(said()).toBe("");
  expect(vi.getTimerCount()).toBe(0);
  // An arrival for the subject now shown is said.
  render({ subject: "t", keys: ["x", "y"] });
  advance(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(said()).toBe("Notice y");
});

it("holds an arrival while deferred and says it once the deferral ends", () => {
  const { render, advance, said } = setup({ keys: ["a"], deferred: true });
  render({ keys: ["a", "b"], deferred: true });
  advance(AFTER_FOCUS_MOVE_DELAY_MS * 5);
  expect(said()).toBe("");
  render({ keys: ["a", "b"], deferred: false });
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  expect(said()).toBe("");
  advance(1);
  expect(said()).toBe("Notice b");
});

it("does not queue an arrival it has said again when a deferral ends", () => {
  const { render, advance, said } = setup({ keys: ["a"] });
  render({ keys: ["a", "b"] });
  advance(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(said()).toBe("Notice b");
  render({ keys: ["a", "b"], deferred: true });
  render({ keys: ["a", "b"], deferred: false });
  expect(vi.getTimerCount()).toBe(0);
  expect(said()).toBe("Notice b");
});
