// @vitest-environment jsdom
import type { Terminal } from "@xterm/xterm";
import { expect, it, vi } from "vitest";

import { IS_MAC } from "../useWindowShortcut";
import type { TerminalRow } from "./terminalLines";
import { brokenUrlsAt, linkBox, linkSegmentsIn, TERMINAL_URL_REGEX, terminalLinkHandler, visibleLinkSegments } from "./terminalLinks";

it("opens a link with a plain click", () => {
  const open = vi.fn(async () => {});
  const onClick = terminalLinkHandler(open, () => false);
  const event = new MouseEvent("click");
  onClick(event, "https://example.com");
  expect(open).toHaveBeenCalledWith("https://example.com", event);
});

it.each<[string, MouseEventInit, boolean]>([
  ["a text selection", {}, true],
  ["a secondary button", { button: 2 }, false],
  ...(IS_MAC ? [["macOS Control-click", { ctrlKey: true }, false] as [string, MouseEventInit, boolean]] : []),
])("does not open a link for %s", (_name, options, selected) => {
  const open = vi.fn(async () => {});
  terminalLinkHandler(open, () => selected)(new MouseEvent("mouseup", options), "https://example.com");
  expect(open).not.toHaveBeenCalled();
});

it("boxes a link from the cell grid", () => {
  const grid = { cols: 10, rows: 5, viewportY: 20, screen: { left: 100, top: 50, width: 100, height: 100 } };
  expect(linkBox("u", { start: { x: 3, y: 22 }, end: { x: 5, y: 22 } }, grid)).toEqual({
    uri: "u", left: 120, top: 70, width: 30, height: 20,
  });
});

it("ends a plain-text address at the first non-ASCII character", () => {
  const match = (text: string) => TERMINAL_URL_REGEX.exec(text)?.[0];
  expect(match("看 http://localhost:5301/a.html?s=x（加&t=d")).toBe("http://localhost:5301/a.html?s=x");
  expect(match("http://localhost:5301/a.html看深色")).toBe("http://localhost:5301/a.html");
});

// A terminal 24 columns wide; a double-width character takes two cells, the second one empty.
const COLS = 24;
function row(text: string, wrapped = false): TerminalRow {
  const cells = [...text].flatMap((char) => (char.charCodeAt(0) > 0xff ? [char, ""] : [char]));
  return { cells: [...cells, ...Array(COLS - cells.length).fill(" ")], wrapped };
}
function brokenAt(rows: TerminalRow[], y: number) {
  return brokenUrlsAt((at) => rows[at - 1], y);
}

it("joins an address the program broke onto the next row, whichever of the rows is hovered", () => {
  const rows = [row("see http://a.dev/x?l=zh-"), row("  Hans and more")];
  const link = {
    url: "http://a.dev/x?l=zh-Hans",
    range: { start: { x: 5, y: 1 }, end: { x: 6, y: 2 } },
  };
  expect(brokenAt(rows, 1)).toEqual([link]);
  expect(brokenAt(rows, 2)).toEqual([link]);
});

it("measures a joined address in cells, past a double-width prefix", () => {
  const rows = [row("看http://a.dev/xyzxyzxyz"), row("  &t=1")];
  expect(brokenAt(rows, 1)[0].range).toEqual({ start: { x: 3, y: 1 }, end: { x: 6, y: 2 } });
});

it("joins the first address of a row that reaches the last column, not the last one", () => {
  const rows = [row("http://a.b/http://c.d/xx"), row("  yy")];
  expect(brokenAt(rows, 1)[0].url).toBe("http://a.b/http://c.d/xxyy");
});

it("leaves an address written inside another one's continuation to the outer one", () => {
  const rows = [row("see http://a.b/xxxxxxxxx"), row("  web/http://c.d/yyyyyyy"), row("  zz")];
  expect(brokenAt(rows, 2).map((link) => link.url)).toEqual(["http://a.b/xxxxxxxxxweb/http://c.d/yyyyyyyzz"]);
});

