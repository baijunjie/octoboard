import type { IBufferCell, IBufferRange, ILink, ILinkProvider, Terminal } from "@xterm/xterm";

import { IS_MAC } from "../useWindowShortcut";
import {
  cachedRows,
  crossesJoin,
  indentOf,
  type LineChain,
  type LineLayout,
  lineCells,
  lineText,
  type LineRow,
  LOGICAL_LINE_MARGIN_CHARS,
  type LinkSegment,
  logicalLinesIn,
  matchCells,
  mergeSegments,
  textEnd,
  terminalRowReader,
  type TerminalRow,
} from "./terminalLines";

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

const WORD_CHAR = /^[A-Za-z0-9]$/;
const WORD_RUN = /^[A-Za-z0-9]+$/;
/** A run that opens a list item rather than carrying on an address: marks alone (`-`), or a number (`1.`). */
const LIST_MARKER = /^(?:[^A-Za-z0-9]+|[0-9]+\.)$/;

/**
 * Whether `row` carries on an address that ends on the line `above`, and the column it does from.
 *
 * A line whose last row reaches the terminal's last column always does: a break at the width cannot
 * be told from a word that merely follows, so the word is taken along. A line that stops short
 * does only when the address is its last text and the next row is not a separate word or list
 * marker. A program often breaks at its own text width, short of the terminal's last column, and so
 * does a row xterm wrapped after a resize; a mark such as `-` or `&` shows the rows are one address
 * (`…auto-select-` / `account&…`). A finished address and a word of letters or digits
 * (`http://a.dev/x` / `Hans`) stay apart, and so do an address and the marker of a list item that
 * follows it (`-`, `1.`). The width of the lines around is no proof for an address, which a word
 * may follow, so only the terminal's own counts.
 */
function continuesAddress(above: LineRow[], row: TerminalRow, layout: LineLayout): number | undefined {
  const from = indentOf(row.cells);
  const end = urlRun(row.cells, from);
  if (end === from) return undefined;
  const cells = lineCells(above);
  const limit = textEnd(cells);
  let start = limit;
  while (start > 0 && URL_CHAR_CELL.test(cells[start - 1])) start--;
  // The address is the line's last text: it holds a scheme, or is a piece an earlier row began.
  if (start === limit || (start !== indentOf(cells) && cells.slice(start, limit).join("").search(URL_START) < 0)) return undefined;
  if (textEnd(above[above.length - 1].row.cells) >= layout.cols) return from;
  const continuation = row.cells.slice(from, end).join("");
  if (continuation.search(URL_START) === 0 || LIST_MARKER.test(continuation)) return undefined;
  return WORD_CHAR.test(cells[limit - 1]) && WORD_RUN.test(continuation) ? undefined : from;
}

const URL_CHAIN: LineChain = { continues: continuesAddress, maxChars: LOGICAL_LINE_MARGIN_CHARS, ignoresLayout: true };

const URL_GLOBAL = new RegExp(TERMINAL_URL_REGEX.source, "g");

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

/** An address in a logical line: its cells, one segment per row it crosses, and whether the program
 * broke it across rows. An address is ASCII, so each of its characters is one cell. */
interface UrlMatch {
  url: string;
  segments: LinkSegment[];
  range: IBufferRange;
  broken: boolean;
}

function urlMatchesIn(line: LineRow[]): UrlMatch[] {
  const { text, at, joins } = lineText(line);
  const found: UrlMatch[] = [];
  for (const match of text.matchAll(URL_GLOBAL)) {
    if (!isUrl(match[0])) continue;
    const start = match.index;
    const end = start + match[0].length;
    const { segments, range } = matchCells(at, start, end);
    found.push({
      url: match[0],
      segments,
      range,
      broken: crossesJoin(joins, start, end),
    });
  }
  return found;
}

/**
 * Finds the addresses that the program itself broke across rows and that cover row `y` (1-based),
 * for `TERMINAL_URL_REGEX` and the terminal's soft wrap to leave in pieces. Claude Code wraps its
 * own text: it ends the row and, usually, indents the next one, so xterm sees two unrelated rows.
 * Which row carries on which is `continuesAddress`, over logical lines, so that a row xterm
 * wrapped after a resize, or when the output was replayed into a terminal of another width, joins
 * like any other.
 */
export function brokenUrlsAt(rowAt: (y: number) => TerminalRow | undefined, y: number): BrokenUrl[] {
  const found: BrokenUrl[] = [];
  for (const line of logicalLinesIn(cachedRows(rowAt), y, y, URL_CHAIN)) {
    for (const { url, range, broken } of urlMatchesIn(line)) {
      if (broken && range.start.y <= y && y <= range.end.y) found.push({ url, range });
    }
  }
  return found.sort((a, b) => b.range.start.y - a.range.start.y || a.range.start.x - b.range.start.x);
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
  for (const line of logicalLinesIn(read, from, to, URL_CHAIN)) {
    for (const match of urlMatchesIn(line)) segments.push(...match.segments);
  }
  return mergeSegments(segments.filter((segment) => segment.y >= from && segment.y <= to));
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
