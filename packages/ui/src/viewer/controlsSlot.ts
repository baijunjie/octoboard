import { createContext } from "react";

/** What the viewer's header offers the view controls (the Markdown source/document choice, the diff
 * layout choice, word wrap), so they sit on the header's own row instead of taking one of their own
 * above the code. Absent where there is no header, and the controls are then not drawn. */
export interface Controls {
  /** The element the Markdown source/document choice is drawn into; null before the header has mounted. */
  markdownView: HTMLElement | null;
  /** The element the diff layout choice is drawn into; null before the header has mounted. */
  layout: HTMLElement | null;
  /** Says that code is on screen whose lines can wrap, which shows the header's wrap choice until
   * the returned function says it is gone. Any number of claims share the one choice. */
  claimWrap: () => () => void;
}

export const ControlsSlot = createContext<Controls | null>(null);