it("chains an address over every row that reaches the last column", () => {
  const rows = [row("http://a.dev/aaaaaaaaaaa"), row("  bbbbbbbbbbbbbbbbbbbbbb"), row("  cc")];
  expect(brokenAt(rows, 3)).toEqual([
    {
      url: "http://a.dev/aaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbcc",
      range: { start: { x: 1, y: 1 }, end: { x: 4, y: 3 } },
    },
  ]);
});

it("finds both the address a row ends and the one it starts", () => {
  const rows = [row("http://a.dev/aaaaaaaaaaa"), row("  bb http://c.dev/cccccc"), row("  dd")];
  expect(brokenAt(rows, 2).map((link) => link.url)).toEqual([
    "http://c.dev/ccccccdd",
    "http://a.dev/aaaaaaaaaaabb",
  ]);
});

it.each([
  ["it stops short of the last column and the next row is a separate word", [row("http://a.dev/x"), row("  Hans")]],
  ["the next row is a separate word", [row("http://a.dev/docs"), row("  frame")]],
  ["the next row starts its own address", [row("http://a.dev/x"), row("  http://b.dev/y")]],
  ["it stops short of the last column and a list item follows", [row("http://a.dev/?l=zh-Hans"), row("  - Show it")]],
  ["it stops short of the last column and a numbered item follows", [row("http://a.dev/?l=zh-Hans"), row("  1. Show it")]],
  ["it stops short of the last column and a lettered item follows", [row("http://a.dev/?l=zh-Hans"), row("  b. Show it")]],
  ["it stops short of the last column and a roman-numbered item follows", [row("http://a.dev/?l=zh-Hans"), row("  iv. Show it")]],
  ["it stops short of the last column and an upper-case item follows", [row("http://a.dev/?l=zh-Hans"), row("  IV. Show it")]],
  ["other text follows it on its own row", [row("http://a.dev/sel- now"), row("  account&x=1")]],
  ["a border follows it", [row("http://a.dev/sel-│"), row("  account&x=1")]],
  ["the next row is empty", [row("http://a.dev/xxxxxxxxxxx"), row("")]],
  ["the next row starts with a delimiter", [row("http://a.dev/xxxxxxxxxxx"), row("  (Hans)")]],
  ["the next row is the terminal's own wrap", [row("http://a.dev/xxxxxxxxxxx"), row("Hans", true)]],
])("leaves an address alone when %s", (_rule, rows) => {
  expect(brokenAt(rows, 1)).toEqual([]);
  expect(brokenAt(rows, 2)).toEqual([]);
});

it("joins an address the program broke short of the last column, whichever row is hovered", () => {
  const rows = [row("http://a.dev/sel-"), row("  account&x=1")];
  const link = {
    url: "http://a.dev/sel-account&x=1",
    range: { start: { x: 1, y: 1 }, end: { x: 13, y: 2 } },
  };
  expect(brokenAt(rows, 1)).toEqual([link]);
  expect(brokenAt(rows, 2)).toEqual([link]);
});

it("joins a short address that breaks on a hyphen onto the word that follows", () => {
  const rows = [row("http://a.dev/sel-"), row("  Hans")];
  expect(brokenAt(rows, 2).map((link) => link.url)).toEqual(["http://a.dev/sel-Hans"]);
});

it("joins a short address onto a continuation that carries address punctuation", () => {
  const rows = [row("http://a.dev/docs"), row("  frame.html?x=1")];
  expect(brokenAt(rows, 1).map((link) => link.url)).toEqual(["http://a.dev/docsframe.html?x=1"]);
});

it("chains a short break across every following row that still continues the address", () => {
  const rows = [row("http://a.dev/sel-"), row("  account&theme="), row("  light&lang=zh-Hans")];
  expect(brokenAt(rows, 3).map((link) => link.url)).toEqual(["http://a.dev/sel-account&theme=light&lang=zh-Hans"]);
  expect(brokenAt(rows, 2).map((link) => link.url)).toEqual(["http://a.dev/sel-account&theme=light&lang=zh-Hans"]);
});

