// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { type ConsoleOwnerRule, PRESS_SETTLE_MS, useConsoleOwnerRule } from "./useConsoleOwnerRule";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root | undefined;

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  act(() => root?.unmount());
  vi.useRealTimers();
  document.body.replaceChildren();
  root = undefined;
});

/** Mounts the hook on console `c-a`; `show` makes another console current. */
function mount() {
  const apply = vi.fn();
  let latest!: ConsoleOwnerRule;
  function Probe({ consoleId }: { consoleId: string }) {
    latest = useConsoleOwnerRule(consoleId, apply);
    return null;
  }
  root = createRoot(document.body.appendChild(document.createElement("div")));
  const show = (consoleId: string) => act(() => root!.render(<Probe consoleId={consoleId} />));
  show("c-a");
  apply.mockClear();
  return { apply, show, rule: () => latest };
}
const wait = (ms: number) => act(() => void vi.advanceTimersByTime(ms));
const release = () => act(() => void window.dispatchEvent(new Event("pointerup")));

it("applies the rule at once when another console is made current without a press", () => {
  const { apply, show } = mount();
  show("c-b");
  expect(apply).toHaveBeenCalledTimes(1);
});

it("holds the rule for a press until it has ended and settled", () => {
  const { apply, show, rule } = mount();
  act(() => rule().holdForPress());
  show("c-b");
  expect(apply).not.toHaveBeenCalled();
  release();
  wait(PRESS_SETTLE_MS - 1);
  expect(apply).not.toHaveBeenCalled();
  wait(1);
  expect(apply).toHaveBeenCalledTimes(1);
});

it("drops a held rule when the owner was set during the press", () => {
  const { apply, show, rule } = mount();
  act(() => rule().holdForPress());
  show("c-b");
  release();
  // The click that selects a row comes after the release.
  act(() => rule().ownerSet());
  wait(PRESS_SETTLE_MS);
  expect(apply).not.toHaveBeenCalled();
  show("c-a");
  expect(apply).toHaveBeenCalledTimes(1);
});

it("ends a press the window lost focus in, with no release", () => {
  const { apply, show, rule } = mount();
  act(() => rule().holdForPress());
  show("c-b");
  act(() => void window.dispatchEvent(new Event("blur")));
  wait(PRESS_SETTLE_MS);
  expect(apply).toHaveBeenCalledTimes(1);
});

it("does nothing at the end of a press that changed no console", () => {
  const { apply, rule } = mount();
  act(() => rule().holdForPress());
  release();
  wait(PRESS_SETTLE_MS);
  expect(apply).not.toHaveBeenCalled();
});
