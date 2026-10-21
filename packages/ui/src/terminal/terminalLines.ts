import type { IBufferRange, Terminal } from "@xterm/xterm";

/** One buffer row as the cells across it: a blank cell is `" "`, and the right half of a
 * double-width character is `""`, so a cell's index is its column whatever stands before it.
 * `wrapped` is xterm's own soft wrap: the row continues the one above it. `padded` says the last
 * cell is the empty one a double-width character left when it did not fit there and moved to the
 * next row: it is `""` as well, but it is no half of a character. */
export interface TerminalRow {
  cells: string[];
  wrapped: boolean;
  padded?: boolean;
}

/** One row's share of a logical line. */
export interface LineRow {
  /** The 1-based buffer line. */
  y: number;
  row: TerminalRow;
  /** The first column that belongs to the line: a row the program carried the line onto does not
   * take its own indent. */
  from: number;
  /** The column after the last one that belongs to it: the row a program break follows ends where
   * its text does. */
  to: number;
  /** The row is where the program carried the line on, not one xterm soft-wrapped. */
  broke?: boolean;
}

/** What tells a line reached the width its program laid it out at: the terminal's width, and the
 * longest line among those around it, which is wider when the output was laid out for a wider
 * terminal than the one showing it, and narrower when it was laid out for a narrower one. */
export interface LineLayout {
  cols: number;
  longest: number;
}

/** How a logical line goes on over a row the program broke it onto. `continues` answers the column
 * `row`'s share of the line starts at when it carries on `above` (the rows of the line above, as the
 * line holds them, which `layout` describes), or `undefined` when it does not. A chain stops growing
 * at `maxChars`. A chain that sets `ignoresLayout` is not given the longest line around (`longest`
 * is 0), and the rows around that are read to learn it are left unread. */
export interface LineChain {
  continues: (above: LineRow[], row: TerminalRow, layout: LineLayout) => number | undefined;
  maxChars: number;
  ignoresLayout?: boolean;
}

/** The first column of a row's text, after its blank cells. */
export function indentOf(cells: string[]): number {
  let col = 0;
  while (cells[col] === " ") col++;
  return col;
}

/** The first column after the last text in cells. A blank cell, and the empty half of a wide
 * character, is not text. */
export function textEnd(cells: string[]): number {
  let end = cells.length;
  while (end > 0 && (cells[end - 1] === " " || cells[end - 1] === "")) end--;
  return end;
}

/** The cells of a line's rows that belong to it, one after the other. */
export function lineCells(line: LineRow[]): string[] {
  return line.flatMap(({ row, from, to }) => row.cells.slice(from, to));
}