it("joins a short address onto a word that ends a sentence", () => {
  expect(brokenAt([row("http://a.dev/sel-"), row("  Hans. Next")], 2).map((link) => link.url)).toEqual(["http://a.dev/sel-Hans"]);
  expect(brokenAt([row("http://a.dev/sel-"), row("  mid. Next")], 2).map((link) => link.url)).toEqual(["http://a.dev/sel-mid"]);
});

it("stops a short chain at a separate word", () => {
  const rows = [row("http://a.dev/docs"), row("  frame.html"), row("  today")];
  expect(brokenAt(rows, 1).map((link) => link.url)).toEqual(["http://a.dev/docsframe.html"]);
  expect(brokenAt(rows, 3)).toEqual([]);
});

function sized(cols: number, text: string, wrapped = false): TerminalRow {
  return { cells: [...text.padEnd(cols)], wrapped };
}

it("joins an address in output replayed into a narrower terminal, where its first row was soft-wrapped", () => {
  // Laid out 26 columns wide, shown in 20: the first row xterm wrapped is short, and the
  // program's break follows it.
  const rows = [sized(20, "  https://ex.dev/aaa"), sized(20, "aaaaaa", true), sized(20, "  bbb/ccc next")];
  const url = "https://ex.dev/aaaaaaaaabbb/ccc";
  const link = { url, range: { start: { x: 3, y: 1 }, end: { x: 9, y: 3 } } };
  expect(brokenAt(rows, 1)).toEqual([link]);
  expect(brokenAt(rows, 3)).toEqual([link]);
  expect(segmentsOf(rows)).toEqual([
    { y: 1, col: 2, width: 18 },
    { y: 2, col: 0, width: 6 },
    { y: 3, col: 2, width: 7 },
  ]);
});

it("joins an address whose first row was soft-wrapped at the width a resize left behind, onto a continuation that is no plain word", () => {
  const rows = [sized(20, "  https://ex.dev/aaa"), sized(20, "aaaaaa", true), sized(20, "  b.html")];
  expect(brokenAt(rows, 2).map((link) => link.url)).toEqual(["https://ex.dev/aaaaaaaaab.html"]);
});

it("leaves an address alone when the last row of its soft-wrapped line stops short and a word follows", () => {
  const rows = [sized(20, "some prose words to "), sized(20, "see https://a.dev/x", true), sized(20, "  Next thing")];
  expect(brokenAt(rows, 2)).toEqual([]);
  expect(brokenAt(rows, 3)).toEqual([]);
});

it("joins a replayed address over every row, though the last soft-wrapped row of a piece is short", () => {
  const rows = [
    sized(30, "  https://example.com/aaaaaaaa"),
    sized(30, "a/bbbbbbb", true),
    sized(30, "  bbb/cccccccccc/01234 next"),
  ];
  expect(brokenAt(rows, 3).map((link) => link.url)).toEqual(["https://example.com/aaaaaaaaa/bbbbbbbbbb/cccccccccc/01234"]);
});

it("joins an address broken at the terminal's width though a longer soft-wrapped line is in its block", () => {
  const rows = [row("http://a.dev/aaaaaaaaaaa"), row("  bbb"), row("a line that fills up row"), row("and goes on", true)];
  expect(brokenAt(rows, 1).map((link) => link.url)).toEqual(["http://a.dev/aaaaaaaaaaabbb"]);
});

it("leaves an address alone when its row stops short of the layout width and a word follows", () => {
  const rows = [sized(60, "  https://github.com/org/repo/commit/0123456789ab"), sized(60, "  cdef0123 next")];
  expect(brokenAt(rows, 1)).toEqual([]);
  expect(brokenAt(rows, 2)).toEqual([]);
});

function segmentsOf(rows: TerminalRow[], from = 1, to = rows.length) {
  return linkSegmentsIn((at) => rows[at - 1], from, to);
}

it("segments a plain address, past a double-width prefix", () => {
  expect(segmentsOf([row("看 http://a.dev/x, ok")])).toEqual([{ y: 1, col: 3, width: 14 }]);
});

