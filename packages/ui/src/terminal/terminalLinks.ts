import type { IBufferRange } from "@xterm/xterm";

import { isPrimaryModifier } from "../useWindowShortcut";

/**
 * Returns the handler xterm calls for a click on a link, for both detected plain-text addresses
 * and OSC 8 hyperlinks. It opens the link only when the click carries the platform's shortcut
 * modifier (Cmd on an Apple keyboard, Ctrl elsewhere), so a plain click stays a text selection.
 * `open` is the platform's link capability, which a missing one leaves out.
 */
export function terminalLinkHandler(
  open: ((url: string) => Promise<void>) | undefined,
): (event: MouseEvent, uri: string) => void {
  return (event, uri) => {
    if (!open || !isPrimaryModifier(event)) return;
    open(uri).catch((error: unknown) => {
      console.warn("Terminal: cannot open the link", error);
    });
  };
}

/**
 * The plain-text addresses the terminal turns into links: `@xterm/addon-web-links`' own pattern,
 * with every non-ASCII character also ending the address. The addon's stops only at ASCII
 * delimiters, so an address written straight into CJK prose would take the full-width bracket or
 * the words after it along (`…?a=b（加` as one link). An address spelling its path in raw
 * non-ASCII is cut short in exchange; terminal output percent-encodes those nearly always.
 */
export const TERMINAL_URL_REGEX =
  /(https?|HTTPS?):[/]{2}[^\s"'!*(){}|\\^<>`\u0080-\uffff]*[^\s"':,.!?{}|\\^~[\]`()<>\u0080-\uffff]/;

/** An OSC 8 hyperlink under the pointer: its real target, and where it is on screen in viewport
 * pixels, which is what a tooltip on it is placed against. */
export interface HoveredTerminalLink {
  uri: string;
  left: number;
  top: number;
  width: number;
  height: number;
}

/** The cell grid a link's range is measured against. `screen` is the box the cells fill. */
export interface TerminalGrid {
  cols: number;
  rows: number;
  /** The buffer line shown on the first row. */
  viewportY: number;
  screen: { left: number; top: number; width: number; height: number };
}

/** Where a link's `range` (1-based buffer cells, the end inclusive, on a single line) is on screen. */
export function linkBox(uri: string, range: IBufferRange, grid: TerminalGrid): HoveredTerminalLink {
  const cellWidth = grid.screen.width / grid.cols;
  const cellHeight = grid.screen.height / grid.rows;
  const row = range.start.y - 1 - grid.viewportY;
  return {
    uri,
    left: grid.screen.left + (range.start.x - 1) * cellWidth,
    top: grid.screen.top + row * cellHeight,
    width: (range.end.x - range.start.x + 1) * cellWidth,
    height: cellHeight,
  };
}
