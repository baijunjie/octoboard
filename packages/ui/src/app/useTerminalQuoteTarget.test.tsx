// @vitest-environment jsdom
import { act, createRef, type RefObject } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, onTestFinished, vi } from "vitest";

import { framesAsTimers } from "../components/dialog/dialogTestSupport";
import { MODAL_CLOSE_SETTLE_MS } from "../components/focus/modalFocus";
import type { QuoteBlock, QuoteTarget } from "../components/host/quoteTarget";
import type { TerminalPaneHandle } from "../terminal/TerminalPane";
import { fakeTimersKeepingFrames } from "../test/clock";
import { useTerminalQuoteTarget } from "./useTerminalQuoteTarget";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => document.body.replaceChildren());

/** The hook mounted over `ref`, with every target it returned, one per render. `render` renders again, with
 * `nextUncoverTerminal` as the `uncoverTerminal` the hook is given from then on, where one is passed. */
async function mountHook(ref: RefObject<TerminalPaneHandle | null>, focusTerminal: () => void = () => {}, uncoverTerminal: () => void = () => {}) {
  const targets: QuoteTarget[] = [];
  let uncover = uncoverTerminal;
  function Probe({ tick }: { tick: number }) {
    void tick;
    targets.push(useTerminalQuoteTarget(ref, focusTerminal, uncover));
    return null;
  }
  const root = createRoot(document.body.appendChild(document.createElement("div")));
  onTestFinished(() => act(() => root.unmount()));
  await act(async () => root.render(<Probe tick={0} />));
  const render = (tick: number, nextUncoverTerminal?: () => void) => {
    if (nextUncoverTerminal) uncover = nextUncoverTerminal;
    return act(async () => root.render(<Probe tick={tick} />));
  };
  return { targets, render };
}

const handle = (block: QuoteBlock | undefined): TerminalPaneHandle => ({ focus: vi.fn(), handOff: vi.fn(), quoteBlock: vi.fn(() => block), quote: vi.fn() });

it("is the same target on every render", async () => {
  const { targets, render } = await mountHook(createRef<TerminalPaneHandle>());
  await render(1);
  await render(2);
  expect(targets.length).toBeGreaterThan(1);
  expect(new Set(targets).size).toBe(1);
});

it("is not blocked while the pane's handle says nothing blocks it, and forwards the text to it", async () => {
  const ref = createRef<TerminalPaneHandle>() as { current: TerminalPaneHandle | null };
  ref.current = handle(undefined);
  const { targets } = await mountHook(ref);
  expect(targets[0].blocked()).toBeUndefined();
  targets[0].quote("some text");
  expect(ref.current.quote).toHaveBeenCalledExactlyOnceWith("some text");
});

it.each(["input", "program"] as const)("is blocked as the pane's handle says, by the %s", async (block) => {
  const ref = createRef<TerminalPaneHandle>() as { current: TerminalPaneHandle | null };
  ref.current = handle(block);
  const { targets } = await mountHook(ref);
  expect(targets[0].blocked()).toBe(block);
});

it("is blocked by the input, and quotes nothing, while the ref holds no handle", async () => {
  const { targets } = await mountHook(createRef<TerminalPaneHandle>());
  expect(targets[0].blocked()).toBe("input");
  expect(() => targets[0].quote("text")).not.toThrow();
});

it("reads the handle when asked, so a handle set or swapped after the first render is the one used", async () => {
  const ref = createRef<TerminalPaneHandle>() as { current: TerminalPaneHandle | null };
  const { targets, render } = await mountHook(ref);
  expect(targets[0].blocked()).toBe("input");
  const first = handle(undefined);
  ref.current = first;
  expect(targets[0].blocked()).toBeUndefined();
  const second = handle("program");
  ref.current = second;
  await render(1);
  expect(targets[0].blocked()).toBe("program");
  targets[0].quote("late");
  expect(first.quote).not.toHaveBeenCalled();
  expect(second.quote).toHaveBeenCalledExactlyOnceWith("late");
});