it("segments an address xterm wrapped across rows, even when only its tail is in view", () => {
  const rows = [row("see http://a.dev/xxxxxxx"), row("yyyy and more", true)];
  const expected = [
    { y: 1, col: 4, width: 20 },
    { y: 2, col: 0, width: 4 },
  ];
  expect(segmentsOf(rows)).toEqual(expected);
  expect(segmentsOf(rows, 2, 2)).toEqual([expected[1]]);
});

it("segments an address the program broke, each row from its indent, once", () => {
  const rows = [row("see http://a.dev/xxxxxxx"), row("  yyyyyyyyyyyyyyyyyyyyyy"), row("  zz")];
  expect(segmentsOf(rows)).toEqual([
    { y: 1, col: 4, width: 20 },
    { y: 2, col: 2, width: 22 },
    { y: 3, col: 2, width: 2 },
  ]);
});

it("segments an address the program broke short of the last column, on every row", () => {
  const rows = [row("http://a.dev/sel-"), row("  account&x=1")];
  expect(segmentsOf(rows)).toEqual([
    { y: 1, col: 0, width: 17 },
    { y: 2, col: 2, width: 11 },
  ]);
});


// A table as Claude Code draws one, 110 columns wide: each cell padded with a blank on both sides,
// and text too long for its cell wrapped inside it, filling the cell before it breaks.
function cellRow(text: string, cols = 110): TerminalRow {
  const cells = [...text].flatMap((char) => (char.charCodeAt(0) > 0x2fff ? [char, ""] : [char]));
  return { cells: [...cells, ...Array(cols - cells.length).fill(" ")], wrapped: false };
}
const TABLE = [
  "┌──────┬──────────────────────────────────────────────┬──────────────────────────────────────────────┐",
  "│ 主色 │ http://localhost:5480/gallery.html?scenario= │ http://localhost:5480/gallery.html?scenario= │",
  "│ (45% │ focus-context-tooltip-limit&theme=light&lang │ focus-context-tooltip-limit&theme=dark&lang= │",
  "│ )    │ =zh-Hans                                     │ zh-Hans                                      │",
  "├──────┼──────────────────────────────────────────────┼──────────────────────────────────────────────┤",
  "│ 辅色 │ http://localhost:5480/gallery.html?s=a&la    │ see                                          │",
  "│      │ ng=en                                        │ http://localhost:5480/gallery.html?scenario= │",
  "│      │                                              │ x&theme=dark                                 │",
  "└──────┴──────────────────────────────────────────────┴──────────────────────────────────────────────┘",
].map((text) => cellRow(text));
const LIGHT = "http://localhost:5480/gallery.html?scenario=focus-context-tooltip-limit&theme=light&lang=zh-Hans";
const DARK = "http://localhost:5480/gallery.html?scenario=focus-context-tooltip-limit&theme=dark&lang=zh-Hans";
const piece = (y: number, col: number, width: number) => ({ start: { x: col + 1, y }, end: { x: col + width, y } });

it("joins an address wrapped inside a table cell, in each column, offering each piece on its own row", () => {
  expect(brokenAt(TABLE, 2)).toEqual([
    { url: LIGHT, range: piece(2, 9, 44) },
    { url: DARK, range: piece(2, 56, 44) },
  ]);
  expect(brokenAt(TABLE, 3)).toEqual([
    { url: LIGHT, range: piece(3, 9, 44) },
    { url: DARK, range: piece(3, 56, 44) },
  ]);
  expect(brokenAt(TABLE, 4)).toEqual([
    { url: LIGHT, range: piece(4, 9, 8) },
    { url: DARK, range: piece(4, 56, 7) },
  ]);
});

it("colours an address in table cells on its pieces alone, without the borders or the padding", () => {
  expect(segmentsOf(TABLE, 1, 4)).toEqual([
    { y: 2, col: 9, width: 44 },
    { y: 2, col: 56, width: 44 },
    { y: 3, col: 9, width: 44 },
    { y: 3, col: 56, width: 44 },
    { y: 4, col: 9, width: 8 },
    { y: 4, col: 56, width: 7 },
  ]);
});

