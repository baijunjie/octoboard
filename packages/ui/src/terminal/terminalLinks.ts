import type { IBufferCell, IBufferRange, ILink, ILinkProvider, Terminal } from "@xterm/xterm";

import { IS_MAC } from "../useWindowShortcut";

/**
 * Returns the handler xterm calls for a click on a link, for both detected plain-text addresses
 * and OSC 8 hyperlinks. xterm activates on mouseup even after a drag within a link, so an active
 * selection must suppress opening. Secondary buttons and macOS Control-click belong to the menu.
 * `open` is the platform's link capability, which a missing one leaves out; it gets the click too,
 * for the modifiers held.
 */
export function terminalLinkHandler(
  open: ((url: string, event: MouseEvent) => Promise<void>) | undefined,
  hasSelection: () => boolean,
): (event: MouseEvent, uri: string) => void {
  return (event, uri) => {
    if (!open || event.button !== 0 || (IS_MAC && event.ctrlKey) || hasSelection()) return;
    open(uri, event).catch((error: unknown) => {
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

/** Where each character of the row sits. A wide character's empty second cell is no character, so
 * a match index in the text is the column the character stands in. */
function textColumns(row: TerminalRow): { text: string; cols: number[] } {
  const cols: number[] = [];
  let text = "";
  row.cells.forEach((cell, col) => {
    if (cell === "") return;
    for (let i = 0; i < cell.length; i++) cols.push(col);
    text += cell;
  });
  return { text, cols };
}

/** The column an address starts at in the row and runs from to the last column, if any does. */
function brokenStartCol(row: TerminalRow): number {
  const { text, cols } = textColumns(row);
  for (const match of text.matchAll(URL_START)) {
    const col = cols[match.index];
    if (urlRun(row.cells, col) === row.cells.length) return col;
  }
  return -1;
}

/** The first column after the row's last text. A blank cell, and the empty half of a wide
 * character, is not text. */
function textEnd(cells: string[]): number {
  let end = cells.length;
  while (end > 0 && (cells[end - 1] === " " || cells[end - 1] === "")) end--;
  return end;
}

/** The column a scheme starts at when that address is the row's last text and stops short of the
 * last column. One that fills the row is `brokenStartCol`'s. */
function shortStartCol(row: TerminalRow): number {
  const { text, cols } = textColumns(row);
  const limit = textEnd(row.cells);
  for (const match of text.matchAll(URL_START)) {
    const col = cols[match.index];
    const end = urlRun(row.cells, col);
    if (end === limit && end < row.cells.length) return col;
  }
  return -1;
}

/** The address characters a later row might continue, when they are the row's last text and stop
 * short of the last column: a scheme, or the tail of an address that a row above started. */
function shortTail(row: TerminalRow): string[] | undefined {
  const limit = textEnd(row.cells);
  if (limit === 0 || limit === row.cells.length) return undefined;
  const start = shortStartCol(row);
  if (start >= 0) return row.cells.slice(start, limit);
  const from = indentOf(row);
  if (urlRun(row.cells, from) === limit) return row.cells.slice(from, limit);
  return undefined;
}

const WORD_CHAR = /^[A-Za-z0-9]$/;
const WORD_RUN = /^[A-Za-z0-9]+$/;

/**
 * Whether `row` carries on an address that ends on `above`.
 *
 * A piece that fills its row always does: a break on the last column cannot be told from a word
 * that merely follows, so the word is taken along. A piece that stops short does only when it is
 * the row's last text and the next row is not a separate word. A program often breaks at its own
 * text width, short of the terminal's last column; a mark such as `-` or `&` shows the rows are
 * one address (`…auto-select-` / `account&…`). A finished address and a word of letters or digits
 * (`http://a.dev/x` / `Hans`) stay apart. A row xterm itself wrapped is the web-links addon's.
 */
function continuesAddress(above: TerminalRow, row: TerminalRow): boolean {
  if (row.wrapped) return false;
  const from = indentOf(row);
  const end = urlRun(row.cells, from);
  if (end === from) return false;
  const filled = brokenStartCol(above) >= 0 || urlRun(above.cells, indentOf(above)) === above.cells.length;
  if (filled) return true;
  const tail = shortTail(above);
  if (!tail) return false;
  const continuation = row.cells.slice(from, end).join("");
  if (continuation.search(URL_START) === 0) return false;
  const prevEnd = tail[tail.length - 1] ?? "";
  return !(WORD_CHAR.test(prevEnd) && WORD_RUN.test(continuation));
}

/**
 * Finds the addresses that the program itself broke across rows and that cover row `y` (1-based),
 * for `TERMINAL_URL_REGEX` and the terminal's soft wrap to leave in pieces. Claude Code wraps its
 * own text: it ends the row and, usually, indents the next one, so xterm sees two unrelated rows.
 * Which row carries on which is `continuesAddress`.
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
    const row = read(top);
    const above = read(top - 1);
    if (!row || row.wrapped || !above || !continuesAddress(above, row)) break;
  }
  return outermost(found);
}

/** Without the addresses written inside another one's continuation, which belong to that one. */
function outermost(found: BrokenUrl[]): BrokenUrl[] {
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
  const filledCol = brokenStartCol(first);
  const startCol = filledCol >= 0 ? filledCol : shortStartCol(first);
  if (startCol < 0) return undefined;

  const parts = [{ y, from: startCol, cells: first.cells.slice(startCol, urlRun(first.cells, startCol)) }];
  let prev = first;
  for (let next = y + 1; ; next++) {
    const row = rowAt(next);
    if (!row || !continuesAddress(prev, row)) break;
    const from = indentOf(row);
    const end = urlRun(row.cells, from);
    parts.push({ y: next, from, cells: row.cells.slice(from, end) });
    // Anything past this piece stays on this row. A further row would continue a different address.
    if (textEnd(row.cells) !== end) break;
    prev = row;
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

/** Some of one row's cells that belong to a link: `y` is the 1-based buffer line, `col` the
 * 0-based first column, `width` the number of cells. */
export interface LinkSegment {
  y: number;
  col: number;
  width: number;
}

/** What identifies a segment on screen. */
export function segmentKey(y: number, col: number, width: number): string {
  return `${y}:${col}:${width}`;
}

const URL_GLOBAL = new RegExp(TERMINAL_URL_REGEX.source, "g");

/** How many characters of a logical line are read beyond the rows being looked at, as the
 * web-links addon bounds its own lookup. A soft-wrapped line can be megabytes long. */
const LOGICAL_LINE_MARGIN_CHARS = 2048;

/** Whether `text` parses as an address whose scheme and host are what was written: the web-links
 * addon's own test, which leaves out text that matches the pattern but would not open. */
function isUrl(text: string): boolean {
  try {
    const url = new URL(text);
    const auth = url.password && url.username ? `${url.username}:${url.password}@` : url.username ? `${url.username}@` : "";
    return text.toLowerCase().startsWith(`${url.protocol}//${auth}${url.host}`.toLowerCase());
  } catch {
    return false;
  }
}

/** The cells of the plain-text addresses in one logical line, given as the rows it fills (a row
 * and the rows xterm soft-wrapped after it): one segment per row an address crosses. An address
 * is ASCII, so each of its characters is one cell. */
function plainUrlSegments(rows: { y: number; row: TerminalRow }[]): LinkSegment[] {
  let text = "";
  const at: { y: number; col: number }[] = [];
  for (const { y, row } of rows) {
    row.cells.forEach((cell, col) => {
      if (cell === "") return;
      for (let i = 0; i < cell.length; i++) at.push({ y, col });
      text += cell;
    });
  }
  const segments: LinkSegment[] = [];
  for (const match of text.matchAll(URL_GLOBAL)) {
    if (!isUrl(match[0])) continue;
    for (let i = match.index; i < match.index + match[0].length; i++) {
      const last = segments[segments.length - 1];
      if (i > match.index && last.y === at[i].y && last.col + last.width === at[i].col) last.width++;
      else segments.push({ y: at[i].y, col: at[i].col, width: 1 });
    }
  }
  return segments;
}

/** The cells a broken address fills, one segment per row. The range ends on the last row, so an
 * earlier row is measured by its own address characters: a piece that stops short of the last
 * column must not take the blank cells after it. */
function brokenUrlSegments(found: BrokenUrl, rowAt: (y: number) => TerminalRow | undefined): LinkSegment[] {
  const { start, end } = found.range;
  const segments: LinkSegment[] = [];
  for (let y = start.y; y <= end.y; y++) {
    const row = rowAt(y);
    if (!row) continue;
    const col = y === start.y ? start.x - 1 : indentOf(row);
    const to = y === end.y ? end.x : urlRun(row.cells, col);
    if (to > col) segments.push({ y, col, width: to - col });
  }
  return segments;
}

/** The broken addresses that cover any of the rows `from` to `to`: those covering `from`, and
 * those starting after it. */
function brokenUrlsIn(rowAt: (y: number) => TerminalRow | undefined, from: number, to: number): BrokenUrl[] {
  const found = brokenUrlsAt(rowAt, from);
  for (let y = from + 1; y <= to; y++) {
    const url = brokenUrlFrom(rowAt, y);
    if (url) found.push(url);
  }
  return outermost(found);
}

/** Joins the segments that overlap on a row into one, and sorts them. */
export function mergeSegments(segments: LinkSegment[]): LinkSegment[] {
  const merged: LinkSegment[] = [];
  for (const segment of [...segments].sort((a, b) => a.y - b.y || a.col - b.col)) {
    const last = merged[merged.length - 1];
    if (last && last.y === segment.y && segment.col < last.col + last.width) {
      last.width = Math.max(last.width, segment.col + segment.width - last.col);
    } else {
      merged.push({ ...segment });
    }
  }
  return merged;
}

/** The logical lines touching rows `from` to `to` (1-based buffer lines, both inclusive), each as
 * the rows it fills: a row that is not a continuation and the rows xterm soft-wrapped after it. A
 * logical line starts on a row that is not a continuation, and the first row shown may be one;
 * how far a line is followed either way is bounded. */
export function logicalLinesIn(
  read: (y: number) => TerminalRow | undefined,
  from: number,
  to: number,
): { y: number; row: TerminalRow }[][] {
  const margin = Math.ceil(LOGICAL_LINE_MARGIN_CHARS / Math.max(1, read(from)?.cells.length ?? 1));
  const found: { y: number; row: TerminalRow }[][] = [];
  let y = from;
  while (y > 1 && from - y < margin && read(y)?.wrapped) y--;
  while (y <= to) {
    const lines: { y: number; row: TerminalRow }[] = [];
    for (let row = read(y); row && (lines.length === 0 || (row.wrapped && y <= to + margin)); row = read(y)) {
      lines.push({ y, row });
      y++;
    }
    if (lines.length === 0) break;
    found.push(lines);
  }
  return found;
}

/**
 * The cells of every address `TERMINAL_URL_REGEX` finds (and `new URL` accepts), soft-wrapped ones
 * included, and of every one the program broke across rows (see `brokenUrlsAt`), on the rows
 * `from` to `to` (1-based buffer lines, both inclusive), read through `read`, which should
 * remember rows (see `cachedRows`). A segment lies on one row, and none overlaps another.
 */
export function linkSegmentsIn(
  read: (y: number) => TerminalRow | undefined,
  from: number,
  to: number,
): LinkSegment[] {
  const segments: LinkSegment[] = [];
  for (const lines of logicalLinesIn(read, from, to)) segments.push(...plainUrlSegments(lines));
  for (const url of brokenUrlsIn(read, from, to)) segments.push(...brokenUrlSegments(url, read));
  return mergeSegments(segments.filter((segment) => segment.width > 0 && segment.y >= from && segment.y <= to));
}

/** `rowAt` remembering each row it has read, for the several passes over the same rows that finding
 * links takes. It is good for one look at the screen, no longer. */
export function cachedRows(rowAt: (y: number) => TerminalRow | undefined): (y: number) => TerminalRow | undefined {
  const cache = new Map<number, TerminalRow | undefined>();
  return (y) => {
    if (!cache.has(y)) cache.set(y, rowAt(y));
    return cache.get(y);
  };
}

/** Reads buffer line `y` (1-based) of `term`'s active buffer as a `TerminalRow`. */
export function terminalRowReader(term: Terminal): (y: number) => TerminalRow | undefined {
  const cell = term.buffer.active.getNullCell();
  return (y) => {
    const line = term.buffer.active.getLine(y - 1);
    if (!line) return undefined;
    const cells: string[] = [];
    for (let x = 0; x < term.cols; x++) {
      const c = line.getCell(x, cell);
      cells.push(!c || c.getWidth() === 0 ? "" : c.getChars() || " ");
    }
    return { cells, wrapped: line.isWrapped };
  };
}

/** More cells to colour as links on rows `from` to `to`, read through the same `read` as the
 * addresses. */
export type ExtraLinkSegments = (
  read: (y: number) => TerminalRow | undefined,
  from: number,
  to: number,
) => LinkSegment[];

/** The cells to colour as links in the rows `term` is showing now: the addresses, and the cells
 * `extra` gives. */
export function visibleLinkSegments(term: Terminal, extra?: ExtraLinkSegments): LinkSegment[] {
  const { viewportY } = term.buffer.active;
  const read = cachedRows(terminalRowReader(term));
  const from = viewportY + 1;
  const to = viewportY + term.rows;
  return mergeSegments([
    ...linkSegmentsIn(read, from, to),
    ...visibleOscLinkSegments(term),
    ...(extra?.(read, from, to) ?? []),
  ]);
}


/** xterm 6 returns its internal CellData through the public buffer API. Only this adapter reads
 * its extended URL id and the OSC link service, which have no public equivalent. Missing internals
 * leave OSC labels uncoloured while plain-text detection continues to work. */
function visibleOscLinkSegments(term: Terminal): LinkSegment[] {
  const service = (term as unknown as {
    _core?: { _oscLinkService?: { getLinkData?: (id: number) => { uri: string } | undefined } };
  })._core?._oscLinkService;
  if (typeof service?.getLinkData !== "function") return [];
  const buffer = term.buffer.active;
  const cell = buffer.getNullCell();
  const accepted = new Map<number, boolean>();
  const segments: LinkSegment[] = [];
  for (let y = buffer.viewportY; y < buffer.viewportY + term.rows; y++) {
    const line = buffer.getLine(y);
    if (!line) continue;
    for (let col = 0; col < term.cols; col++) {
      const current = line.getCell(col, cell) as (IBufferCell & { hasExtendedAttrs?: () => number; extended?: { urlId?: number } }) | undefined;
      const id = current?.hasExtendedAttrs?.() ? current.extended?.urlId : undefined;
      if (!id) continue;
      if (!accepted.has(id)) {
        const uri = service.getLinkData(id)?.uri;
        let valid = false;
        try {
          valid = !!uri && ["http:", "https:"].includes(new URL(uri).protocol);
        } catch {
          // Match xterm's refusal to offer malformed OSC 8 targets as links.
        }
        accepted.set(id, valid);
      }
      if (!accepted.get(id)) continue;
      const last = segments[segments.length - 1];
      if (last?.y === y + 1 && last.col + last.width === col) last.width++;
      else segments.push({ y: y + 1, col, width: 1 });
    }
  }
  return segments;
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
  const rowAt = terminalRowReader(term);
  return {
    provideLinks(y, callback) {
      const links: ILink[] = brokenUrlsAt(rowAt, y).map((found) => ({ range: found.range, text: found.url, activate }));
      callback(links.length > 0 ? links : undefined);
    },
  };
}
