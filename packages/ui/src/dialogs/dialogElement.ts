import type { RefObject } from "react";

/** The dialog element around the marker `inside`: HeroUI's `Modal.Dialog` takes no ref, so a dialog
 * finds its own element from a marker it renders inside. */
export function dialogAround(inside: RefObject<HTMLElement | null>): HTMLElement | null | undefined {
  return inside.current?.closest<HTMLElement>('[role="dialog"], [role="alertdialog"]');
}