it("joins an address broken inside a word over two rows of a cell, beside a cell that starts it on its second row", () => {
  expect(brokenAt(TABLE, 7).map((link) => link.url)).toEqual([
    "http://localhost:5480/gallery.html?s=a&lang=en",
    "http://localhost:5480/gallery.html?scenario=x&theme=dark",
  ]);
  expect(brokenAt(TABLE, 8)).toEqual([{ url: "http://localhost:5480/gallery.html?scenario=x&theme=dark", range: piece(8, 56, 12) }]);
});

it("leaves a cell's address alone where a row border follows it", () => {
  const rows = [TABLE[1], TABLE[4], cellRow("│ more │ focus-context                                │ x                                            │")];
  expect(brokenAt(rows, 1)).toEqual([]);
  expect(brokenAt(rows, 3)).toEqual([]);
});

it("leaves a cell's address alone where it stops short of the cell's end and a word follows", () => {
  const rows = [
    cellRow("│ a    │ http://localhost:5480/gallery.html           │ b                                            │"),
    cellRow("│      │ today                                        │                                              │"),
  ];
  expect(brokenAt(rows, 1)).toEqual([]);
  expect(brokenAt(rows, 2)).toEqual([]);
});

it("joins an address in a table row the terminal soft-wrapped after it narrowed", () => {
  // The rule and the first record at 60 columns: xterm wrapped the 102-cell rows onto two.
  const narrow = TABLE.slice(0, 5).flatMap((wide) => [
    { cells: wide.cells.slice(0, 60), wrapped: false },
    { cells: wide.cells.slice(60, 120), wrapped: true },
  ]);
  // The record's last row: the left piece on its first half, the right one split over both.
  expect(brokenAt(narrow, 7)).toEqual([
    { url: LIGHT, range: piece(7, 9, 8) },
    { url: DARK, range: piece(7, 56, 4) },
  ]);
  expect(brokenAt(narrow, 8)).toEqual([{ url: DARK, range: piece(8, 0, 3) }]);
  expect(brokenAt(narrow, 4)).toEqual([{ url: DARK, range: piece(4, 0, 40) }]);
});

/** `rows` as a terminal `cols` wide shows them once xterm has soft-wrapped each: a wide character
 * that does not fit a row's last column moves to the next row and leaves that column padded. */
function narrowed(rows: TerminalRow[], cols: number): TerminalRow[] {
  return rows.flatMap(({ cells }) => {
    const out: TerminalRow[] = [];
    let current: string[] = [];
    for (let i = 0; i < cells.length; ) {
      const width = cells[i + 1] === "" ? 2 : 1;
      if (current.length + width > cols) {
        const padded = current.length < cols;
        out.push({ cells: [...current, ...Array(cols - current.length).fill("")], wrapped: out.length > 0, ...(padded && { padded }) });
        current = [];
      }
      current.push(...cells.slice(i, i + width));
      i += width;
    }
    out.push({ cells: [...current, ...Array(cols - current.length).fill(" ")], wrapped: out.length > 0 });
    return out;
  });
}
/** The text on screen in a link's segments. */
const textOf = (rows: TerminalRow[], segments: { y: number; col: number; width: number }[]) =>
  segments.map(({ y, col, width }) => rows[y - 1].cells.slice(col, col + width).join("")).join("");

