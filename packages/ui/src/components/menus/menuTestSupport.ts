import { act } from "react";
import { onTestFinished } from "vitest";

import { runOutTimers } from "../../test/clock";
import { typeInto } from "../../test/dom";

/** react-aria-components' `SubmenuTrigger` default `delay`: how long a hover on a submenu item waits before the
 * submenu opens. It is a prop default inside the library, not exported, so tests that hover a submenu open
 * advance their fake clock by this much. */
export const SUBMENU_HOVER_DELAY_MS = 200;

/** Opens the menu of `trigger` the way a mouse press, a finger's tap or the Enter key does (the input decides whether
 * a menu's filter field takes focus), then runs the fake clock out: react-aria hands focus to the field a frame later.
 * Needs `fakeTimers()` first. */
export async function openBy(trigger: HTMLElement, by: "pointer" | "touch" | "keyboard"): Promise<void> {
  if (by === "keyboard") {
    act(() => trigger.focus());
    await act(async () => {
      trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
      trigger.dispatchEvent(new KeyboardEvent("keyup", { key: "Enter", bubbles: true, cancelable: true }));
    });
  } else {
    const pointerType = by === "touch" ? "touch" : "mouse";
    await act(async () => {
      trigger.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType, button: 0 }));
      trigger.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, pointerType, button: 0 }));
      trigger.click();
    });
  }
  runOutTimers();
}

/** The filter field of the open menu. */
export const filterField = () => document.querySelector<HTMLInputElement>("input")!;

/** Types `text` into the filter field and runs the fake clock out. Needs `fakeTimers()` first. */
export async function typeInFilter(text: string): Promise<void> {
  await act(async () => typeInto(filterField(), text));
  runOutTimers();
}

/** Presses `key` in the filter field and runs the fake clock out. Needs `fakeTimers()` first. */
export async function pressInFilter(key: string): Promise<void> {
  await act(async () => {
    filterField().dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
    filterField().dispatchEvent(new KeyboardEvent("keyup", { key, bubbles: true, cancelable: true }));
  });
  runOutTimers();
}

/** Makes every popover that closes from now on stay mounted, as it does for the exit animation of a real browser, until
 * it is opened again or the test ends. jsdom has no animations, so react-aria takes a popover away the moment its menu
 * closes; it plays one only where an element has `getAnimations`, and waits for what that returns to finish, which here
 * never does. Call it while the menu is open, then close it. Its stand-in for `CSSTransition` is for react-aria's
 * entering check, which reads that global. */
export function holdExitAnimations(): void {
  const target = HTMLElement.prototype as unknown as { getAnimations?: () => unknown[] };
  const global = globalThis as { CSSTransition?: unknown };
  const never = new Promise<void>(() => {});
  target.getAnimations = () => [{ finished: never }];
  global.CSSTransition = class {};
  onTestFinished(() => {
    delete target.getAnimations;
    delete global.CSSTransition;
  });
}
