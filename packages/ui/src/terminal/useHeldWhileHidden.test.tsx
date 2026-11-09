// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { useHeldWhileHidden } from "./useHeldWhileHidden";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

type Props = { sentence: string | undefined; hidden: boolean; stillDue?: () => boolean };

function Probe({ sentence, hidden, stillDue }: Props): string {
  return useHeldWhileHidden(sentence, hidden, { stillDue }) ?? "";
}

/** Mounts the probe on a fake clock started before the hook arms its timer; whatever is still pending is run out
 * and counted before the real clock comes back. */
function setup(initial: Props) {
  vi.useFakeTimers();
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  onTestFinished(() => {
    act(() => root.unmount());
    host.remove();
    act(() => void vi.runOnlyPendingTimers());
    expect(vi.getTimerCount()).toBe(0);
    vi.useRealTimers();
  });
  const render = (props: Props) => act(() => root.render(<Probe {...props} />));
  const advance = (ms: number) => act(() => void vi.advanceTimersByTime(ms));
  render(initial);
  return { host, render, advance, out: () => host.textContent };
}

it("passes the sentence through while it is not hidden", () => {
  const { render, out } = setup({ sentence: "One", hidden: false });
  expect(out()).toBe("One");
  render({ sentence: "Two", hidden: false });
  expect(out()).toBe("Two");
  render({ sentence: undefined, hidden: false });
  expect(out()).toBe("");
});

it("gives nothing for a sentence that arrives while hidden, and writes it once the wait after exposure has passed", () => {
  const { render, advance, out } = setup({ sentence: "One", hidden: true });
  expect(out()).toBe("");
  advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
  expect(out()).toBe("");
  render({ sentence: "One", hidden: false });
  expect(out()).toBe("");
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  expect(out()).toBe("");
  advance(1);
  expect(out()).toBe("One");
});

it("holds a sentence that changes while hidden, and writes the one current at exposure, not each change", () => {
  const { render, advance, out } = setup({ sentence: "One", hidden: false });
  render({ sentence: "One", hidden: true });
  render({ sentence: "Two", hidden: true });
  expect(out()).toBe("");
  render({ sentence: "Three", hidden: true });
  render({ sentence: "Three", hidden: false });
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  expect(out()).toBe("");
  advance(1);
  expect(out()).toBe("Three");
});

it("keeps a sentence already written through a hidden period without clearing it, and does not write it again", () => {
  const { host, render, advance, out } = setup({ sentence: "One", hidden: false });
  const history: string[] = [];
  const observer = new MutationObserver(() => history.push(host.textContent ?? ""));
  observer.observe(host, { childList: true, characterData: true, subtree: true });
  onTestFinished(() => observer.disconnect());
  render({ sentence: "One", hidden: true });
  expect(out()).toBe("One");
  advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
  render({ sentence: "One", hidden: false });
  advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
  expect(out()).toBe("One");
  expect(observer.takeRecords()).toEqual([]);
  expect(history).toEqual([]);
});

it("does not replay a sentence written after exposure when hidden and exposed again", () => {
  const { render, advance, out } = setup({ sentence: "One", hidden: true });
  render({ sentence: "One", hidden: false });
  advance(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(out()).toBe("One");
  render({ sentence: "One", hidden: true });
  render({ sentence: "One", hidden: false });
  advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
  expect(out()).toBe("One");
});

it("does not write a sentence that went stale while hidden", () => {
  const { render, advance, out } = setup({ sentence: "One", hidden: true });
  render({ sentence: undefined, hidden: true });
  render({ sentence: undefined, hidden: false });
  advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
  expect(out()).toBe("");
});

it("does not write a held sentence when it goes stale during the wait after exposure", () => {
  const { render, advance, out } = setup({ sentence: "One", hidden: true });
  render({ sentence: "One", hidden: false });
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  render({ sentence: undefined, hidden: false });
  advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
  expect(out()).toBe("");
});

it("restarts the wait when it is hidden again before the wait after exposure ends", () => {
  const { render, advance, out } = setup({ sentence: "One", hidden: true });
  render({ sentence: "One", hidden: false });
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  render({ sentence: "One", hidden: true });
  advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
  expect(out()).toBe("");
  render({ sentence: "One", hidden: false });
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  expect(out()).toBe("");
  advance(1);
  expect(out()).toBe("One");
});

it("takes a sentence that arrives after exposure at once, when none was held", () => {
  const { render, out } = setup({ sentence: undefined, hidden: true });
  render({ sentence: undefined, hidden: false });
  render({ sentence: "One", hidden: false });
  expect(out()).toBe("One");
});

it("counts a held sentence as delivered, unwritten, when stillDue says no as the wait ends", () => {
  let due = true;
  const stillDue = () => due;
  const { render, advance, out } = setup({ sentence: undefined, hidden: true, stillDue });
  render({ sentence: "One", hidden: true, stillDue });
  render({ sentence: "One", hidden: false, stillDue });
  due = false;
  advance(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(out()).toBe("");
  // Nor written on a later exposure, once it would be due again.
  due = true;
  render({ sentence: "One", hidden: true, stillDue });
  render({ sentence: "One", hidden: false, stillDue });
  advance(AFTER_FOCUS_MOVE_DELAY_MS * 2);
  expect(out()).toBe("");
  // A new sentence is a new announcement.
  render({ sentence: "Two", hidden: false, stillDue });
  expect(out()).toBe("Two");
});
