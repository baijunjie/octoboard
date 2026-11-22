// @vitest-environment jsdom
import { act, createRef, type RefObject } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import type { QuoteBlock, QuoteTarget } from "../components/host/quoteTarget";
import type { TerminalPaneHandle } from "../terminal/TerminalPane";
import { useTerminalQuoteTarget } from "./useTerminalQuoteTarget";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => document.body.replaceChildren());

/** The hook mounted over `ref`, with every target it returned, one per render. */
async function mountHook(ref: RefObject<TerminalPaneHandle | null>) {
  const targets: QuoteTarget[] = [];
  function Probe({ tick }: { tick: number }) {
    void tick;
    targets.push(useTerminalQuoteTarget(ref));
    return null;
  }
  const root = createRoot(document.body.appendChild(document.createElement("div")));
  onTestFinished(() => act(() => root.unmount()));
  await act(async () => root.render(<Probe tick={0} />));
  const render = (tick: number) => act(async () => root.render(<Probe tick={tick} />));
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
