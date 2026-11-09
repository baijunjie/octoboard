// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { windowAttention } from "../components/announce/windowTestSupport";
import { useAttendedDelivery } from "./useAttendedDelivery";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const SENTENCE = "Recovered terminal display";

type Props = { id: string | undefined; sentence?: string; stillDue?: () => boolean };

function Probe({ id, sentence = SENTENCE, stillDue }: Props): string {
  return useAttendedDelivery(id, sentence, { stillDue }) ?? "";
}

/** Mounts the probe on a fake clock started before the hook arms its timer; whatever is still pending is run out
 * and counted before the real clock comes back. */
function setup(initial: Props) {
  vi.useFakeTimers();
  const setAttended = windowAttention();
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
  return { render, advance, setAttended, said: () => host.textContent };
}

it("gives nothing until the focus-move wait of attended time has passed, then the sentence", () => {
  const { advance, said } = setup({ id: "a" });
  expect(said()).toBe("");
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  expect(said()).toBe("");
  advance(1);
  expect(said()).toBe(SENTENCE);
});

it("keeps the sentence once given, across the user leaving and coming back", () => {
  const { advance, setAttended, said } = setup({ id: "a" });
  advance(AFTER_FOCUS_MOVE_DELAY_MS);
  setAttended(false);
  expect(said()).toBe(SENTENCE);
  setAttended(true);
  advance(AFTER_FOCUS_MOVE_DELAY_MS * 2);
  expect(said()).toBe(SENTENCE);
});

it("queues nothing for no key", () => {
  const { render, advance, said } = setup({ id: undefined });
  expect(vi.getTimerCount()).toBe(0);
  advance(AFTER_FOCUS_MOVE_DELAY_MS * 2);
  expect(said()).toBe("");
  render({ id: undefined, sentence: "Another sentence" });
  expect(said()).toBe("");
});

it("writes nothing while the window is not attended, and waits the full time again once it is", () => {
  const { advance, setAttended, said } = setup({ id: "a" });
  setAttended(false);
  advance(AFTER_FOCUS_MOVE_DELAY_MS * 10);
  expect(said()).toBe("");
  setAttended(true);
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  expect(said()).toBe("");
  advance(1);
  expect(said()).toBe(SENTENCE);
});

it("starts the wait over when the user leaves and comes back before it ends", () => {
  const { advance, setAttended, said } = setup({ id: "a" });
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  setAttended(false);
  advance(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(said()).toBe("");
  setAttended(true);
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  expect(said()).toBe("");
  advance(1);
  expect(said()).toBe(SENTENCE);
});

it("counts a hidden document as the user being away", () => {
  let visibility: DocumentVisibilityState = "visible";
  const visible = vi.spyOn(document, "visibilityState", "get").mockImplementation(() => visibility);
  onTestFinished(() => visible.mockRestore());
  const { advance, said } = setup({ id: "a" });
  const changeVisibility = (next: DocumentVisibilityState) => act(() => {
    visibility = next;
    document.dispatchEvent(new Event("visibilitychange"));
  });
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  changeVisibility("hidden");
  advance(AFTER_FOCUS_MOVE_DELAY_MS * 2);
  expect(said()).toBe("");
  changeVisibility("visible");
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  expect(said()).toBe("");
  advance(1);
  expect(said()).toBe(SENTENCE);
});

it("drops an announcement unsaid when its key goes away before it is due", () => {
  const { render, advance, said } = setup({ id: "a" });
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  render({ id: undefined });
  advance(AFTER_FOCUS_MOVE_DELAY_MS * 2);
  expect(said()).toBe("");
});

it("takes the sentence back at once when the key goes away after it was given", () => {
  const { render, advance, said } = setup({ id: "a" });
  advance(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(said()).toBe(SENTENCE);
  render({ id: undefined });
  expect(said()).toBe("");
});

it("empties at once on a switch to another key with the same sentence, and writes it again after its own wait", () => {
  const { render, advance, said } = setup({ id: "a" });
  advance(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(said()).toBe(SENTENCE);
  render({ id: "b" });
  expect(said()).toBe("");
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  expect(said()).toBe("");
  advance(1);
  expect(said()).toBe(SENTENCE);
});

it("does not carry the wait of the old key over to the new one", () => {
  const { render, advance, said } = setup({ id: "a" });
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  render({ id: "b" });
  advance(1);
  expect(said()).toBe("");
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  expect(said()).toBe(SENTENCE);
});

it("says a key that went away and came back again, as a fresh announcement", () => {
  const { render, advance, said } = setup({ id: "a" });
  advance(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(said()).toBe(SENTENCE);
  render({ id: undefined });
  expect(said()).toBe("");
  render({ id: "a" });
  expect(said()).toBe("");
  advance(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(said()).toBe(SENTENCE);
});

it("follows a change of sentence under the same key", () => {
  const { render, advance, said } = setup({ id: "a" });
  advance(AFTER_FOCUS_MOVE_DELAY_MS);
  render({ id: "a", sentence: "Another sentence" });
  expect(said()).toBe("Another sentence");
});

it("does not write, nor queue again, when the announcement is no longer due as the wait ends", () => {
  let due = false;
  const { render, advance, setAttended, said } = setup({ id: "a", stillDue: () => due });
  advance(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(said()).toBe("");
  // Nothing is left to fire, and leaving and coming back does not queue it for the same key again.
  expect(vi.getTimerCount()).toBe(0);
  due = true;
  setAttended(false);
  setAttended(true);
  expect(vi.getTimerCount()).toBe(0);
  advance(AFTER_FOCUS_MOVE_DELAY_MS * 2);
  expect(said()).toBe("");
  render({ id: "a", stillDue: () => true });
  advance(AFTER_FOCUS_MOVE_DELAY_MS * 2);
  expect(said()).toBe("");
});

it("reads whether it is still due as the wait ends, not as it was queued", () => {
  let due = true;
  const { advance, said } = setup({ id: "a", stillDue: () => due });
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  due = false;
  advance(1);
  expect(said()).toBe("");
});

it("uses the latest condition a re-render passed, not the one the wait started with", () => {
  const { render, advance, said } = setup({ id: "a", stillDue: () => true });
  advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  render({ id: "a", stillDue: () => false });
  advance(1);
  expect(said()).toBe("");
});

it("says again for the same key once it went away, even after it had settled as not due", () => {
  const { render, advance, said } = setup({ id: "a", stillDue: () => false });
  advance(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(said()).toBe("");
  render({ id: undefined });
  render({ id: "a", stillDue: () => true });
  advance(AFTER_FOCUS_MOVE_DELAY_MS);
  expect(said()).toBe(SENTENCE);
});
