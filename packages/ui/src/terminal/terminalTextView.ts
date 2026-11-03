import type { Terminal } from "@xterm/xterm";

/** What `exposeReadOnlyView` hangs on the terminal's element. */
export interface TerminalTextView {
  cols: () => number;
  rows: () => number;
  /** The text of viewport row `y`, one character per column. */
  row: (y: number) => string;
}
export type TerminalElement = HTMLElement & { octoboardTerminalText?: TerminalTextView };

/**
 * Hangs a read-only view of `term`'s screen text on its element, for the gallery's pointer steps
 * and the wkbench harness: the WebGL renderer leaves `.xterm-rows` empty, so the DOM no longer says
 * what the terminal shows. It reads the buffer and changes nothing. Call it after `term.open()`.
 */
export function exposeReadOnlyView(term: Terminal): void {
  const view: TerminalTextView = {
    cols: () => term.cols,
    rows: () => term.rows,
    // A wide character's second cell and an empty cell both read as a space, so a string index
    // is a column (for text of single UTF-16 units).
    row: (y) => {
      const line = term.buffer.active.getLine(term.buffer.active.viewportY + y);
      if (!line) return "";
      let text = "";
      for (let x = 0; x < term.cols; x++) {
        const cell = line.getCell(x);
        text += cell?.getChars() || " ";
      }
      return text;
    },
  };
  if (term.element) (term.element as TerminalElement).octoboardTerminalText = view;
}
