// Shared by the tests of the tooltips an indicator carries: driving the pointer the way react-aria
// wants it, mounting a component that has one, and running its delay out.
import type React from "react";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { onTestFinished, vi } from "vitest";

/**
 * A pointer event of a real mouse. React raises `onPointerEnter` / `onPointerLeave` from
 * `pointerover` / `pointerout`, and react-aria opens a hover tooltip only once it has seen the
 * pointer move, so a move comes before the one that enters.
 *
 * The gallery's own hover step repeats this rather than importing it: it runs inside the gallery's
 * browser page, which cannot pull in `vitest`.
 */
export function pointer(type: "pointermove" | "pointerover" | "pointerout"): MouseEvent {
  const event = new MouseEvent(type, { bubbles: true });
  Object.defineProperty(event, "pointerType", { value: "mouse" });
  return event;
}

/** Puts the pointer on `element`; the tooltip then opens once its delay has run out. */
export function enter(element: Element): void {
  act(() => {
    element.dispatchEvent(pointer("pointermove"));
    element.dispatchEvent(pointer("pointerover"));
  });
}

/** Takes the pointer off `element`, which closes an open tooltip at once. */
export function leave(element: Element): void {
  act(() => void element.dispatchEvent(pointer("pointerout")));
}

/**
 * A press of `element` by `pointerType`, a finger by default: the pointer events react-aria's
 * `usePress` reads, then the `click` that follows. The box has a size, since react-aria reads a
 * pointer event of no size as a virtual one.
 *
 * After a touch press (not a pen's) react-aria ignores the mouse's hover for half a second (the
 * emulated mouse events iOS sends), which would swallow the next hover, this test's or the next
 * test's. The wait is run out on the fake clock where there is one, and otherwise when the test
 * ends.
 */
export function press(element: Element, pointerType: "mouse" | "touch" | "pen" = "touch"): void {
  const init = { bubbles: true, cancelable: true, button: 0, pointerId: 1, isPrimary: true, pointerType, width: 20, height: 20 };
  act(() => {
    element.dispatchEvent(new PointerEvent("pointerdown", init));
    element.dispatchEvent(new PointerEvent("pointerup", init));
    element.dispatchEvent(new MouseEvent("click", { bubbles: true, button: 0, detail: 1 }));
  });
  if (pointerType !== "touch") return;
  if (vi.isFakeTimers()) act(() => void vi.advanceTimersByTime(600));
  else onTestFinished(() => new Promise((resolve) => setTimeout(resolve, 600)));
}

/** A click with no pointer behind it, which react-aria reads as a virtual press: what assistive
 * technology activating a control, or `element.click()`, sends. */
export function activate(element: HTMLElement): void {
  act(() => element.click());
}

/** A key pressed and released on `element`, as a keyboard press of a control is. */
export function tap(element: Element, key: "Enter" | " "): void {
  act(() => {
    element.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
    element.dispatchEvent(new KeyboardEvent("keyup", { key, bubbles: true }));
  });
}

/** The touch popover, or `null` while none is open. */
export function openPopover(): Element | null {
  return document.querySelector('[role="dialog"]');
}

/** The one tooltip that can be open at a time, or `null` while none is. */
export function openTooltip(): Element | null {
  return document.querySelector('[role="tooltip"]');
}

/**
 * Renders `element` into a container of its own and returns that container. The root is unmounted
 * when the test ends, not merely taken out of the document: the unmount runs the effect cleanups,
 * which is what stops a tooltip's delay still counting down after the test.
 */
export function mountForTooltip(element: React.ReactElement): HTMLElement {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(element));
  return container;
}

/**
 * Fake timers for the length of the test, so a tooltip's delay can be run out and nothing can be
 * left counting down. Returns the call that runs the pending timers out.
 */
export function withFakeTimers(): () => void {
  vi.useFakeTimers();
  onTestFinished(() => {
    vi.useRealTimers();
  });
  return () => act(() => void vi.runOnlyPendingTimers());
}

/** The mounted indicator's outermost element — the tooltip's trigger where it has one — which is
 * what the pointer is put on. */
export function trigger(container: HTMLElement): HTMLElement {
  return container.firstElementChild as HTMLElement;
}
