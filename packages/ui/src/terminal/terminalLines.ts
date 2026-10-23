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
  /** For a row made of a table cell's share of a buffer line rather than a buffer row: the column
   * its first cell stands at. */
  offset?: number;
  /** For a row made of a table cell's share of a table row xterm soft-wrapped over several buffer
   * lines: the buffer cell each of its cells stands on. */
  places?: BufferCell[];
}

/** A cell of the buffer: `y` is the 1-based buffer line, `col` the 0-based column. */
interface BufferCell {
  y: number;
  col: number;
}

/** One row's share of a logical line. */
export interface LineRow {
  /** The 1-based buffer line: for a table cell's row, the first one its table row is on. */
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
  /** For a table cell's row whose table row xterm soft-wrapped: the last buffer line that is on. */
  bottom?: number;
}

/** What tells a line reached the width its program laid it out at: the terminal's width, and the
 * longest line among those around it, which is wider when the output was laid out for a wider
 * terminal than the one showing it, and narrower when it was laid out for a narrower one. For a
 * table cell's row, both are the width that reaching the cell's edge shows a break at, which is
 * `Infinity` where it shows none (see `tableCellLines`), and `cell` is set: a cell may hold several
 * items, one to a row, whatever width each fills. */
export interface LineLayout {
  cols: number;
  longest: number;
  cell?: boolean;
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
 * program broke the line onto starts. `apart` says the line is made of table cells, so that other
 * text stands between its rows' shares on screen. */
export interface LineText {
  text: string;
  at: TextCell[];
  joins: number[];
  apart: boolean;
}

/** Whether a row is a table cell's share of the buffer rather than a buffer row. */
function isCellRow(row: TerminalRow): boolean {
  return row.offset !== undefined || row.places !== undefined;
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
    const { places, offset = 0 } = row;
    for (let col = from; col < to; col++) {
      const cell = row.cells[col];
      if (cell === "") continue;
      const wide = row.cells[col + 1] === "" && !(row.padded && col + 2 === row.cells.length);
      const width = wide ? 2 : 1;
      const place = places?.[col];
      const cellY = place ? place.y : y;
      const cellCol = place ? place.col : col + offset;
      for (let i = 0; i < cell.length; i++) at.push({ y: cellY, col: cellCol, width });
      text += cell;
    }
  }
  return { text, at, joins, apart: line.some(({ row }) => isCellRow(row)) };
}

/** Whether the text from `start` to `end` (exclusive) runs over a program break. */
export function crossesJoin(joins: number[], start: number, end: number): boolean {
  return joins.some((join) => start < join && join < end);
}

/** The cells the text from `start` to `end` (exclusive) fills, one segment per row, and the ranges
 * to offer them over as a link (1-based, the end inclusive). xterm's range takes in every cell
 * between its ends, which in a line `apart` (made of table cells) would take in the borders and the
 * other columns' text between its pieces, so there each piece is offered on its own. */
