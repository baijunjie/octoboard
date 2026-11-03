import type { WorktreeInfo } from "../../protocol";
import type { Comparison } from "./useBranchComparison";
import type { ChangeList } from "./useChangeList";
import type { CommitList } from "./useCommitList";

/** Where the uncommitted part of the list stands: no branch to read it for, a branch no worktree
 * has checked out, or the list read from the worktree that has (`own` when that is the worktree
 * holding the project's directory). */
export type UncommittedState =
  | { state: "none" }
  | { state: "notCheckedOut"; branch: string }
  | { state: "listed"; list: ChangeList; worktree: WorktreeInfo; own: boolean };

/** Where the committed part of the list stands: no branch, or one of the two with no commit yet, a main working copy on no branch to
 * compare with (`noBase`), or the comparison with `base`, the main working copy's branch or, for
 * that branch itself, a remote-tracking branch (`baseKind`) or none, all its commits (an empty
 * `base`), and — grouped by commit — its commits. */
export type CommittedState =
  | { state: "none" | "noBase" }
  | {
    state: "compare";
    branch: string;
    base: string;
    baseKind: "local" | "remote" | "empty";
    comparison: Comparison;
    commits: CommitList | undefined;
  };
