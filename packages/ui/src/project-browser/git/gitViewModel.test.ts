import { expect, it } from "vitest";

import type { WorktreeInfo } from "../../protocol";
import { committedState, uncommittedState } from "./gitViewModel";
import type { Comparison } from "./useBranchComparison";
import type { ChangeList } from "./useChangeList";
import type { CommitList } from "./useCommitList";

const worktree: WorktreeInfo = { id: "w", root: "/w", main: false, head: "c1", branch: "topic", scope_present: true };
const list: ChangeList = { state: "loaded", head: "c1", items: [], complete: true, refreshing: false };

it("says where the uncommitted part stands for the branch and its worktree", () => {
  expect(uncommittedState(undefined, undefined, undefined, list, false)).toEqual({ state: "none" });
  expect(uncommittedState("topic", undefined, undefined, list, false)).toEqual({ state: "notCheckedOut", branch: "topic" });
  expect(uncommittedState("topic", worktree, "w", list, false)).toEqual({ state: "listed", list, worktree, own: false });
  expect(uncommittedState("main", worktree, undefined, list, false)).toEqual({ state: "listed", list, worktree, own: true });
});

it("shows the list of a worktree found gone as still read", () => {
  const gone: ChangeList = { state: "error", message: "gone", unavailable: true, refreshing: false };
  const state = uncommittedState("topic", worktree, "w", gone, true);
  expect(state.state === "listed" && state.list).toEqual({ state: "loading" });
  expect(uncommittedState("topic", worktree, "w", gone, true)).toEqual(state);
});

it("passes a scope with nothing to compare through as it is", () => {
  const scope = { state: "noBase" } as const;
  expect(committedState(scope, { state: "idle" }, { state: "idle" }, true, true)).toBe(scope);
});

it("gives the commits only while they are grouped and the comparison has some", () => {
  const scope = { state: "compare", branch: "topic", base: "main", baseKind: "local" } as const;
  const comparison: Comparison = { state: "loading" };
  const commits: CommitList = { state: "loading" };
  expect(committedState(scope, comparison, commits, true, true)).toEqual({ ...scope, comparison, commits });
  expect(committedState(scope, comparison, commits, false, true)).toEqual({ ...scope, comparison, commits: undefined });
  expect(committedState(scope, comparison, commits, true, false)).toEqual({ ...scope, comparison, commits: undefined });
});
