import type { ILinkProvider, Terminal } from "@xterm/xterm";
import { expect, it, vi } from "vitest";

import { foundPathMatches, installPathLinks, type PathAnswer, PathLinkCache, pathCandidates, pathMatchesIn } from "./pathLinks";
import type { TerminalRow } from "./terminalLines";

const paths = (text: string) => pathCandidates(text).map((candidate) => candidate.path);

it.each<[string, string, number | undefined]>([
  ["a :line", "at src/a.ts:42 it fails", 42],
  ["a :line:col", "at src/a.ts:42:7: it fails", 42],
  ["a compiler's (line,col)", "src/a.ts(12,5): error TS2322", 12],
  ["no position", "see src/a.ts now", undefined],
])("reads the line from %s", (_name, text, line) => {
  expect(pathCandidates(text)[0].line).toBe(line);
});

it.each<[string, string, string[]]>([
  ["an absolute path", "wrote /Volumes/x/icons/tray.png now", ["/Volumes/x/icons/tray.png"]],
  ["a home-relative path", "see ~/notes/a.md", ["~/notes/a.md"]],
  ["a relative path with a directory", "edit `packages/ui/src/foo.ts`", ["packages/ui/src/foo.ts"]],
  ["a file name with a line", "at foo.ts:42:7 it fails", ["foo.ts"]],
  ["a path ending a sentence", "saved to /tmp/a.png.", ["/tmp/a.png"]],
  ["a path in parentheses", "(see /tmp/a.png)", ["/tmp/a.png"]],
  ["a path before a colon", "/tmp/a.png: written", ["/tmp/a.png"]],
  ["a compiler's position", "src/a.ts(12,5): error TS2322", ["src/a.ts"]],
  ["an @ mention", "look at @src/a.ts and @/tmp/b.png", ["src/a.ts", "/tmp/b.png"]],
  ["a path in CJK prose", "已写入 /tmp/图片.png。", ["/tmp/图片.png"]],
])("finds %s", (_name, text, expected) => {
  expect(paths(text)).toEqual(expected);
});

it.each([
  ["an address", "open https://example.com/a/b now"],
  ["a bare word", "the README and a.b are not paths"],
  ["a lone slash", "a / b"],
])("does not find a path in %s", (_name, text) => {
  expect(paths(text)).toEqual([]);
});

it.each([
  ["a route pattern", "open app/[id]/page.tsx now"],
  ["a route group", "open app/(group)/page.tsx"],
])("does not find a path in %s", (_name, text) => {
  expect(paths(text)).toEqual([]);
});

it("starts an @ mention's path after the @", () => {
  const [candidate] = pathCandidates("x @src/a.ts:3");
  expect("x @src/a.ts:3".slice(candidate.start, candidate.end)).toBe("src/a.ts:3");
});

it("ends the candidate before the line suffix's trailing punctuation", () => {
  const [candidate] = pathCandidates("x foo.ts:42: bad");
  expect("x foo.ts:42: bad".slice(candidate.start, candidate.end)).toBe("foo.ts:42");
});

const COLS = 20;
function row(text: string, wrapped = false): TerminalRow {
  const cells = [...text].flatMap((char) => (char.charCodeAt(0) > 0xff ? [char, ""] : [char]));
  return { cells: [...cells, ...Array(COLS - cells.length).fill(" ")], wrapped };
}

it("places a path soft-wrapped onto the next row, in cells", () => {
  const rows = [row("see /tmp/aaaa/bbbbbb"), row("b/c.png ok", true)];
  const [match] = pathMatchesIn((y) => rows[y - 1], 1, 2);
  expect(match.path).toBe("/tmp/aaaa/bbbbbbb/c.png");
  expect(match.segments).toEqual([{ y: 1, col: 4, width: 16 }, { y: 2, col: 0, width: 7 }]);
  expect(match.ranges).toEqual([{ start: { x: 5, y: 1 }, end: { x: 7, y: 2 } }]);
});

it("counts a double-width character as two cells", () => {
  const [match] = pathMatchesIn((y) => [row("/a/图.png")][y - 1], 1, 1);
  expect(match.segments).toEqual([{ y: 1, col: 0, width: 9 }]);
});

// A line the program broke onto the next row in the middle of a path, laid out 30 columns wide:
// the first piece fills the row, and the next row starts with its indent.
const BROKEN_PATH = "/tmp/scratch/icon/set.iconset/icon.png";
function sized(cols: number, text: string, wrapped = false): TerminalRow {
  return { cells: [...text.padEnd(cols)], wrapped };
}
const matchesAt = (rows: TerminalRow[], from: number, to = from) => pathMatchesIn((y) => rows[y - 1], from, to);

