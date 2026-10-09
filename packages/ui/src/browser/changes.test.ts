import { expect, it } from "vitest";

import type { ChangeEntry, ChangeSide, ContentSource } from "../protocol";
import { changeEvidence, changeItem, changeItems, changeNeighbours, rereadChange, type ChangeEvidence, type ShownChange } from "./changes";

const index = (blob: string): ContentSource => ({ kind: "index", worktree: "w", blob });
const at = (path: string, blob = "b1"): ChangeSide => ({ state: "present", path, kind: "file", source: index(blob) });
const absent: ChangeSide = { state: "absent" };

const entries: ChangeEntry[] = [
  { group: "untracked", old: absent, new: at("new.txt") },
  { group: "unstaged", old: at("src/a.ts"), new: at("src/a.ts", "live") },
  { group: "staged", old: at("src/a.ts", "head"), new: at("src/a.ts") },
  { group: "conflicted", path: "z.txt", conflict: "both_modified" },
  { group: "staged", old: { state: "out_of_scope", repository_path: "lib/b.ts" }, new: at("b.ts") },
];

it("orders sections as git status does, and paths within one as the file tree does", () => {
  expect(changeItems(entries).map((item) => `${item.section} ${item.path} ${item.status}`)).toEqual([
    "staged src/a.ts modified",
    "staged b.ts renamed",
    "conflicted z.txt conflicted",
    "unstaged src/a.ts modified",
    "untracked new.txt added",
  ]);
});

it("tells one path's staged and unstaged changes apart", () => {
  const [staged, unstaged] = [changeItem(entries[2]), changeItem(entries[1])];
  expect(staged.path).toBe(unstaged.path);
  expect(staged.key).not.toBe(unstaged.key);
});

it("moves from one section into the next, places a change the list lost where it was, and does not wrap", () => {
  const items = changeItems(entries);
  const lastStaged = items[1];
  expect(changeNeighbours(items, lastStaged).next?.section).toBe("conflicted");
  expect(changeNeighbours(items, items[0]).previous).toBeUndefined();
  expect(changeNeighbours(items, items[items.length - 1]).next).toBeUndefined();
  const gone = changeItem({ group: "unstaged", old: at("src/zz.ts"), new: absent });
  expect(changeNeighbours(items, gone)).toMatchObject({ previous: { path: "src/a.ts", section: "unstaged" }, next: { section: "untracked" } });
});

const listed = (versions: string): ChangeEvidence => ({ state: "listed", versions });
it.each<[string, ShownChange, ChangeEvidence, ChangeEvidence | undefined, boolean, boolean]>([
  ["the same versions listed again", { state: "read" }, listed("v1"), listed("v1"), true, false],
  ["other versions listed", { state: "read" }, listed("v2"), listed("v1"), true, true],
  ["the change gone from the list", { state: "read" }, { state: "gone" }, listed("v1"), true, true],
  ["a list that says nothing yet", { state: "read" }, { state: "unknown" }, listed("v1"), true, false],
  ["a change that kept moving, at a fresh list", { state: "error", changing: true }, listed("v1"), listed("v1"), true, true],
  ["a change that kept moving, at no fresh list", { state: "error", changing: true }, listed("v1"), listed("v1"), false, false],
  ["a read still out", { state: "loading" }, listed("v2"), listed("v1"), true, false],
])("reads a change again on %s: %s", (_, shown, evidence, previous, fresh, reread) => {
  expect(rereadChange(shown, evidence, previous, fresh)).toBe(reread);
});

it("finds a change in a list, gone from a complete one, unknown in a cut or refreshing one", () => {
  const items = changeItems(entries);
  const key = items[0].key;
  expect(changeEvidence({ items, complete: true, refreshing: false }, key)).toEqual(listed(items[0].versions));
  expect(changeEvidence({ items: [], complete: true, refreshing: false }, key)).toEqual({ state: "gone" });
  expect(changeEvidence({ items: [], complete: false, refreshing: false }, key)).toEqual({ state: "unknown" });
  expect(changeEvidence({ items, complete: true, refreshing: true }, key)).toEqual({ state: "unknown" });
});
