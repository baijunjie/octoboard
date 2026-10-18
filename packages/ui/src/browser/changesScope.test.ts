import { expect, it } from "vitest";

import type { GitSourceInfo, WorktreeInfo } from "../protocol";
import { changesScope } from "./changesScope";

const tree = (id: string, branch: string | null, main = false): WorktreeInfo => ({ id, root: `/${id}`, main, head: "c1", branch, scope_present: true });
const repository = (own: string, worktrees: WorktreeInfo[]): GitSourceInfo => ({ repository: "repo", common_dir: "/r/.git", worktree: own, scope: "", worktrees });
const MAIN = tree("m", "main", true);
const TOPIC = tree("w", "topic");

it("defaults to the branch of the worktree holding the project's directory, which is where its changes are read", () => {
  expect(changesScope(repository("m", [MAIN, TOPIC]), undefined)).toMatchObject({ ownBranch: "main", branch: "main", holderId: undefined, committed: { state: "isBase", branch: "main" } });
});

it("reads a chosen branch's uncommitted changes from the worktree that has it checked out, and compares it with the main working copy's", () => {
  const scope = changesScope(repository("m", [MAIN, TOPIC]), "topic");
  expect(scope).toMatchObject({ branch: "topic", holder: TOPIC, holderId: "w", committed: { state: "compare", base: "main", branch: "topic" } });
});

it("has no holder for a branch checked out nowhere, which is still compared", () => {
  expect(changesScope(repository("m", [MAIN, TOPIC]), "loose")).toMatchObject({ branch: "loose", holder: undefined, holderId: undefined, committed: { state: "compare", base: "main", branch: "loose" } });
});

it("compares nothing while the main working copy is on no branch", () => {
  expect(changesScope(repository("w", [tree("m", null, true), TOPIC]), undefined)).toMatchObject({ branch: "topic", committed: { state: "noBase" } });
});

it("has no committed part for a branch with no commit yet, nor against a main working copy with none", () => {
  const unborn = (id: string, branch: string, main = false): WorktreeInfo => ({ ...tree(id, branch, main), head: null });
  expect(changesScope(repository("m", [MAIN, unborn("w", "topic")]), "topic")).toMatchObject({ branch: "topic", committed: { state: "none" } });
  expect(changesScope(repository("m", [unborn("m", "main", true), TOPIC]), "topic")).toMatchObject({ branch: "topic", committed: { state: "none" } });
  expect(changesScope(repository("m", [unborn("m", "main", true)]), undefined)).toMatchObject({ branch: "main", committed: { state: "isBase", branch: "main" } });
});

it("names no branch while the project's worktree is detached and none is chosen", () => {
  expect(changesScope(repository("d", [MAIN, tree("d", null)]), undefined)).toMatchObject({ ownBranch: undefined, branch: undefined, holder: undefined, committed: { state: "none" } });
});

it("has nothing outside a repository", () => {
  expect(changesScope(null, "topic")).toMatchObject({ branch: "topic", holder: undefined, committed: { state: "noBase" } });
});
