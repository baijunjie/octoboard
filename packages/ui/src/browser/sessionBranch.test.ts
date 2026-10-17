import { expect, it } from "vitest";

import type { BranchInfo, WorktreeInfo } from "../protocol";
import { selectionForBranch, sessionBranch } from "./sessionBranch";

const made = (name: string, session: string | undefined, responsible_since: number): BranchInfo => ({
  name,
  commit: "c",
  origin: session === undefined ? undefined : { responsible_session: session, responsible_title: "t", responsible_since },
});
const tree = (id: string, branch: string | null, main = false): WorktreeInfo => ({ id, root: `/${id}`, main, head: "c", branch, scope_present: true });

it("takes the branch the session most recently became responsible for", () => {
  const branches = [made("old", "s", 1), made("new", "s", 5), made("other", "t", 9), made("bare", undefined, 0)];
  expect(sessionBranch(branches, "s")).toBe("new");
  expect(sessionBranch(branches, "nobody")).toBeUndefined();
});

it("follows the branch to its worktree and compares it with the main working copy", () => {
  const worktrees = [tree("m", "main", true), tree("w", "topic")];
  expect(selectionForBranch("topic", worktrees, "m")).toEqual({ worktree: "w", left: "main", right: "topic" });
  // The holder is the default worktree: no explicit choice. Not checked out anywhere: default too.
  expect(selectionForBranch("topic", [tree("m", "main", true), tree("w", "topic")], "w").worktree).toBeUndefined();
  expect(selectionForBranch("gone", worktrees, "m")).toEqual({ worktree: undefined, left: "main", right: "gone" });
});

it("leaves From empty for a detached or same-branch main working copy, and restores defaults without a branch", () => {
  expect(selectionForBranch("topic", [tree("m", null, true)], "m")).toMatchObject({ left: undefined, right: "topic" });
  expect(selectionForBranch("main", [tree("m", "main", true)], "m")).toMatchObject({ left: undefined, right: "main" });
  expect(selectionForBranch(undefined, [tree("m", "main", true)], "m")).toEqual({ worktree: undefined, left: undefined, right: undefined });
});