export function matchCells(
  at: TextCell[],
  start: number,
  end: number,
  apart: boolean,
): { segments: LinkSegment[]; ranges: IBufferRange[] } {
  const segments: LinkSegment[] = [];
  for (let i = start; i < end; i++) {
    const { y, col, width } = at[i];
    const last = segments[segments.length - 1];
    // The second half of a surrogate pair stands on the cell its first half did.
    if (last && last.y === y && last.col + last.width > col) continue;
    if (last && last.y === y && last.col + last.width === col) last.width += width;
    else segments.push({ y, col, width });
  }
  if (apart) {
    return { segments, ranges: segments.map(({ y, col, width }) => ({ start: { x: col + 1, y }, end: { x: col + width, y } })) };
  }
  const first = at[start];
  const final = at[end - 1];
  return {
    segments,
    ranges: [{ start: { x: first.col + 1, y: first.y }, end: { x: final.col + final.width, y: final.y } }],
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

/** The last buffer line a line's row is on: a table cell's row counts every one its table row is
 * on, where the terminal soft-wrapped that. */
function bottomOf({ y, bottom }: LineRow): number {
  return bottom ?? y;
}

/** The buffer lines a group of rows soft-wrapped together fills. */
function span(group: LineRow[]): { top: number; bottom: number } {
  return { top: group[0].y, bottom: bottomOf(group[group.length - 1]) };
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

/** The groups as lines, where a group the program broke onto goes on the line above. `layoutOf`
 * describes the line a group would go on by the index of its last group, and `rowChars` is what each
 * buffer line a group fills counts towards `chain.maxChars`. A group goes on only while the line,
 * with it, stays within `chain.maxChars`; past that the groups that would go on it are left apart,
 * whichever group the rows being read start at. */
function chainedLines(
  groups: LineRow[][],
  layoutOf: (i: number) => LineLayout,
  chain: LineChain,
  rowChars: number,
): LineRow[][] {
  const lines: LineRow[][] = [];
  // The rows of the previous group as its line holds them, and how many characters the groups that
  // go on one another up to it fill.
  let placed: LineRow[] = [];
  let chars = 0;
  groups.forEach((group, i) => {
    const column = i > 0 ? chain.continues(placed, group[0].row, layoutOf(i - 1)) : undefined;
    if (column === undefined) chars = 0;
    const { top, bottom } = span(group);
    chars += (bottom - top + 1) * rowChars;
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

/** The characters that draw a table's column borders, and a box's sides. */
const TABLE_BORDERS = new Set(["│", "┃", "║"]);

/** Whether a cell holds a box-drawing character, which every row of a drawn table starts with. */
function isBoxDrawing(cell: string | undefined): boolean {
  return cell !== undefined && cell >= "\u2500" && cell <= "\u257f";
}

/** A group's cells laid end to end as the program laid them out. On one buffer line they stand at
 * their own columns. Over rows xterm soft-wrapped, `places` says where each stands, and the empty
 * cell a wide character left at a row's end is taken out, so the columns are the program's. */
interface LaidCells {
  cells: string[];
  places?: BufferCell[];
}

function laidCells(group: LineRow[]): LaidCells {
  if (group.length === 1) return { cells: group[0].row.cells };
  const cells: string[] = [];
  const places: BufferCell[] = [];
  for (const { y, row } of group) {
    for (let col = 0; col < row.cells.length; col++) {
      if (row.padded && col === row.cells.length - 1) continue;
      cells.push(row.cells[col]);
      places.push({ y, col });
    }
  }
  return { cells, places };
}

/** A group drawn as part of a table: a row of cells, whose text starts and ends with a vertical
 * border, with the columns of its borders (a box's inside is a row of one cell); or a rule,
 * box-drawing characters alone (`┌──┬──┐`, `├──┼──┤`, `└──┴──┘`), from column `first` to
 * `last`. */
type TablePart =
  | { kind: "row"; group: LineRow[]; laid: LaidCells; borders: number[] }
  | { kind: "rule"; first: number; last: number };

function tablePartOf(group: LineRow[]): TablePart | undefined {
  const head = group[0].row.cells;
  if (!isBoxDrawing(head[indentOf(head)])) return undefined;
  const laid = laidCells(group);
  const { cells } = laid;
  const first = indentOf(cells);
  const end = textEnd(cells);
  if (end - first >= 2 && TABLE_BORDERS.has(cells[first]) && TABLE_BORDERS.has(cells[end - 1])) {
    const borders: number[] = [];
    for (let col = first; col < end; col++) if (TABLE_BORDERS.has(cells[col])) borders.push(col);
    return { kind: "row", group, laid, borders };
  }
  for (let col = first; col < end; col++) if (!isBoxDrawing(cells[col])) return undefined;
  return { kind: "rule", first, last: end - 1 };
}

type TableRow = Extract<TablePart, { kind: "row" }>;

function sameBorders(a: TableRow, b: TableRow): boolean {
  return a.borders.length === b.borders.length && a.borders.every((col, i) => col === b.borders[i]);
}

/** Whether a run of table rows is one record of a table of two or more columns that rules its
 * records apart: a rule that meets its outer borders is right `above` it and right `below` it. A
 * record that spans more than `reach` buffer lines, the rows read on each side of those asked for,
 * is not taken as one, so that whichever of its rows are asked for, its rules are read or it is no
 * record. */
function isRuledRecord(run: TableRow[], above: TablePart | undefined, below: TablePart | undefined, reach: number): boolean {
  const { borders } = run[0];
  const closes = (part: TablePart | undefined) =>
    part?.kind === "rule" && part.first === borders[0] && part.last === borders[borders.length - 1];
  const lines = span(run[run.length - 1].group).bottom - run[0].group[0].y + 1;
  return borders.length > 2 && lines <= reach && closes(above) && closes(below);
}

/**
 * The lines of a run of table rows whose borders stand at the same columns, a line per cell rather
 * than per row: a program wraps a cell's text inside the cell, so each column's cells chain as rows
 * of their own. A cell's row is its cells between the borders, less the blank a program pads the
 * cell with before the right border (Claude Code pads both sides with one), so that text filling the
 * cell reaches the row's end; the blank after the left border is the row's indent.
 *
 * Text that reaches the cell's edge shows a break there only in a `ruled` record: Claude Code rules
 * the records of a table apart and fills a cell before it wraps its text, so the rows between two
 * rules are one record wrapped. In a box, or a table that does not rule its records apart, a cell
 * is as wide as its longest text, which then reaches the edge whole, and the next row may be another
 * record; there the edge is no proof, and only what the rows' text shows joins them.
 *
 * A row is counted towards the chain's cap at `cols`, the terminal's width, for every buffer line
 * its table row is on, as other rows are, so the rows read around those asked for take in as long
 * a chain.
 */
function tableCellLines(run: TableRow[], ruled: boolean, cols: number, chain: LineChain): LineRow[][] {
  const { borders } = run[0];
  const lines: LineRow[][] = [];
  for (let i = 0; i + 1 < borders.length; i++) {
    const left = borders[i] + 1;
    let right = borders[i + 1];
    if (right - left > 1 && run.every(({ laid }) => laid.cells[right - 1] === " ")) right--;
    if (right <= left) continue;
    const edge = ruled ? right - left : Infinity;
    const layout: LineLayout = { cols: edge, longest: edge, cell: true };
    const groups = run.map(({ group, laid }) => [cellRow(group, laid, left, right)]);
    lines.push(...chainedLines(groups, () => layout, chain, cols));
  }
  return lines;
}

/** The share of a table row from column `left` to `right` (exclusive), as a row of its own. */
function cellRow(group: LineRow[], { cells, places }: LaidCells, left: number, right: number): LineRow {
  const share = cells.slice(left, right);
  const row: TerminalRow = places
    ? { cells: share, wrapped: false, places: places.slice(left, right) }
    : { cells: share, wrapped: false, offset: left };
  const top = group[0].y;
  const bottom = group[group.length - 1].y;
  return { y: top, row, from: 0, to: right - left, ...(bottom !== top && { bottom }) };
}

/** The groups as lines (see `chainedLines`), where a run of table rows makes a line of each cell
 * (see `tableCellLines`) and goes on no line around it. `reach` is how many rows are read on each
 * side of those asked for. */
function chainedWithTables(
  groups: LineRow[][],
  cols: number,
  evidence: number,
  reach: number,
  chain: LineChain,
): LineRow[][] {
  const longest = chain.ignoresLayout ? undefined : layoutEvidence(groups, evidence);
  const parts = groups.map(tablePartOf);
  const lines: LineRow[][] = [];
  let i = 0;
  while (i < groups.length) {
    const part = parts[i];
    let next = i + 1;
    if (part?.kind === "row") {
      const run = [part];
      for (let row = parts[next]; row?.kind === "row" && sameBorders(part, row); row = parts[++next]) run.push(row);
      lines.push(...tableCellLines(run, isRuledRecord(run, parts[i - 1], parts[next], reach), cols, chain));
    } else {
      while (next < groups.length && parts[next]?.kind !== "row") next++;
      const start = i;
      lines.push(...chainedLines(groups.slice(start, next), (j) => ({ cols, longest: longest?.[start + j] ?? 0 }), chain, cols));
    }
    i = next;
  }
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
 * indent. The rows of a table drawn with box-drawing borders are taken apart into their cells, each
 * column's cells chained like rows of their own (see `tableCellLines`). The rows around those asked
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
  const lines = chain ? chainedWithTables(groups, cols, evidence, around, chain) : groups;
  return lines.filter((line) => line.some((row) => row.y <= to && bottomOf(row) >= from));
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
