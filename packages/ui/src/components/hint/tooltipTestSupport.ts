// Shared by the tests of the tooltips an indicator carries: driving the pointer the way react-aria
// wants it, mounting a component that has one, and running its delay out.
import type React from "react";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { onTestFinished, vi } from "vitest";
import { waitOnRealClock } from "../../test/clock";

/** How long react-aria ignores a mouse's hover after a touch press, taking it for the emulated mouse
 * events iOS sends after a tap. Taken from react-aria 3.52, `useHover`
 * (`setGlobalIgnoreEmulatedMouseEvents`); recheck it on an upgrade. */
const TOUCH_HOVER_SUPPRESS_MS = 500;

/** How long react-aria keeps its tooltips warm after one closes, opening the next with no delay.
 * Taken from react-stately 3.50, `useTooltipTriggerState` (`TOOLTIP_COOLDOWN`), which no tooltip
 * here lengthens with a longer `closeDelay`; recheck it on an upgrade. */
const TOOLTIP_COOLDOWN_MS = 500;

/** react-stately 3.50 `useTooltipTriggerState`'s default `TOOLTIP_DELAY`, used for control tooltips when HeroUI's CSS
 * delay is missing under jsdom. Recheck it on an upgrade. */
export const CONTROL_TOOLTIP_DELAY_MS = 1500;

/** Closes an open tooltip while its trigger is still mounted, arming react-stately's global warm-up cooldown. */
export function closeOpenTooltip(): void {
  if (openTooltip()) act(() => void document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
}

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
  if (vi.isFakeTimers()) act(() => void vi.advanceTimersByTime(TOUCH_HOVER_SUPPRESS_MS));
  else onTestFinished(() => waitOnRealClock(TOUCH_HOVER_SUPPRESS_MS));
}

/**
 * Waits, on the real clock, until react-aria's tooltips are cold again, for a test that counts on a
 * tooltip still waiting out its delay. Call it first, before faking any timers.
 *
 * The warm-up is a window-wide singleton the test cannot reset or fake: closing any tooltip leaves
 * it set for `TOOLTIP_COOLDOWN_MS` on whichever clock was real when it closed, during which the next
 * tooltip opens at once. Without this wait such a test passes alone and fails after any test that
 * closed a tooltip.
 */
export async function tooltipsCooledDown(): Promise<void> {
  if (vi.isFakeTimers()) throw new Error("tooltipsCooledDown waits on the real clock: call it before vi.useFakeTimers()");
  await act(() => waitOnRealClock(TOOLTIP_COOLDOWN_MS));
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
 * which is what stops a tooltip's delay still counting down after the test. An open tooltip is closed first: unmount
 * alone leaves react-stately's global warm-up set without arming its cooldown.
 */
export function mountForTooltip(element: React.ReactElement): HTMLElement {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    closeOpenTooltip();
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(element));
  return container;
}

/** The mounted indicator's outermost element — the tooltip's trigger where it has one — which is
 * what the pointer is put on. */
export function trigger(container: HTMLElement): HTMLElement {
  return container.firstElementChild as HTMLElement;
}
