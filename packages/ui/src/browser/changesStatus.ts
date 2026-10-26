// What the Changes view says around its list, from the state of its parts: pure, so what is said
// when is one place and tested on its own, and the component is left to lay it out.
import type { ErrorPresentation } from "../errorPresentation";
import type { PlainMessageKey } from "../i18n/catalog";
import type { WorktreeInfo } from "../protocol";
import { displayWirePath, wireBaseName } from "../wirePath";
import { comparisonNews } from "./comparisonNews";
import { shortCommit } from "./shortCommit";
import type { BranchList } from "./useBranchList";
import type { CommittedState, UncommittedState } from "./useGitReview";

/** A line above the list, as the words it is made of; paths and branches are in display form, except
 * the worktree's `root`, a wire path, which the line's copy button turns into display form itself. `cut` is
 * whether its uncommitted changes were cut short, which the line ends on a mark for. */
export type ChangesNotice =
  | { id: "unborn" }
  | { id: "worktree"; name: string; root: string; cut?: true }
  | { id: "scopeMissing" }
  | { id: "noRemote"; branch: string }
  | { id: "noBase" }
  | { id: "comparing"; base: string }
  | { id: "base"; base: string; baseCommit: string; tip: string }
  | { id: "listingCommits" };

/** A part of the list that could not be read: the panel's title when the list has nothing else to
 * show, and the line it says it in otherwise. */
export interface ChangesFailure {
  key: string;
  title: PlainMessageKey;
  line: "git.changes.listError" | "git.changes.compareError" | "git.changes.commitsError";
  message: ErrorPresentation;
}

/** Whether the list's uncommitted part, and its committed part, are still being read. */
export function partsLoading({ uncommitted, committed }: { uncommitted: UncommittedState; committed: CommittedState }): { uncommitted: boolean; committed: boolean } {
  return {
    uncommitted: uncommitted.state === "listed" && uncommitted.list.state === "loading",
    committed: committed.state === "compare" && (committed.comparison.state === "loading" || committed.commits?.state === "loading"),
  };
}

/** Whether `branch` is one the repository no longer has: a complete branch list lacks it. A branch
 * checked out in a worktree with no commit yet (`holder`, the worktree that has it checked out) is
 * not in any list and is not gone. */
export function branchGone(branch: string | undefined, branches: BranchList, holder: WorktreeInfo | undefined): boolean {
  if (branch === undefined || branches.state !== "loaded" || !branches.complete) return false;
  if (holder?.head === null) return false;
  return !branches.branches.some((b) => b.name === branch);
}

export interface ChangesStatus {
  /** The worktree shown under the branch selector, followed by the lines above the list. */
  notices: ChangesNotice[];
  /** Every part that failed, in the order the list's parts stand. */
  failures: ChangesFailure[];
  /** The branches the comparison on screen was made with that a complete branch list no longer has. */
  goneBranches: string[];
  /** Whether what is listed stops short of everything: some part of it was cut. */
  partial: boolean;
  /** Whether the committed changes (the comparison's, or the commits') were cut. */
  committedPartial: boolean;
  /** How many commits are listed when only the newest were, none when all were. */
  commitsCut: number | undefined;
}

export function changesStatus({
  branches,
  uncommitted,
  committed,
}: {
  branches: BranchList;
  uncommitted: UncommittedState;
  committed: CommittedState;
}): ChangesStatus {
  const notices: ChangesNotice[] = [];
  const failures: ChangesFailure[] = [];
  let listingCommits = false;
  const partialUncommitted = uncommitted.state === "listed" && uncommitted.list.state === "loaded" && !uncommitted.list.complete;
  if (uncommitted.state === "listed") {
    const { worktree, own, list } = uncommitted;
    // The worktree is named when it is not the project's own; the project's own is named too when its changes were
    // cut short, since the warning is about it and its line is where the warning's mark goes.
    if (!own || partialUncommitted) notices.push({ id: "worktree", name: wireBaseName(worktree.root), root: worktree.root, ...(partialUncommitted && { cut: true as const }) });
    if (worktree.head === null) notices.push({ id: "unborn" });
    if (!worktree.scope_present) notices.push({ id: "scopeMissing" });
    if (list.state === "error") failures.push({ key: "uncommitted", title: "git.error.list", line: "git.changes.listError", message: list.message });
  }
  if (committed.state === "noBase") {
    notices.push({ id: "noBase" });
  } else if (committed.state === "compare") {
    const { comparison, commits, base, baseKind, branch } = committed;
    // Compared with nothing there is no fork to name: the line says so from the start, whatever
    // the comparison is doing.
    if (baseKind === "empty") notices.push({ id: "noRemote", branch: displayWirePath(branch) });
    if (comparison.state === "loading") {
      if (baseKind !== "empty") notices.push({ id: "comparing", base: displayWirePath(base) });
    } else if (comparison.state === "error") {
      failures.push({ key: "committed", title: "git.changes.compareFailed", line: "git.changes.compareError", message: comparison.message });
    } else if (comparison.state === "loaded") {
      if (baseKind !== "empty") {
        notices.push({
          id: "base",
          base: displayWirePath(comparison.left.branch),
          baseCommit: shortCommit(comparison.base),
          tip: shortCommit(comparison.right.commit),
        });
      }
      if (commits?.state === "loading") listingCommits = true;
      else if (commits?.state === "error") failures.push({ key: "commits", title: "git.changes.commitsFailed", line: "git.changes.commitsError", message: commits.message });
    }
  }

  if (listingCommits) notices.push({ id: "listingCommits" });

  const shown = committed.state === "compare" && committed.comparison.state === "loaded" ? committed.comparison : undefined;
  const commitList = committed.state === "compare" ? committed.commits : undefined;
  const committedPartial = commitList === undefined ? shown !== undefined && !shown.complete : commitList.state === "loaded" && !commitList.complete;
  return {
    notices,
    failures,
    goneBranches: shown ? comparisonNews(branches, shown).gone.map(displayWirePath) : [],
    partial: partialUncommitted || committedPartial,
    committedPartial,
    commitsCut: commitList?.state === "loaded" && !commitList.commitsComplete ? commitList.count : undefined,
  };
}