it("finds a path the program broke onto the next row, as the whole path and as its pieces", () => {
  const rows = [
    sized(30, "  Checked it, looks right:"),
    sized(30, "  /tmp/scratch/icon/set.iconse"),
    sized(30, "  t/icon.png. I have not seen"),
  ];
  const matches = matchesAt(rows, 1, 3);
  const joined = matches.find((match) => match.joined);
  expect(joined?.path).toBe(BROKEN_PATH);
  expect(joined?.segments).toEqual([{ y: 2, col: 2, width: 28 }, { y: 3, col: 2, width: 10 }]);
  expect(joined?.ranges).toEqual([{ start: { x: 3, y: 2 }, end: { x: 12, y: 3 } }]);
  expect(matches.filter((match) => !match.joined).map((match) => match.path)).toEqual(["/tmp/scratch/icon/set.iconse", "t/icon.png"]);
});

it("finds a program-broken path in output replayed into a narrower terminal", () => {
  // The same output at 20 columns: xterm soft-wrapped the full row, which leaves a short piece.
  const rows = [sized(20, "  /tmp/scratch/icon/"), sized(20, "set.iconse", true), sized(20, "  t/icon.png. ok")];
  const joined = matchesAt(rows, 1, 3).find((match) => match.joined);
  expect(joined?.path).toBe(BROKEN_PATH);
  expect(joined?.segments).toEqual([
    { y: 1, col: 2, width: 18 },
    { y: 2, col: 0, width: 10 },
    { y: 3, col: 2, width: 10 },
  ]);
});

it("finds a program-broken path whose row is taken on from a word above it", () => {
  // Replayed at 20 columns, the line above leaves `right:` on a short row, which the path row
  // joins.
  const rows = [
    sized(20, "  Checked it, looks "),
    sized(20, "right:", true),
    sized(20, "  /tmp/scratch/icon/"),
    sized(20, "set.iconse", true),
    sized(20, "  t/icon.png. ok"),
  ];
  const joined = matchesAt(rows, 1, 5).filter((match) => match.joined).map((match) => match.path);
  expect(joined).toContain(BROKEN_PATH);
});

it("finds a program-broken path from the row its last piece is on", () => {
  const rows = [sized(30, "  Checked it:"), sized(30, "  /tmp/scratch/icon/set.iconse"), sized(30, "  t/icon.png. ok")];
  expect(matchesAt(rows, 3).find((match) => match.joined)?.path).toBe(BROKEN_PATH);
});

it.each([
  ["an empty row", [sized(30, "  /tmp/scratch/icon/set.iconse"), sized(30, "  ")]],
  ["a row that starts with a bracket", [sized(30, "  /tmp/scratch/icon/set.iconse"), sized(30, "  (see above)")]],
  ["a row after a line that stops short", [sized(30, "  /tmp/a/b.png"), sized(30, "  t/icon.png. ok")]],
])("joins no path to %s", (_name, rows) => {
  expect(matchesAt(rows, 1, 2).some((match) => match.joined)).toBe(false);
});

// The path row, 83 cells of text in a terminal 140 wide, and the path it makes with the next row.
const WIDE_PATH = "  the file is at /tmp/scratch/aaaaaaaaaaaaaaaa/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const WIDE_JOINED = "/tmp/scratch/aaaaaaaaaaaaaaaa/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/icon.png";
const joinedPaths = (rows: TerminalRow[], from: number, to = from) =>
  matchesAt(rows, from, to).filter((match) => match.joined).map((match) => match.path);

it("joins a path broken where the program's text width ended, short of the terminal's", () => {
  // Laid out 83 columns wide, shown in 140: the longest line of the block is the one that broke.
  const rows = [sized(140, "  A line of prose above, shorter."), sized(140, WIDE_PATH), sized(140, "  /icon.png ok")];
  expect(WIDE_PATH.length).toBe(83);
  expect(joinedPaths(rows, 2)).toEqual([WIDE_JOINED]);
});

it("does not join a path to a row when a longer line of its block is the one that filled the width", () => {
  const rows = [sized(140, `${WIDE_PATH} and more words after it, filling the line`), sized(140, WIDE_PATH), sized(140, "  /icon.png ok")];
  expect(joinedPaths(rows, 2)).toEqual([]);
});

it("joins a path broken at the terminal's width though a longer soft-wrapped line is in its block", () => {
  const rows = [
    sized(30, "  /tmp/scratch/icon/set.iconse"),
    sized(30, "  t/icon.png. ok"),
    sized(30, "a line that fills the row up"),
    sized(30, "and goes on and on", true),
  ];
  expect(joinedPaths(rows, 1, 2)).toContain(BROKEN_PATH);
});