it("joins an address in a table row soft-wrapped where a wide character left a padded column", () => {
  const rule = (left: string, mid: string, right: string) => `${left}${"─".repeat(8)}${mid}${"─".repeat(26)}${right}`;
  const wide = [
    rule("┌", "┬", "┐"),
    "│ 一二三 │ http://a.dev/aaaaaaaaaaa │",
    "│        │ bbbbbbbbbbbb             │",
    rule("└", "┴", "┘"),
  ].map((text) => cellRow(text, 40));
  // At 5 columns, `二` would straddle the last column of the second row of `│ 一二三 …`.
  const rows = narrowed(wide, 5);
  expect(rows.some((row) => row.padded)).toBe(true);
  const segments = segmentsOf(rows);
  expect(textOf(rows, segments)).toBe("http://a.dev/aaaaaaaaaaabbbbbbbbbbbb");
  const y = segments[0].y;
  expect(brokenAt(rows, y).map((link) => link.url)).toEqual(["http://a.dev/aaaaaaaaaaabbbbbbbbbbbb"]);
});

// A record of a ruled table, a box, and a table whose records are not ruled apart, each with an
// address that fills its cell above a row that starts with a word.
const FILLED = "http://localhost:3000/api?id=12345";
/** `texts` as a ruled table, its first `header` rows ruled off from the rest. */
const ruled = (texts: string[], header = 0) => {
  const widths = texts[0].split("│").slice(1, -1).map((cell) => cell.length);
  const rule = (left: string, mid: string, right: string) => left + widths.map((width) => "─".repeat(width)).join(mid) + right;
  const ruledOff = header > 0 ? [...texts.slice(0, header), rule("├", "┼", "┤"), ...texts.slice(header)] : texts;
  return [rule("┌", "┬", "┐"), ...ruledOff, rule("└", "┴", "┘")].map((text) => cellRow(text, 80));
};

it("joins a word onto an address that fills its cell in a record of a ruled table", () => {
  const rows = ruled([`│ id │ ${FILLED} │`, `│    │ 678${" ".repeat(FILLED.length - 3)} │`]);
  expect(brokenAt(rows, 3).map((link) => link.url)).toEqual([`${FILLED}678`]);
});

it.each([
  ["a box", ["┌" + "─".repeat(33) + "┐", "│ Docs: http://localhost:3000/api │", "│ Press q to quit                 │", "└" + "─".repeat(33) + "┘"]],
  ["a table that does not rule its records apart", [`│ id │ ${FILLED} │`, `│    │ 678${" ".repeat(FILLED.length - 3)} │`]],
  ["a table whose next record starts its own address", [
    "│ Name │ URL                          │",
    "│ web  │ http://localhost:3000/web/ap │",
    "│ docs │ https://x.dev                │",
  ]],
])("leaves an address that fills its cell alone in %s", (_name, texts) => {
  const rows = texts.map((text) => cellRow(text, 80));
  for (let y = 1; y <= rows.length; y++) expect(brokenAt(rows, y)).toEqual([]);
});

// A table that rules off its header alone, so its body is one record however many it lists.
const ADDRESS = "http://localhost:3000/web/ap";
const listed = (next: string) =>
  ruled([`│ Name │ ${"URL".padEnd(ADDRESS.length)} │`, `│ web  │ ${ADDRESS} │`, `│ docs │ ${next.padEnd(ADDRESS.length)} │`], 1);

it("joins no address in a cell onto a row that starts an address of its own, though it fills the cell", () => {
  const rows = listed("https://x.dev");
  for (let y = 1; y <= rows.length; y++) expect(brokenAt(rows, y)).toEqual([]);
  expect(textOf(rows, segmentsOf(rows))).toBe(`${ADDRESS}https://x.dev`);
  expect(segmentsOf(rows).map(({ y }) => y)).toEqual([4, 5]);
});

it("joins a word onto an address that fills its cell where a table rules off its header alone", () => {
  // A limit, not a wish: the body is taken as one record, so the next record's word is taken along.
  expect(brokenAt(listed("pending"), 5).map((link) => link.url)).toEqual([`${ADDRESS}pending`]);
});

