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