it("does not join a listing's short rows, though they are the longest of their block", () => {
  expect(joinedPaths([sized(30, "  src/"), sized(30, "  lib/")], 1, 2)).toEqual([]);
});

// A line longer than the path row, which would be the one that filled the width if it counted.
const LONGER = sized(140, `${WIDE_PATH} and more words after it, filling the line`);
const BLANK = sized(140, "");

it.each([
  ["above", [LONGER, BLANK, sized(140, WIDE_PATH), sized(140, "  /icon.png ok")], 3],
  ["below", [sized(140, WIDE_PATH), sized(140, "  /icon.png ok"), BLANK, LONGER], 1],
])("does not count a longer line %s a blank row as one of the block", (_side, rows, at) => {
  expect(joinedPaths(rows, at)).toEqual([WIDE_JOINED]);
  expect(joinedPaths(rows, 1, rows.length)).toContain(WIDE_JOINED);
});

it.each<[string, number, boolean]>([
  ["above", 10, false],
  ["above", 24, true],
  ["below", 10, false],
  ["below", 24, true],
])("learns the layout from a longer line %s only within reach: %i rows between, joined %s", (side, between, joins) => {
  // Reach is 20 rows. The filler is not blank, so it does not end the block.
  const filler = Array.from({ length: between }, () => sized(140, "lorem ipsum (dolor)"));
  const path = [sized(140, WIDE_PATH), sized(140, "  /icon.png ok")];
  const rows = side === "above" ? [LONGER, ...filler, ...path] : [...path, ...filler, LONGER];
  const at = side === "above" ? rows.length - 1 : 1;
  const expected = joins ? [WIDE_JOINED] : [];
  expect(joinedPaths(rows, at)).toEqual(expected);
  expect(joinedPaths(rows, 1, rows.length)).toEqual(expected);
  expect(joinedPaths(rows, at, rows.length)).toEqual(expected);
});

it("finds a path crossing two breaks once, though it is looked for from each break", () => {
  const rows = [
    sized(30, "  abcdefghijklmnopqrstuvwxyzab"),
    sized(30, "  note /tmp/scratch/iconsetxyz"),
    sized(30, "  bbb/c.png ok"),
  ];
  expect(joinedPaths(rows, 1, 3)).toEqual(["/tmp/scratch/iconsetxyzbbb/c.png"]);
});

it("links the pieces of a broken path when the joined path is not one, and the whole when it is", () => {
  // A full-width row ending in a complete path, then an unrelated word.
  const rows = [sized(20, "  see /tmp/aaaa/b.png"), sized(20, "  tomorrow ok")];
  const matches = matchesAt(rows, 1, 2);
  expect(matches.find((match) => match.joined)?.path).toBe("/tmp/aaaa/b.pngtomorrow");
  const whole = (path: string) => path === "/tmp/aaaa/b.png";
  expect(foundPathMatches(matches, whole).map((match) => match.path)).toEqual(["/tmp/aaaa/b.png"]);

  const broken = [sized(30, "  /tmp/scratch/icon/set.iconse"), sized(30, "  t/icon.png. ok")];
  const pieces = matchesAt(broken, 1, 2);
  expect(foundPathMatches(pieces, (path) => path === BROKEN_PATH).map((match) => match.path)).toEqual([BROKEN_PATH]);
  expect(foundPathMatches(pieces, (path) => path !== BROKEN_PATH).map((match) => match.path)).toEqual(["/tmp/scratch/icon/set.iconse", "t/icon.png"]);
});

// One record of a ruled table whose left cell wraps a path inside it, beside a cell holding a path
// on each row.
const RULE = "─".repeat(30);
const PATH_RECORD = [
  "│ /tmp/scratch/icon/set.iconse │ /tmp/a/b.png                 │",
  "│ t/icon.png                   │ /tmp/c/d.png                 │",
];
const PATH_TABLE = [`┌${RULE}┬${RULE}┐`, ...PATH_RECORD, `└${RULE}┴${RULE}┘`].map((text) => sized(70, text));

it("joins a path wrapped inside a table cell, without the borders or the padding", () => {
  const joined = matchesAt(PATH_TABLE, 2, 3).filter((match) => match.joined);
  expect(joined.map((match) => match.path)).toEqual([BROKEN_PATH]);
  expect(joined[0].segments).toEqual([{ y: 2, col: 2, width: 28 }, { y: 3, col: 2, width: 10 }]);
  expect(joined[0].ranges).toEqual([
    { start: { x: 3, y: 2 }, end: { x: 30, y: 2 } },
    { start: { x: 3, y: 3 }, end: { x: 12, y: 3 } },
  ]);
});

