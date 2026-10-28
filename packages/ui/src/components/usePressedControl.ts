import { useCallback, useEffect, useRef } from "react";

/** What a press can end on as a control of its own: the element pressed is the nearest of these around the event's
 * target, so a focusable container around it (a list, a panel with `tabindex`) never counts as what was pressed. */
const CONTROL = [
  "button",
  "a[href]",
  "input",
  "select",
  "textarea",
  '[role="button"]',
  '[role="link"]',
  '[role="menuitem"]',
  '[role="menuitemcheckbox"]',
  '[role="menuitemradio"]',
  '[role="option"]',
  '[role="tab"]',
  '[role="switch"]',
  '[role="checkbox"]',
].join(", ");

/** The keys that press a control. */
const ACTIVATION_KEYS = new Set(["Enter", " "]);

/**
 * Returns what tells, from inside a press's handler, which control that press ended on: React Aria hands a press
 * handler no element, and a press without a pointer (an assistive technology's, a script's `click()`) or one on a
 * control that never takes focus leaves `document.activeElement` elsewhere. Each input event that can end a press is
 * read in the capture phase, ahead of the handler, which runs on the pointer's release, the click of a press without a
 * pointer, or the activation key's; every such event replaces what the last one recorded, nothing included, so an
 * earlier press is never taken for this one. The record lasts until the turn after the event, then reads as nothing.
 * Any other key only drops a record still held, so typing costs no more than a comparison.
 */
export function usePressedControl(): () => HTMLElement | null {
  const pressed = useRef<HTMLElement | null>(null);
  useEffect(() => {
    let clear: ReturnType<typeof setTimeout> | undefined;
    const record = (control: HTMLElement | null) => {
      if (control === null && pressed.current === null) return;
      pressed.current = control;
      clearTimeout(clear);
      if (control) clear = setTimeout(() => { pressed.current = null; }, 0);
    };
    const onPointerOrClick = (event: Event) => {
      record(event.target instanceof Element ? event.target.closest<HTMLElement>(CONTROL) : null);
    };
    const onKey = (event: KeyboardEvent) => {
      record(ACTIVATION_KEYS.has(event.key) && event.target instanceof Element ? event.target.closest<HTMLElement>(CONTROL) : null);
    };
    document.addEventListener("pointerup", onPointerOrClick, true);
    document.addEventListener("click", onPointerOrClick, true);
    document.addEventListener("keydown", onKey, true);
    document.addEventListener("keyup", onKey, true);
    return () => {
      document.removeEventListener("pointerup", onPointerOrClick, true);
      document.removeEventListener("click", onPointerOrClick, true);
      document.removeEventListener("keydown", onKey, true);
      document.removeEventListener("keyup", onKey, true);
      clearTimeout(clear);
    };
  }, []);
  return useCallback(() => pressed.current, []);
}
