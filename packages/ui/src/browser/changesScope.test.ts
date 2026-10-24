import { expect, it } from "vitest";

import type { BranchInfo, GitSourceInfo, UpstreamInfo, WorktreeInfo } from "../protocol";
import { changesScope as scopeOf } from "./changesScope";
import type { BranchList } from "./useBranchList";

const tree = (id: string, branch: string | null, main = false): WorktreeInfo => ({ id, root: `/${id}`, main, head: "c1", branch, scope_present: true });
const repository = (own: string, worktrees: WorktreeInfo[]): GitSourceInfo => ({ repository: "repo", common_dir: "/r/.git", worktree: own, scope: "", worktrees });
const loaded = (branches: BranchInfo[], complete = true, upstreamsComplete = true, defaultBranch?: UpstreamInfo): BranchList => ({ state: "loaded", branches, complete, upstreamsComplete, defaultBranch, sentAt: 1 });
const branch = (name: string, upstream?: string): BranchInfo => ({ name, commit: "c1", upstream: upstream ? { name: upstream, commit: "c0" } : undefined });
const LISTED = loaded([branch("main", "origin/main"), branch("topic")]);
const changesScope = (git: GitSourceInfo | null, chosen: string | undefined, branches: BranchList = LISTED) => scopeOf(git, chosen, branches);
const MAIN = tree("m", "main", true);
const TOPIC = tree("w", "topic");

it("defaults to the branch of the worktree holding the project's directory, which is where its changes are read", () => {
  expect(changesScope(repository("m", [MAIN, TOPIC]), undefined)).toMatchObject({ ownBranch: "main", branch: "main", holderId: undefined });
});

it("compares the main working copy's own branch with the remote branch the list gives it", () => {
  expect(changesScope(repository("m", [MAIN, TOPIC]), undefined).committed).toEqual({ state: "compare", base: "origin/main", baseKind: "remote", branch: "main" });
  expect(changesScope(repository("m", [MAIN, TOPIC]), "main").committed).toEqual({ state: "compare", base: "origin/main", baseKind: "remote", branch: "main" });
});

it("compares the main working copy's own branch with nothing when the list gives it no remote branch", () => {
  const git = repository("m", [MAIN, TOPIC]);
  const own = { state: "compare", base: "", baseKind: "empty", branch: "main" };
  expect(changesScope(git, "main", loaded([branch("main"), branch("topic")])).committed).toEqual(own);
});

it("compares the main working copy's own branch with the remote default branch when it has no remote branch", () => {
  const git = repository("m", [MAIN, TOPIC]);
  const trunk = { name: "origin/trunk", commit: "c0" };
  expect(changesScope(git, "main", loaded([branch("main"), branch("topic")], true, true, trunk)).committed).toEqual({ state: "compare", base: "origin/trunk", baseKind: "remote", branch: "main" });
  // Its own remote branch comes first.
  expect(changesScope(git, "main", loaded([branch("main", "origin/main"), branch("topic")], true, true, trunk)).committed).toMatchObject({ base: "origin/main" });
  // Another branch is compared with the main working copy's, not the default branch.
  expect(changesScope(git, "topic", loaded([branch("main"), branch("topic")], true, true, trunk)).committed).toMatchObject({ base: "main", baseKind: "local" });
});

it("waits for the branch list, and the branch in it, before saying anything of the main working copy's own branch", () => {
  const git = repository("m", [MAIN, TOPIC]);
  expect(changesScope(git, "main", { state: "loading" }).committed).toEqual({ state: "none" });
  expect(changesScope(git, "main", { state: "error", message: "x" }).committed).toEqual({ state: "none" });
  expect(changesScope(git, "main", loaded([branch("topic")])).committed).toEqual({ state: "none" });
  expect(changesScope(git, "main", loaded([branch("main")], false)).committed).toEqual({ state: "compare", base: "", baseKind: "empty", branch: "main" });
});

it("says nothing of the main working copy's own branch when the remote branches could not be listed", () => {
  const git = repository("m", [MAIN, TOPIC]);
  expect(changesScope(git, "main", loaded([branch("main"), branch("topic")], true, false)).committed).toEqual({ state: "none" });
  // An upstream it did find is used.
  expect(changesScope(git, "main", loaded([branch("main", "origin/main"), branch("topic")], true, false)).committed).toMatchObject({ state: "compare", base: "origin/main", baseKind: "remote" });
});

it("leaves another branch compared with the main working copy's, whatever the branch list says", () => {
  const git = repository("m", [MAIN, TOPIC]);
  const upstream = loaded([branch("main", "origin/main"), branch("topic", "origin/topic")]);
  expect(changesScope(git, "topic", upstream).committed).toEqual({ state: "compare", base: "main", baseKind: "local", branch: "topic" });
  expect(changesScope(git, "topic", { state: "loading" }).committed).toEqual({ state: "compare", base: "main", baseKind: "local", branch: "topic" });
});

it("reads a chosen branch's uncommitted changes from the worktree that has it checked out, and compares it with the main working copy's", () => {
  const scope = changesScope(repository("m", [MAIN, TOPIC]), "topic");
  expect(scope).toMatchObject({ branch: "topic", holder: TOPIC, holderId: "w", committed: { state: "compare", base: "main", baseKind: "local", branch: "topic" } });
});

it("has no holder for a branch checked out nowhere, which is still compared", () => {
  expect(changesScope(repository("m", [MAIN, TOPIC]), "loose")).toMatchObject({ branch: "loose", holder: undefined, holderId: undefined, committed: { state: "compare", base: "main", baseKind: "local", branch: "loose" } });
});

it("compares nothing while the main working copy is on no branch", () => {
  expect(changesScope(repository("w", [tree("m", null, true), TOPIC]), undefined)).toMatchObject({ branch: "topic", committed: { state: "noBase" } });
});

it("has no committed part for a branch with no commit yet, nor against a main working copy with none", () => {
  const unborn = (id: string, branch: string, main = false): WorktreeInfo => ({ ...tree(id, branch, main), head: null });
  expect(changesScope(repository("m", [MAIN, unborn("w", "topic")]), "topic")).toMatchObject({ branch: "topic", committed: { state: "none" } });
  expect(changesScope(repository("m", [unborn("m", "main", true), TOPIC]), "topic")).toMatchObject({ branch: "topic", committed: { state: "none" } });
  expect(changesScope(repository("m", [unborn("m", "main", true)]), undefined, loaded([]))).toMatchObject({ branch: "main", committed: { state: "none" } });
});

it("names no branch while the project's worktree is detached and none is chosen", () => {
  expect(changesScope(repository("d", [MAIN, tree("d", null)]), undefined)).toMatchObject({ ownBranch: undefined, branch: undefined, holder: undefined, committed: { state: "none" } });
});

it("has nothing outside a repository", () => {
  expect(changesScope(null, "topic")).toMatchObject({ branch: "topic", holder: undefined, committed: { state: "noBase" } });
});