it("keeps a path in another column that lies between the ends of a joined one", () => {
  const found = (path: string) => path === BROKEN_PATH || path === "/tmp/a/b.png";
  expect(foundPathMatches(matchesAt(PATH_TABLE, 2, 3), found).map((match) => match.path)).toEqual(["/tmp/a/b.png", BROKEN_PATH]);
});

it("joins no path whose piece stops short of its cell's edge in a ruled record", () => {
  const rows = [
    `┌${RULE}┬${RULE}┐`,
    "│ /tmp/scratch/icon/set.icon   │ x                            │",
    "│ t/icon.png                   │ y                            │",
    `└${RULE}┴${RULE}┘`,
  ].map((text) => sized(70, text));
  expect(matchesAt(rows, 1, 4).some((match) => match.joined)).toBe(false);
});

it.each([
  ["a table that does not rule its records apart", PATH_RECORD],
  ["a box", ["│ /tmp/scratch/icon/set.iconse │", "│ t/icon.png                   │"]],
])("joins no path that fills its cell in %s", (_name, texts) => {
  const rows = texts.map((text) => sized(70, text));
  expect(matchesAt(rows, 1, 2).some((match) => match.joined)).toBe(false);
});


function cache(answer: (paths: string[]) => PathAnswer[], now = () => 0) {
  const probe = vi.fn(async (asked: string[]) => answer(asked));
  const onChange = vi.fn();
  return { cache: new PathLinkCache(probe, onChange, now, 0), probe, onChange };
}

it("asks about paths together, once, and links only the ones found", async () => {
  const { cache: links, probe, onChange } = cache((asked) => asked.map((path) => ({ kind: path === "/a" ? "file" : "none" })));
  links.want(["/a", "/b", "/a"]);
  links.want(["/b"]);
  expect(links.kind("/a")).toBeUndefined();
  await vi.waitFor(() => expect(onChange).toHaveBeenCalledOnce());
  expect(probe).toHaveBeenCalledExactlyOnceWith(["/a", "/b"]);
  expect(links.kind("/a")).toBe("file");
  expect(links.kind("/b")).toBeUndefined();
});

it("keeps the view the daemon gave a path beside its kind", async () => {
  const view = { worktree: "w", path: "src/a.ts" };
  const { cache: links, onChange } = cache(() => [{ kind: "file", view }]);
  links.want(["/a"]);
  await vi.waitFor(() => expect(onChange).toHaveBeenCalledOnce());
  expect(links.answerFor("/a").view).toEqual(view);
  expect(links.answerFor("/b").view).toBeUndefined();
});

it("gives a click the kind of the path beside its view", async () => {
  const view = { path: "docs/README.md" };
  const { cache: links, onChange } = cache(() => [{ kind: "directory", view }]);
  links.want(["/docs"]);
  await vi.waitFor(() => expect(onChange).toHaveBeenCalledOnce());
  expect(links.answerFor("/docs")).toEqual({ kind: "directory", view, viewerOnly: undefined });
});

it("asks again only once the answer is old, and says nothing when it did not change", async () => {
  let time = 0;
  const { cache: links, probe, onChange } = cache(() => [{ kind: "none" }], () => time);
  links.want(["/a"]);
  await vi.waitFor(() => expect(probe).toHaveBeenCalledOnce());
  links.want(["/a"]);
  time = 60_000;
  links.want(["/a"]);
  await vi.waitFor(() => expect(probe).toHaveBeenCalledTimes(2));
  expect(onChange).not.toHaveBeenCalled();
});

it("drops an answer that arrives after the cache was cleared", async () => {
  let answer: (kinds: PathAnswer[]) => void = () => {};
  const probe = vi.fn(() => new Promise<PathAnswer[]>((resolve) => { answer = resolve; }));
  const onChange = vi.fn();
  const links = new PathLinkCache(probe, onChange, () => 0, 0);
  links.want(["/a"]);
  await vi.waitFor(() => expect(probe).toHaveBeenCalledOnce());
  links.clear();
  answer([{ kind: "file" }]);
  await Promise.resolve();
  await Promise.resolve();
  expect(links.kind("/a")).toBeUndefined();
  expect(onChange).not.toHaveBeenCalled();
});

