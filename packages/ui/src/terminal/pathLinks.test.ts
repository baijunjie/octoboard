import { expect, it, vi } from "vitest";

import { type PathAnswer, PathLinkCache, pathCandidates, pathMatchesIn } from "./pathLinks";
import type { TerminalRow } from "./terminalLinks";

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
  expect(match.range).toEqual({ start: { x: 5, y: 1 }, end: { x: 7, y: 2 } });
});

it("counts a double-width character as two cells", () => {
  const [match] = pathMatchesIn((y) => [row("/a/图.png")][y - 1], 1, 1);
  expect(match.segments).toEqual([{ y: 1, col: 0, width: 9 }]);
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
