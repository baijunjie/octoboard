import { expect, it } from "vitest";

import type { ErrorPresentation } from "../errorPresentation";
import type { WorktreeInfo } from "../protocol";
import { branchGone, changesStatus, partsLoading } from "./changesStatus";
import type { Comparison } from "./useBranchComparison";
import type { BranchList } from "./useBranchList";
import type { CommitList } from "./useCommitList";
import type { CommittedState, UncommittedState } from "./useGitReview";

const message = "boom" as unknown as ErrorPresentation;
const tree = (head: string | null, scopePresent = true): WorktreeInfo => ({ id: "w", root: "/home/me/code/repo", main: false, head, branch: "topic", scope_present: scopePresent });
const listed = (state: "loading" | "loaded" | "error" = "loaded", options: { worktree?: WorktreeInfo; own?: boolean; complete?: boolean } = {}): UncommittedState => ({
  state: "listed",
  worktree: options.worktree ?? tree("c1"),
  own: options.own ?? true,
  list:
    state === "loaded"
      ? { state, head: "c1", items: [], complete: options.complete ?? true, refreshing: false }
      : state === "error"
        ? { state, message, unavailable: false, refreshing: false }
        : { state },
});
const branchesOf = (names: string[], complete = true): BranchList => ({ state: "loaded", branches: names.map((name) => ({ name, commit: "c1" })), complete, sentAt: 20 });
const loadedComparison = (complete = true): Comparison => ({
  state: "loaded",
  left: { branch: "main", commit: "c1" },
  right: { branch: "topic", commit: "c2" },
  base: "c1000000000",
  items: [],
  complete,
  receivedAt: 10,
});
const compare = (comparison: Extract<CommittedState, { state: "compare" }>["comparison"], commits?: Extract<CommittedState, { state: "compare" }>["commits"]): CommittedState => ({
  state: "compare",
  base: "main",
  comparison,
  commits,
});
const status = (uncommitted: UncommittedState, committed: CommittedState, branches = branchesOf(["main", "topic"])) =>
  changesStatus({ branches, uncommitted, committed, homeDir: "/home/me" });

it("says a branch no worktree has checked out has no uncommitted changes, and an own branch nothing about its worktree", () => {
  expect(status({ state: "notCheckedOut", branch: "topic" }, { state: "none" }).notices).toEqual([{ id: "notCheckedOut", branch: "topic" }]);
  expect(status(listed(), { state: "none" }).notices).toEqual([]);
});

it("names the worktree another branch's changes are read from, with the home folder abbreviated, and a scope it lacks", () => {
  expect(status(listed("loaded", { own: false, worktree: tree("c1", false) }), { state: "none" }).notices).toEqual([
    { id: "worktree", path: "~/code/repo" },
    { id: "scopeMissing" },
  ]);
});

it("says a worktree with no commit yet has none", () => {
  expect(status(listed("loaded", { worktree: tree(null) }), { state: "isBase", branch: "topic" }).notices).toEqual([{ id: "unborn" }, { id: "isBase", branch: "topic" }]);
});

it("says what the committed part is while it is read, and what it is made of once it is", () => {
  expect(status(listed(), { state: "noBase" }).notices).toEqual([{ id: "noBase" }]);
  expect(status(listed(), compare({ state: "loading" })).notices).toEqual([{ id: "comparing", base: "main" }]);
  expect(status(listed(), compare(loadedComparison(), { state: "loading" })).notices).toEqual([
    { id: "base", base: "main", baseCommit: "c100000", tip: "c2" },
    { id: "listingCommits" },
  ]);
});

it("collects the parts that failed, in the order of the list", () => {
  const failures = status(listed("error"), compare(loadedComparison(), { state: "error", message })).failures;
  expect(failures.map(({ key, title }) => [key, title])).toEqual([
    ["uncommitted", "git.error.list"],
    ["commits", "git.changes.commitsFailed"],
  ]);
  expect(status(listed(), compare({ state: "error", message })).failures.map(({ key }) => key)).toEqual(["committed"]);
});

it("is partial when the uncommitted list, the comparison or the commits were cut, and says how many commits are listed when only the newest were", () => {
  expect(status(listed("loaded", { complete: false }), { state: "none" }).partial).toBe(true);
  expect(status(listed(), compare(loadedComparison(false))).partial).toBe(true);
  expect(status(listed(), compare(loadedComparison())).partial).toBe(false);
  const commits = (complete: boolean, commitsComplete: boolean): CommitList => ({ state: "loaded", base: "c1", tip: "c2", items: [], count: 100, complete, commitsComplete });
  // Grouped by commit, the commits' own completeness stands for the comparison's.
  expect(status(listed(), compare(loadedComparison(false), commits(true, true))).partial).toBe(false);
  expect(status(listed(), compare(loadedComparison(), commits(false, true))).partial).toBe(true);
  expect(status(listed(), compare(loadedComparison(), commits(true, false))).commitsCut).toBe(100);
  expect(status(listed(), compare(loadedComparison(), commits(true, true))).commitsCut).toBeUndefined();
  // Only the daemon's say that there were more commits than listed counts: a cut change budget has its own note.
  expect(status(listed("loaded", { complete: false }), compare(loadedComparison(false), commits(false, true))).commitsCut).toBeUndefined();
});

it("names the compared branches a complete list no longer has", () => {
  expect(status(listed(), compare(loadedComparison()), branchesOf(["main"])).goneBranches).toEqual(["topic"]);
  expect(status(listed(), compare(loadedComparison()), branchesOf(["main"], false)).goneBranches).toEqual([]);
});

it("tells which parts are still being read", () => {
  expect(partsLoading({ uncommitted: listed("loading"), committed: compare(loadedComparison(), { state: "loading" }) })).toEqual({ uncommitted: true, committed: true });
  expect(partsLoading({ uncommitted: listed(), committed: { state: "noBase" } })).toEqual({ uncommitted: false, committed: false });
});

it("takes a branch for gone only when a complete list lacks it and no worktree has it checked out with no commit yet", () => {
  expect(branchGone("topic", branchesOf(["main"]), listed())).toBe(true);
  expect(branchGone("topic", branchesOf(["main", "topic"]), listed())).toBe(false);
  expect(branchGone("topic", branchesOf(["main"], false), listed())).toBe(false);
  expect(branchGone("topic", { state: "loading" }, listed())).toBe(false);
  expect(branchGone(undefined, branchesOf([]), { state: "none" })).toBe(false);
  // The repository's first branch is in no list until it has a commit.
  expect(branchGone("topic", branchesOf([]), listed("loaded", { worktree: tree(null) }))).toBe(false);
});