it("keeps what it knew when a probe fails, and takes a path it knew nothing of for no path", async () => {
  let fail = false;
  const probe = vi.fn(async (asked: string[]): Promise<PathAnswer[]> => {
    if (fail) throw new Error("down");
    return asked.map((): PathAnswer => ({ kind: "file" }));
  });
  let time = 0;
  const onChange = vi.fn();
  const links = new PathLinkCache(probe, onChange, () => time, 0);
  links.want(["/known"]);
  await vi.waitFor(() => expect(links.kind("/known")).toBe("file"));
  fail = true;
  time = 60_000;
  links.want(["/known", "/new"]);
  await vi.waitFor(() => expect(probe).toHaveBeenCalledTimes(2));
  await vi.waitFor(() => expect(onChange).toHaveBeenCalledTimes(1));
  await Promise.resolve();
  expect(links.kind("/known")).toBe("file");
  expect(links.kind("/new")).toBeUndefined();
  // The failed answer counts as fresh: neither is asked about again at once.
  links.want(["/known", "/new"]);
  await new Promise((resolve) => setTimeout(resolve, 5));
  expect(probe).toHaveBeenCalledTimes(2);
});

it("does not ask about a path that left the screen before the batch went", async () => {
  const { cache: links, probe } = cache((asked) => asked.map((): PathAnswer => ({ kind: "file" })));
  links.want(["/gone", "/here"]);
  links.setVisible(["/here"]);
  await vi.waitFor(() => expect(probe).toHaveBeenCalledOnce());
  expect(probe).toHaveBeenCalledWith(["/here"]);
  // Dropped, not remembered: on screen again it is asked about.
  links.setVisible(["/gone"]);
  links.want(["/gone"]);
  await vi.waitFor(() => expect(probe).toHaveBeenCalledTimes(2));
});

it("sends at most 64 paths a request and two requests at a time", async () => {
  const answers: (() => void)[] = [];
  const probe = vi.fn((asked: string[]) => new Promise<PathAnswer[]>((resolve) => { answers.push(() => resolve(asked.map((): PathAnswer => ({ kind: "none" })))); }));
  const links = new PathLinkCache(probe, vi.fn(), () => 0, 0);
  links.want(Array.from({ length: 64 * 3 }, (_, i) => `/p${i}`));
  await vi.waitFor(() => expect(probe).toHaveBeenCalledTimes(2));
  expect(probe.mock.calls.map(([asked]) => asked.length)).toEqual([64, 64]);
  await new Promise((resolve) => setTimeout(resolve, 5));
  expect(probe).toHaveBeenCalledTimes(2);
  answers[0]();
  await vi.waitFor(() => expect(probe).toHaveBeenCalledTimes(3));
  answers[1]();
  answers[2]();
});

/** The path link provider over `rows`, once the daemon, which finds every path, has answered for `wanted`. */
async function pathProvider(rows: TerminalRow[], wanted: string[]) {
  const term = {
    cols: rows[0].cells.length,
    registerLinkProvider: vi.fn(),
    buffer: { active: {
      getNullCell: () => ({}),
      getLine: (y: number) => rows[y] && {
        isWrapped: rows[y].wrapped,
        getCell: (x: number) => ({ getChars: () => rows[y].cells[x].trim(), getWidth: () => 1 }),
      },
    } },
  };
  const { cache: links } = installPathLinks(
    term as unknown as Terminal,
    { probe: async (asked) => asked.map((): PathAnswer => ({ kind: "file" })), open: vi.fn(async () => {}) },
    () => false,
    () => {},
  );
  const provider = term.registerLinkProvider.mock.calls[0][0] as ILinkProvider;
  links.want(wanted);
  await vi.waitFor(() => expect(wanted.every((path) => links.kind(path) === "file")).toBe(true));
  return (y: number) => {
    const found = vi.fn();
    provider.provideLinks(y, found);
    return found.mock.calls[0][0]?.map(({ text, range }: { text: string; range: unknown }) => ({ text, range }));
  };
}

it("offers a path only on the rows it fills, though its line is under the pointer on others", async () => {
  // One soft-wrapped line: the path is on the first row, and the pointer is on the second.
  const rows = [sized(20, "see /tmp/a.png and m"), sized(20, "ore text", true)];
  const linksAt = await pathProvider(rows, ["/tmp/a.png"]);
  expect(linksAt(1)?.map(({ text }: { text: string }) => text)).toEqual(["/tmp/a.png"]);
  expect(linksAt(2)).toBeUndefined();
});

it("offers a path wrapped inside a table cell on each of its pieces, as the whole path", async () => {
  const linksAt = await pathProvider(PATH_TABLE, [BROKEN_PATH]);
  expect(linksAt(3)).toEqual([{ text: BROKEN_PATH, range: { start: { x: 3, y: 3 }, end: { x: 12, y: 3 } } }]);
});