/** How many cells the line fills up to its last text, its earlier rows in full. */
export function lineLength(line: LineRow[]): number {
  return line.reduce((length, { row }, i) => length + (i === line.length - 1 ? textEnd(row.cells) : row.cells.length), 0);
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

/** Where one character of a line's text stands: its row, its first column and its width in cells. */
export interface TextCell {
  y: number;
  col: number;
  width: number;
}

/** A line's text, where each of its characters stands, and the indexes in the text where a row the
 * program broke the line onto starts. */
export interface LineText {
  text: string;
  at: TextCell[];
  joins: number[];
}

/** The text of a line, where each of its characters stands, and the indexes in the text where a row
 * the program broke the line onto starts. A character of a wide one is on its first cell, whose
 * `width` is two. */
export function lineText(line: LineRow[]): LineText {
  let text = "";
  const at: TextCell[] = [];
  const joins: number[] = [];
  for (const { y, row, from, to, broke } of line) {
    if (broke) joins.push(text.length);
    for (let col = from; col < to; col++) {
      const cell = row.cells[col];
      if (cell === "") continue;
      const wide = row.cells[col + 1] === "" && !(row.padded && col + 2 === row.cells.length);
      const width = wide ? 2 : 1;
      for (let i = 0; i < cell.length; i++) at.push({ y, col, width });
      text += cell;
    }
  }
  return { text, at, joins };
}

/** Whether the text from `start` to `end` (exclusive) runs over a program break. */
export function crossesJoin(joins: number[], start: number, end: number): boolean {
  return joins.some((join) => start < join && join < end);
}

/** The cells the text from `start` to `end` (exclusive) fills: one segment per row, and the same
 * as xterm's link range (1-based, the end inclusive). */
export function matchCells(at: TextCell[], start: number, end: number): { segments: LinkSegment[]; range: IBufferRange } {
  const segments: LinkSegment[] = [];
  for (let i = start; i < end; i++) {
    const { y, col, width } = at[i];
    const last = segments[segments.length - 1];
    // The second half of a surrogate pair stands on the cell its first half did.
    if (last && last.y === y && last.col + last.width > col) continue;
    if (last && last.y === y && last.col + last.width === col) last.width += width;
    else segments.push({ y, col, width });
  }
  const first = at[start];
  const final = at[end - 1];
  return {
    segments,
    range: { start: { x: first.col + 1, y: first.y }, end: { x: final.col + final.width, y: final.y } },
  };
}

/** How many characters of a logical line are read beyond the rows being looked at, as the
 * web-links addon bounds its own lookup. A soft-wrapped line can be megabytes long. */
export const LOGICAL_LINE_MARGIN_CHARS = 2048;

/** The fewest rows read on each side of a line to learn the width the program laid its lines out
 * at. A line's layout is whatever lies within this many rows of it, so that it does not depend on
 * which rows were asked for. The reach is the larger of this and the rows `LOGICAL_LINE_MARGIN_CHARS`
 * takes at the terminal's width. */
const LAYOUT_EVIDENCE_ROWS = 20;

/** The buffer lines a group of rows soft-wrapped together fills. */
function span(group: LineRow[]): { top: number; bottom: number } {
  return { top: group[0].y, bottom: group[group.length - 1].y };
}

/** For each group, the length of the longest among those it shares a block of text with and that
 * lie within `reach` rows of it: a blank line ends a block. */
function layoutEvidence(groups: LineRow[][], reach: number): number[] {
  const lengths = groups.map(lineLength);
  return groups.map((group, i) => {
    if (lengths[i] === 0) return 0;
    let longest = lengths[i];
    for (let j = i - 1; j >= 0 && lengths[j] > 0 && span(group).top - span(groups[j]).bottom <= reach; j--) {
      longest = Math.max(longest, lengths[j]);
    }
    for (let j = i + 1; j < groups.length && lengths[j] > 0 && span(groups[j]).top - span(group).bottom <= reach; j++) {
      longest = Math.max(longest, lengths[j]);
    }
    return longest;
  });
}

/** The rows from `start` to `last` (1-based buffer lines) as groups: a row that is not a
 * continuation and the rows xterm soft-wrapped after it, followed to `limit`. */
function softWrappedGroups(
  read: (y: number) => TerminalRow | undefined,
  start: number,
  last: number,
  limit: number,
): LineRow[][] {
  const groups: LineRow[][] = [];
  let y = start;
  while (y <= last) {
    const group: LineRow[] = [];
    for (let row = read(y); row && (group.length === 0 || (row.wrapped && y <= limit)); row = read(y)) {
      group.push({ y, row, from: 0, to: row.cells.length });
      y++;
    }
    if (group.length === 0) break;
    groups.push(group);
  }
  return groups;
}

/** The groups as lines, where a group the program broke onto goes on the line above. A group goes
 * on only while the line, with it, stays within `chain.maxChars`; past that the groups that would
 * go on it are left apart, whichever group the rows being read start at. */
function chainedLines(groups: LineRow[][], longest: number[] | undefined, cols: number, chain: LineChain): LineRow[][] {
  const lines: LineRow[][] = [];
  // The rows of the previous group as its line holds them, and how many characters the groups that
  // go on one another up to it fill.
  let placed: LineRow[] = [];
  let chars = 0;
  groups.forEach((group, i) => {
    const column = i > 0 ? chain.continues(placed, group[0].row, { cols, longest: longest?.[i - 1] ?? 0 }) : undefined;
    if (column === undefined) chars = 0;
    chars += group.length * cols;
    const goesOn = column !== undefined && chars <= chain.maxChars;
    if (goesOn) {
      const line = lines[lines.length - 1];
      const above = line[line.length - 1];
      line[line.length - 1] = { ...above, to: textEnd(above.row.cells) };
      placed = [{ ...group[0], from: column, broke: true }, ...group.slice(1)];
      line.push(...placed);
    } else {
      placed = [...group];
      lines.push(placed);
    }
  });
  return lines;
}

/** The logical lines touching rows `from` to `to` (1-based buffer lines, both inclusive), each as
 * the rows it fills: a row that is not a continuation and the rows xterm soft-wrapped after it. A
 * logical line starts on a row that is not a continuation, and the first row shown may be one;
 * how far a line is followed either way is bounded.
 *
 * With a `chain`, a row the program itself broke a line onto joins the line above it as well, from
 * the column the chain gives (so its indent is no part of the line). xterm does not mark such a row
 * as wrapped: Claude Code, for one, breaks its own text with a line break and the paragraph's
 * indent. The rows around those asked
 * for are read too, as far as a line can reach and then as far as the layout of its rows is
 * learned from, so that a line comes out the same from any rows that include it. */
export function logicalLinesIn(
  read: (y: number) => TerminalRow | undefined,
  from: number,
  to: number,
  chain?: LineChain,
): LineRow[][] {
  const cols = Math.max(1, read(from)?.cells.length ?? 1);
  const margin = Math.ceil(LOGICAL_LINE_MARGIN_CHARS / cols);
  const evidence = Math.max(LAYOUT_EVIDENCE_ROWS, margin);
  const around = chain ? (chain.ignoresLayout ? 0 : evidence) + Math.ceil(chain.maxChars / cols) : 0;
  const last = to + around;
  let start = Math.max(1, from - around);
  while (start > 1 && from - start < around + margin && read(start)?.wrapped) start--;
  const groups = softWrappedGroups(read, start, last, last + margin);
  const lines = chain ? chainedLines(groups, chain.ignoresLayout ? undefined : layoutEvidence(groups, evidence), cols, chain) : groups;
  return lines.filter((line) => line.some(({ y: at }) => at >= from && at <= to));
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
    // A wide character that does not fit the last column moves to the next row and leaves that
    // column empty, which is no blank in the text.
    const wrapsOn = term.buffer.active.getLine(y)?.isWrapped === true;
    let padded = false;
    const cells: string[] = [];
    for (let x = 0; x < term.cols; x++) {
      const c = line.getCell(x, cell);
      const padding = wrapsOn && x === term.cols - 1 && !!c && c.getWidth() !== 0 && !c.getChars();
      if (padding) padded = true;
      cells.push(!c || c.getWidth() === 0 || padding ? "" : c.getChars() || " ");
    }
    return { cells, wrapped: line.isWrapped, ...(padded && { padded }) };
  };
}
