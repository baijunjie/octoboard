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
