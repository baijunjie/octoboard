// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import type { ChangeEntry, ChangeSide, ContentSource } from "../../protocol";
import { FADE_SIZE } from "../../components/scroll/scrollFade";
import { ROW_HEIGHT } from "../treeRow";
import { changeRowOffset, scrollToChange } from "./changeListLayout";
import { HEADING_HEIGHT } from "./changeSectionHeading";
import { changeItems, commitItems, commitKey, sectionKey, type ChangeItem, type ChangeSection } from "./changes";

const index = (blob: string): ContentSource => ({ kind: "index", worktree: "w", blob });
const at = (path: string, blob = "b1"): ChangeSide => ({ state: "present", path, kind: "file", source: index(blob) });
const absent: ChangeSide = { state: "absent" };

const unstaged = (path: string): ChangeEntry => ({ group: "unstaged", old: at(path, "head"), new: at(path, "live") });
const untracked = (path: string): ChangeEntry => ({ group: "untracked", old: absent, new: at(path) });

const keyOf = (items: readonly ChangeItem[], path: string): string => items.find((item) => item.path === path)!.key;

describe("changeRowOffset with kept sections", () => {
  // `src/` is a directory row above its files in the tree layout, so the layouts' offsets differ.
  const items = changeItems([unstaged("src/a.ts"), unstaged("src/b.ts"), untracked("c.ts")]);

  it.each([
    ["flat", "src/b.ts", HEADING_HEIGHT + ROW_HEIGHT],
    ["tree", "src/b.ts", HEADING_HEIGHT + 2 * ROW_HEIGHT],
  ] as const)("counts the heading of a kept section with no rows above the row, in the %s layout", (layout, path, plain) => {
    const row = keyOf(items, path);
    expect(changeRowOffset(items, row, layout, new Set())).toBe(plain);
    expect(changeRowOffset(items, row, layout, new Set(), ["staged"])).toBe(plain + HEADING_HEIGHT);
  });

  it("counts the heading of a kept section with no rows above a row in a later section", () => {
    const row = keyOf(items, "c.ts");
    expect(changeRowOffset(items, row, "flat", new Set(), ["staged"])).toBe(changeRowOffset(items, row, "flat", new Set())! + HEADING_HEIGHT);
  });

  it("counts the heading of a kept section with no rows above commit rows", () => {
    const commits = commitItems([{ commit: "c1", parent: null, subject: "first", changes: [{ group: "committed", old: at("x.ts", "o"), new: at("x.ts", "n") }] }]);
    expect(changeRowOffset(commits, commitKey("c1"), "flat", new Set())).toBe(HEADING_HEIGHT);
    expect(changeRowOffset(commits, commitKey("c1"), "flat", new Set(), ["staged"])).toBe(2 * HEADING_HEIGHT);
  });

  it("counts every kept section with no rows above the row, whatever order they are given in", () => {
    const row = keyOf(items, "src/a.ts");
    const plain = changeRowOffset(items, row, "flat", new Set())!;
    expect(changeRowOffset(items, row, "flat", new Set(), ["staged", "conflicted"])).toBe(plain + 2 * HEADING_HEIGHT);
    expect(changeRowOffset(items, row, "flat", new Set(), new Set<ChangeSection>(["conflicted", "staged"]))).toBe(plain + 2 * HEADING_HEIGHT);
  });

  it.each([
    ["has rows", ["unstaged", "untracked"]],
    ["has no rows and is below the row", ["committed"]],
  ] as const)("leaves the offsets as they are for a kept section that %s", (_name, kept) => {
    for (const row of items) {
      expect(changeRowOffset(items, row.key, "flat", new Set(), kept)).toBe(changeRowOffset(items, row.key, "flat", new Set()));
    }
  });
});

describe("scrollToChange", () => {
  const items = changeItems([unstaged("a.ts"), unstaged("b.ts"), unstaged("c.ts")]);
  const listOf = (height: number, scrollTop = 0): HTMLElement => {
    const list = document.createElement("div");
    Object.defineProperty(list, "clientHeight", { value: height, configurable: true });
    list.scrollTop = scrollTop;
    return list;
  };

  it("keeps the row clear of the sticky heading as well as the fade when it scrolls up", () => {
    const list = listOf(100, 1000);
    expect(scrollToChange(list, items, keyOf(items, "c.ts"), "flat", new Set())).toBe(true);
    // The row's top is a heading and two rows down; the scroll position leaves room above it for both.
    expect(list.scrollTop).toBe(HEADING_HEIGHT + 2 * ROW_HEIGHT - HEADING_HEIGHT - FADE_SIZE);
    expect(list.scrollTop).toBeGreaterThan(0);
  });

  it("scrolls by an offset that counts a kept section's heading above the row", () => {
    const row = keyOf(items, "c.ts");
    const plain = listOf(100, 1000);
    const kept = listOf(100, 1000);
    scrollToChange(plain, items, row, "flat", new Set());
    expect(scrollToChange(kept, items, row, "flat", new Set(), ["staged"])).toBe(true);
    expect(kept.scrollTop - plain.scrollTop).toBe(HEADING_HEIGHT);
  });

  it("scrolls nothing and reports false without a list, a key or a row on screen", () => {
    const row = keyOf(items, "c.ts");
    expect(scrollToChange(null, items, row, "flat", new Set())).toBe(false);
    const list = listOf(100, 50);
    expect(scrollToChange(list, items, undefined, "flat", new Set())).toBe(false);
    expect(scrollToChange(list, items, "no such row", "flat", new Set(), ["staged"])).toBe(false);
    expect(scrollToChange(list, items, row, "flat", new Set([sectionKey("unstaged")]))).toBe(false);
    expect(list.scrollTop).toBe(50);
  });
});
