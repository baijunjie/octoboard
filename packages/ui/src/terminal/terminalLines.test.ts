import type { Terminal } from "@xterm/xterm";
import { expect, it } from "vitest";

import { type LineChain, type LineRow, lineText, logicalLinesIn, terminalRowReader, type TerminalRow } from "./terminalLines";

it("reads the empty cell a wide character left at the end of a soft-wrapped row as no text, not a blank", () => {
  // A wide character is a cell of width 2 and a cell of width 0, as xterm holds it.
  const lines = [
    { isWrapped: false, cells: [["a", 1], ["b", 1], ["", 1]] },
    { isWrapped: true, cells: [["图", 2], ["", 0], ["c", 1]] },
    { isWrapped: false, cells: [["d", 1], ["", 1], ["", 1]] },
  ] as const;
  const term = {
    cols: 3,
    buffer: { active: {
      getNullCell: () => ({}),
      getLine: (y: number) => lines[y] && {
        isWrapped: lines[y].isWrapped,
        getCell: (x: number) => ({ getChars: () => lines[y].cells[x][0], getWidth: () => lines[y].cells[x][1] }),
      },
    } },
  };
  const read = terminalRowReader(term as unknown as Terminal);
  expect(read(1)).toEqual({ cells: ["a", "b", ""], wrapped: false, padded: true });
  expect(read(2)).toEqual({ cells: ["图", "", "c"], wrapped: true });
  expect(read(3)).toEqual({ cells: ["d", " ", " "], wrapped: false });
});

const whole = (y: number, row: TerminalRow): LineRow => ({ y, row, from: 0, to: row.cells.length });

it("gives the character before a padding cell one cell, and a wide one two", () => {
  const widths = (row: TerminalRow) => lineText([whole(1, row)]).at.map(({ width }) => width);
  expect(widths({ cells: ["a", "b", ""], wrapped: false, padded: true })).toEqual([1, 1]);
  expect(widths({ cells: ["a", "图", ""], wrapped: false })).toEqual([1, 2]);
  expect(widths({ cells: ["图", "", "c"], wrapped: false })).toEqual([2, 1]);
});

// Rows of 200 cells, each filled, every one carrying on the row above it, until a chain fills 8000
// characters (40 rows). That is further than the rows a layout is learned from (20), so how far
// back a chain is followed shows.
const FILLED: TerminalRow[] = Array.from({ length: 100 }, () => ({ cells: Array<string>(200).fill("x"), wrapped: false }));
const ALWAYS: LineChain = { continues: () => 0, maxChars: 8000 };
const spans = (lines: LineRow[][]) => lines.map((line) => line.map(({ y }) => y));
const rowsOf = (first: number, last: number) => Array.from({ length: last - first + 1 }, (_, i) => first + i);

it("cuts a chain at the same row whichever rows are asked for", () => {
  const read = (y: number) => FILLED[y - 1];
  expect(spans(logicalLinesIn(read, 5, 5, ALWAYS))).toEqual([rowsOf(1, 40)]);
  expect(spans(logicalLinesIn(read, 50, 50, ALWAYS))).toEqual([[50]]);
  expect(spans(logicalLinesIn(read, 1, 100, ALWAYS)).filter((line) => line.includes(50))).toEqual([[50]]);
  expect(spans(logicalLinesIn(read, 45, 100, ALWAYS)).filter((line) => line.includes(45))).toEqual([[45]]);
});
