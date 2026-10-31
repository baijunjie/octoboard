// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";

import { useFocusHandoff } from "./useFocusHandoff";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function Host({ shown, focusTerminal, disabled }: { shown: boolean; focusTerminal: () => void; disabled?: boolean }) {
  const held = useFocusHandoff(focusTerminal);
  return shown ? (
    <span {...held}>
      <button disabled={disabled}>control</button>
    </span>
  ) : null;
}

function run(shown: boolean[], focus: boolean): number {
  const focusTerminal = vi.fn();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<Host shown focusTerminal={focusTerminal} />));
  if (focus) act(() => container.querySelector("button")?.focus());
  for (const s of shown) act(() => root.render(<Host shown={s} focusTerminal={focusTerminal} />));
  act(() => root.unmount());
  container.remove();
  return focusTerminal.mock.calls.length;
}

it("hands focus on when the focused control goes away, even after re-renders", () => {
  expect(run([true, true, false], true)).toBe(1);
});

it("does nothing when the control did not hold focus", () => {
  expect(run([false], false)).toBe(0);
});

function runDisabling(focus: boolean): number {
  const focusTerminal = vi.fn();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<Host shown focusTerminal={focusTerminal} />));
  const button = container.querySelector("button")!;
  if (focus) act(() => button.focus());
  // A browser blurs a control as it is disabled; jsdom does not, so it is done by hand.
  act(() => {
    button.disabled = true;
    button.blur();
  });
  act(() => root.render(<Host shown disabled focusTerminal={focusTerminal} />));
  act(() => root.unmount());
  container.remove();
  return focusTerminal.mock.calls.length;
}

it("hands focus on when the focused control is disabled and blurred", () => {
  expect(runDisabling(true)).toBe(1);
});

it("does nothing when a control that never held focus is disabled", () => {
  expect(runDisabling(false)).toBe(0);
});
