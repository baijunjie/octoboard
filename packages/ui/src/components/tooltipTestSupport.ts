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