it("takes a record taller than the rows read around it by the rules for a row that stops short, from any of its rows", () => {
  // 80 columns read 26 rows on each side, so a record of 30 is no record: the piece ending in `-`
  // joins the word below it, and the next filled word does not.
  const width = 40;
  const first = `http://a.dev/${"a".repeat(width - 14)}-`;
  const record = Array.from({ length: 30 }, (_, i) => `│ a │ ${i === 0 ? first : String.fromCharCode(98 + (i % 20)).repeat(width)} │`);
  const rows = ruled(record);
  const url = `${first}${"c".repeat(width)}`;
  for (let y = 2; y <= 31; y++) expect(brokenAt(rows, y).map((link) => link.url)).toEqual(y <= 3 ? [url] : []);
});

it("joins an address in a box where its piece ends in a mark", () => {
  const rows = ["│ see http://a.dev/sel- │", "│ account&x=1           │"].map((text) => cellRow(text, 40));
  expect(brokenAt(rows, 2).map((link) => link.url)).toEqual(["http://a.dev/sel-account&x=1"]);
});

it.each([
  ["a list item", "- item"],
  ["a numbered item", "1. item"],
])("leaves an address in a cell alone where %s follows it", (_name, item) => {
  const rows = ruled([`│ a │ ${"http://a.dev/?l=zh-Hans".padEnd(30)} │`, `│   │ ${item.padEnd(30)} │`]);
  expect(brokenAt(rows, 2)).toEqual([]);
  expect(brokenAt(rows, 3)).toEqual([]);
});

it.each([
  ["its borders stand at other columns", `│      │ ${"x".padEnd(FILLED.length)}│`],
  ["a cell holds a border of its own", `│      │ ab │ ${"x".padEnd(FILLED.length - 5)} │`],
])("ends a cell's address at a row where %s", (_name, next) => {
  const rows = [`│ id   │ ${FILLED} │`, next].map((text) => cellRow(text, 80));
  expect(brokenAt(rows, 1)).toEqual([]);
  expect(brokenAt(rows, 2)).toEqual([]);
});

it("joins no row outside a table onto a cell's address", () => {
  const rows = [cellRow("│ x │ http://a.dev/sel- │", 40), cellRow("  account&x=1", 40)];
  expect(brokenAt(rows, 1)).toEqual([]);
  expect(brokenAt(rows, 2)).toEqual([]);
});

it("stops an address in a cell at the cap, counting each row at the terminal's width", () => {
  // 80 columns: 25 rows hold 2000 characters, the 26th takes the chain past 2048.
  const width = 40;
  const record = Array.from({ length: 26 }, (_, i) => `│ a │ ${i === 0 ? `http://a.dev/${"a".repeat(width - 13)}` : "b".repeat(width)} │`);
  const rows = ruled(record);
  const [link] = brokenAt(rows, 2);
  expect(link.url).toBe(`http://a.dev/${"a".repeat(width - 13)}${"b".repeat(width * 24)}`);
  expect(link.range).toEqual(piece(2, 6, width));
  expect(brokenAt(rows, 26)[0].url).toBe(link.url);
  expect(brokenAt(rows, 27)).toEqual([]);
});


it("colours OSC 8 labels by their cells and accepts only web targets, with a safe fallback", () => {
  const cells = [
    ["看", 2, 1], ["", 0, 1], [" ", 1, 1], ["x", 1, 1],
    ["f", 1, 2], ["!", 1, 3], [" ", 1, 0],
  ] as const;
  const term = {
    cols: cells.length,
    rows: 1,
    _core: { _oscLinkService: { getLinkData: (id: number) => ({ uri: ["", "https://example.com", "file:///tmp/x", "bad"][id] }) } },
    buffer: { active: {
      viewportY: 0,
      getNullCell: () => ({}),
      getLine: (y: number) => y === 0 ? {
        isWrapped: false,
        getCell: (x: number) => ({ getChars: () => cells[x][0], getWidth: () => cells[x][1], hasExtendedAttrs: () => cells[x][2], extended: { urlId: cells[x][2] || 1 } }),
      } : undefined,
    } },
  };
  expect(visibleLinkSegments(term as unknown as Terminal)).toEqual([{ y: 1, col: 0, width: 4 }]);
  Object.assign(term, { _core: undefined });
  expect(visibleLinkSegments(term as unknown as Terminal)).toEqual([]);
});