describe("revealInput", () => {
  /** Frames asked for as 0 ms timers on a fake clock, as a dialog's own focus handoff is tested; the clock is started
   * before the hook is mounted. A frame asked for inside a frame is due a millisecond later, so one advance of this
   * length runs a chain far past the deadline `MODAL_CLOSE_SETTLE_MS` sets. */
  const MANY_FRAMES_MS = 1000;
  const frames = (ms = MANY_FRAMES_MS) => act(() => void vi.advanceTimersByTime(ms));
  /** A few frames, well short of the deadline, with the dialog still there. */
  const FEW_FRAMES_MS = 10;

  async function mountFocusing() {
    framesAsTimers();
    fakeTimersKeepingFrames();
    const focusTerminal = vi.fn();
    const uncoverTerminal = vi.fn();
    const { targets, render } = await mountHook(createRef<TerminalPaneHandle>(), focusTerminal, uncoverTerminal);
    return { target: targets[0], targets, render, focusTerminal, uncoverTerminal };
  }

  function dialog(attributes: Record<string, string> = {}) {
    const element = document.body.appendChild(document.createElement("div"));
    element.setAttribute("role", "dialog");
    for (const [name, value] of Object.entries(attributes)) element.setAttribute(name, value);
    return element;
  }

  it("uncovers the terminal at once, once, before any frame", async () => {
    const { target, uncoverTerminal } = await mountFocusing();
    dialog();
    target.revealInput();
    expect(uncoverTerminal).toHaveBeenCalledOnce();
    frames(FEW_FRAMES_MS);
    expect(uncoverTerminal).toHaveBeenCalledOnce();
  });

  it("uncovers with the latest render's `uncoverTerminal`, not the first's, on the one target", async () => {
    const { target, targets, render, uncoverTerminal: first } = await mountFocusing();
    const second = vi.fn();
    await render(1, second);
    expect(new Set(targets).size).toBe(1);
    target.revealInput();
    expect(second).toHaveBeenCalledOnce();
    expect(first).not.toHaveBeenCalled();
  });

  it("focuses the terminal on a frame after it is asked, not at once", async () => {
    const { target, focusTerminal } = await mountFocusing();
    target.revealInput();
    expect(focusTerminal).not.toHaveBeenCalled();
    frames();
    expect(focusTerminal).toHaveBeenCalledOnce();
  });

  it("waits for a modal dialog to leave the document, though it is gone from view, and focuses once", async () => {
    const { target, focusTerminal } = await mountFocusing();
    const closing = dialog({ "aria-modal": "true" });
    target.revealInput();
    frames(FEW_FRAMES_MS);
    expect(focusTerminal).not.toHaveBeenCalled();
    closing.remove();
    frames();
    expect(focusTerminal).toHaveBeenCalledOnce();
  });

  it("does not wait for a dialog that is not modal", async () => {
    const { target, focusTerminal } = await mountFocusing();
    dialog({ "aria-modal": "false" });
    target.revealInput();
    frames();
    expect(focusTerminal).toHaveBeenCalledOnce();
  });

  it("drops a pending focus when asked again, so the terminal is focused once", async () => {
    const { target, focusTerminal } = await mountFocusing();
    const closing = dialog();
    target.revealInput();
    frames(FEW_FRAMES_MS);
    target.revealInput();
    closing.remove();
    frames();
    expect(focusTerminal).toHaveBeenCalledOnce();
  });

  it("gives up without focusing when another modal dialog is left once the one it was called for has gone", async () => {
    const { target, focusTerminal } = await mountFocusing();
    dialog();
    const closing = dialog();
    target.revealInput();
    closing.remove();
    frames();
    expect(focusTerminal).not.toHaveBeenCalled();
  });

  it("waits for the foremost dialog that is not inert, not for one inside an inert container", async () => {
    const { target, focusTerminal } = await mountFocusing();
    const closing = dialog();
    const behind = document.body.appendChild(document.createElement("div"));
    behind.setAttribute("inert", "");
    behind.appendChild(document.createElement("div")).setAttribute("role", "dialog");
    target.revealInput();
    behind.remove();
    frames(FEW_FRAMES_MS);
    closing.remove();
    frames();
    expect(focusTerminal).toHaveBeenCalledOnce();
  });

  // A frame is 1 ms on this clock, so the margin leaves the deadline's own frame out of either side.
  const DEADLINE_MARGIN_MS = 10;
  it.each([
    ["still waits just before the deadline, and focuses once the dialog goes", MODAL_CLOSE_SETTLE_MS - DEADLINE_MARGIN_MS, 1],
    ["has given up at the deadline, and leaves focus alone when the dialog goes later", MODAL_CLOSE_SETTLE_MS + DEADLINE_MARGIN_MS, 0],
  ])("%s", async (_name, elapsedMs, focused) => {
    const { target, focusTerminal } = await mountFocusing();
    const stuck = dialog();
    target.revealInput();
    frames(elapsedMs);
    expect(focusTerminal).not.toHaveBeenCalled();
    stuck.remove();
    frames(MANY_FRAMES_MS);
    expect(focusTerminal).toHaveBeenCalledTimes(focused);
  });
});
