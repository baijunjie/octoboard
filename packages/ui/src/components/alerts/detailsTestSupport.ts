import { act } from "react";

/** Presses an element the way a pointer does, which is what opens a popover's trigger. */
export function press(element: Element): void {
  act(() => {
    for (const type of ["pointerdown", "mousedown", "pointerup", "mouseup", "click"]) {
      element.dispatchEvent(new MouseEvent(type, { bubbles: true, button: 0 }));
    }
  });
}

/** The Details button of the alert in `root`, if it has one. */
export function detailsButton(root: ParentNode): HTMLButtonElement | undefined {
  return [...root.querySelectorAll("button")].find((button) => button.textContent === "Details");
}

/** Opens the Details popover of the alert in `root` and returns its dialog. */
export function openDetails(root: ParentNode): HTMLElement {
  const button = detailsButton(root);
  if (!button) throw new Error("no Details button");
  press(button);
  return document.querySelector<HTMLElement>('[role="dialog"][aria-label="Details"]')!;
}

/** The "Error details" trigger in `root`, if it has one: the button that shows and hides what the disclosure folds away. */
export function errorDetailsTrigger(root: ParentNode): HTMLButtonElement | null {
  return [...root.querySelectorAll<HTMLButtonElement>("button[aria-expanded]")].find((button) => button.textContent === "Error details") ?? null;
}

/** The "Error details" disclosure in `root`, if it has one: the element holding its trigger and the content under it,
 * which stays in the document (hidden) while the disclosure is collapsed. */
export function errorDetails(root: ParentNode): HTMLElement | null {
  return errorDetailsTrigger(root)?.closest<HTMLElement>(".disclosure") ?? null;
}

/** A copy of `root` without its "Error details" disclosure: what a person reads without opening it. */
export function withoutErrorDetails(root: Element): Element {
  const copy = root.cloneNode(true) as Element;
  errorDetails(copy)?.remove();
  return copy;
}
