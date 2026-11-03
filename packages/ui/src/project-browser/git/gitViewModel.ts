// What the Git mode's view is told of each part of the list, from what its hooks hold. Pure, so the
// compiled caller keeps each result for as long as what it is made of stays the same.
import type { WorktreeInfo } from "../../protocol";
import type { ChangesScope } from "./changesScope";
import type { CommittedState, UncommittedState } from "./gitReviewState";
import type { Comparison } from "./useBranchComparison";
import type { ChangeList } from "./useChangeList";
import type { CommitList } from "./useCommitList";

/** The uncommitted list while it is read, so that one is no new object on every call. */
const LIST_LOADING: ChangeList = { state: "loading" };

/** Where the uncommitted part stands for `branch`, checked out in `holder` (by id `holderId`; none
 * for the worktree holding the project's directory). A worktree found gone is no failure of the
 * list: the worktrees are being read again, and the list then follows the branch to wherever it is
 * checked out, so until then it is still read. */
export function uncommittedState(
  branch: string | undefined,
  holder: WorktreeInfo | undefined,
  holderId: string | undefined,
  list: ChangeList,
  worktreeGone: boolean,
): UncommittedState {
  if (branch === undefined) return { state: "none" };
  if (holder === undefined) return { state: "notCheckedOut", branch };
  return { state: "listed", list: worktreeGone ? LIST_LOADING : list, worktree: holder, own: holderId === undefined };
}

/** Where the committed part stands for the `committed` scope: its comparison and, grouped by commit
 * while the comparison has commits (`forked`), its commits. */
export function committedState(committed: ChangesScope["committed"], comparison: Comparison, commits: CommitList, byCommit: boolean, forked: boolean): CommittedState {
  if (committed.state !== "compare") return committed;
  return { state: "compare", branch: committed.branch, base: committed.base, baseKind: committed.baseKind, comparison, commits: byCommit && forked ? commits : undefined };
}
