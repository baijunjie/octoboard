import { expect, it } from "vitest";

import { parseStored } from "./browserState";

it("carries the layout an earlier version kept under `worktree` over to the Changes tab", () => {
  const raw = JSON.stringify({ p: { mode: "git", layouts: { worktree: "tree" }, expanded: [], used: 1 } });
  expect(parseStored(raw).p.layouts).toEqual({ changes: "tree" });
});

it("ignores the layout an earlier version kept for the Files tab", () => {
  const raw = JSON.stringify({ p: { mode: "files", layouts: { files: "flat", changes: "tree" }, expanded: [], used: 1 } });
  expect(parseStored(raw).p.layouts).toEqual({ changes: "tree" });
  expect(parseStored(JSON.stringify({ p: { mode: "files", layouts: { files: "flat" }, expanded: [], used: 1 } })).p.layouts).toEqual({});
});

it("prefers the Changes tab's own layout to the one carried over", () => {
  const raw = JSON.stringify({ p: { mode: "git", layouts: { changes: "flat", worktree: "tree" }, expanded: [], used: 1 } });
  expect(parseStored(raw).p.layouts).toEqual({ changes: "flat" });
});

it("drops a layout it does not know", () => {
  const raw = JSON.stringify({ p: { mode: "git", layouts: { worktree: "grid" }, expanded: [], used: 1 } });
  expect(parseStored(raw).p.layouts).toEqual({});
});
