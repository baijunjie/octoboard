import type { IBufferRange, ILink, ILinkProvider, Terminal } from "@xterm/xterm";

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

const URL_SCHEME = String.raw`(?:https?|HTTPS?):[/]{2}`;
const URL_CHAR = String.raw`[^\s"'!*(){}|\\^<>\`\u0080-\uffff]`;
const URL_END_CHAR = String.raw`[^\s"':,.!?{}|\\^~[\]\`()<>\u0080-\uffff]`;

/**
 * The plain-text addresses the terminal turns into links: `@xterm/addon-web-links`' own pattern,
 * with every non-ASCII character also ending the address. The addon's stops only at ASCII
 * delimiters, so an address written straight into CJK prose would take the full-width bracket or
 * the words after it along (`…?a=b（加` as one link). An address spelling its path in raw
 * non-ASCII is cut short in exchange; terminal output percent-encodes those nearly always.
 */
export const TERMINAL_URL_REGEX = new RegExp(`${URL_SCHEME}${URL_CHAR}*${URL_END_CHAR}`);

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

/** One buffer row as the cells across it: a blank cell is `" "`, and the right half of a
 * double-width character is `""`, so a cell's index is its column whatever stands before it.
 * `wrapped` is xterm's own soft wrap: the row continues the one above it. */
export interface TerminalRow {
  cells: string[];
  wrapped: boolean;
}

/** An address spread over several rows, with the cells it fills (1-based, the end inclusive). */
export interface BrokenUrl {
  url: string;
  range: IBufferRange;
}

const URL_START = new RegExp(URL_SCHEME, "g");
const URL_CHAR_CELL = new RegExp(`^${URL_CHAR}$`);

/** The end of the run of address characters at `from` in a row's cells. */
function urlRun(cells: string[], from: number): number {
  let end = from;
  while (end < cells.length && URL_CHAR_CELL.test(cells[end])) end++;
  return end;
}

function indentOf(row: TerminalRow): number {
  let col = 0;
  while (row.cells[col] === " ") col++;
  return col;
}

/** The column an address starts at in the row and runs from to the last column, if any does. */
function brokenStartCol(row: TerminalRow): number {
  // A wide character's empty second cell is no character, so the text's indexes are mapped back
  // to columns.
  const cols: number[] = [];
  let text = "";
  row.cells.forEach((cell, col) => {
    if (cell === "") return;
    for (let i = 0; i < cell.length; i++) cols.push(col);
    text += cell;
  });
  for (const match of text.matchAll(URL_START)) {
    const col = cols[match.index];
    if (urlRun(row.cells, col) === row.cells.length) return col;
  }
  return -1;
}

/**
 * Finds the addresses that the program itself broke across rows and that cover row `y` (1-based),
 * for `TERMINAL_URL_REGEX` and the terminal's soft wrap to leave in pieces. Claude Code wraps its
 * own text: it ends the row and, usually, indents the next one, so xterm sees two unrelated rows.
 *
 * An address is broken when it runs to the very last column — Claude Code fills the row to the
 * full width before breaking — and the next row, past any indent, goes on in address characters,
 * which chain onto yet another row for as long as they too reach the last column. A row that
 * xterm itself wrapped is left to the web-links addon. An address that merely ends at the last
 * column takes along the word that starts the next row; that cannot be told apart.
 */
export function brokenUrlsAt(rowAt: (y: number) => TerminalRow | undefined, y: number): BrokenUrl[] {
  const cache = new Map<number, TerminalRow | undefined>();
  const read = (at: number): TerminalRow | undefined => {
    if (!cache.has(at)) cache.set(at, rowAt(at));
    return cache.get(at);
  };
  const found: BrokenUrl[] = [];
  for (let top = y; top >= 1; top--) {
    const url = brokenUrlFrom(read, top);
    if (url && url.range.end.y >= y) found.push(url);
    // Only a row that carries on a row reaching the last column can be the later part of an
    // address.
    const row = read(top);
    const above = read(top - 1);
    if (!row || row.wrapped || !above) break;
    const aboveReaches = brokenStartCol(above) >= 0 || urlRun(above.cells, indentOf(above)) === above.cells.length;
    if (!aboveReaches || !URL_CHAR_CELL.test(row.cells[indentOf(row)])) break;
  }
  // An address written inside another one's continuation belongs to that one.
  return found.filter(
    (url) =>
      !found.some(
        ({ range }) =>
          range.start.y < url.range.start.y &&
          (url.range.start.y < range.end.y || (url.range.start.y === range.end.y && url.range.start.x <= range.end.x)),
      ),
  );
}

function brokenUrlFrom(rowAt: (y: number) => TerminalRow | undefined, y: number): BrokenUrl | undefined {
  const first = rowAt(y);
  if (!first) return undefined;
  const startCol = brokenStartCol(first);
  if (startCol < 0) return undefined;

  const parts = [{ y, from: startCol, cells: first.cells.slice(startCol) }];
  for (let next = y + 1; ; next++) {
    const row = rowAt(next);
    if (!row || row.wrapped) break;
    const from = indentOf(row);
    const end = urlRun(row.cells, from);
    if (end === from) break;
    parts.push({ y: next, from, cells: row.cells.slice(from, end) });
    if (end !== row.cells.length) break;
  }
  if (parts.length === 1) return undefined;

  const match = TERMINAL_URL_REGEX.exec(parts.map((part) => part.cells.join("")).join(""));
  if (!match || match.index !== 0 || match[0].length <= parts[0].cells.length) return undefined;
  let rest = match[0].length;
  for (const part of parts) {
    if (rest > part.cells.length) {
      rest -= part.cells.length;
      continue;
    }
    return {
      url: match[0],
      range: { start: { x: startCol + 1, y }, end: { x: part.from + rest, y: part.y } },
    };
  }
  return undefined;
}

/**
 * The link provider for addresses the program broke across rows (see `brokenUrlsAt`). Registered
 * ahead of the web-links addon, whose link for the first piece alone then loses to this one,
 * as xterm prefers the provider registered first wherever two links meet.
 */
export function brokenUrlLinkProvider(
  term: Terminal,
  activate: (event: MouseEvent, uri: string) => void,
): ILinkProvider {
  const rowAt = (y: number): TerminalRow | undefined => {
    const line = term.buffer.active.getLine(y - 1);
    if (!line) return undefined;
    const cells: string[] = [];
    for (let x = 0; x < term.cols; x++) {
      const cell = line.getCell(x);
      cells.push(!cell || cell.getWidth() === 0 ? "" : cell.getChars() || " ");
    }
    return { cells, wrapped: line.isWrapped };
  };
  return {
    provideLinks(y, callback) {
      const links: ILink[] = brokenUrlsAt(rowAt, y).map((found) => ({ range: found.range, text: found.url, activate }));
      callback(links.length > 0 ? links : undefined);
    },
  };
}
