// What the Changes view says around its list, from the state of its parts: pure, so what is said
// when is one place and tested on its own, and the component is left to lay it out.
import type { ErrorPresentation } from "../errorPresentation";
import type { PlainMessageKey } from "../i18n/catalog";
import { abbreviateHome } from "../pathDisplay";
import { displayWirePath } from "../wirePath";
import { comparisonNews } from "./comparisonNews";
import { shortCommit } from "./shortCommit";
import type { BranchList } from "./useBranchList";
import type { CommittedState, UncommittedState } from "./useGitReview";

/** A line above the list, as the words it is made of; paths and branches are in display form. */
export type ChangesNotice =
  | { id: "notCheckedOut"; branch: string }
  | { id: "unborn" }
  | { id: "worktree"; path: string }
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
 * checked out in a worktree with no commit yet is not in any list and is not gone. */
export function branchGone(branch: string | undefined, branches: BranchList, uncommitted: UncommittedState): boolean {
  if (branch === undefined || branches.state !== "loaded" || !branches.complete) return false;
  if (uncommitted.state === "listed" && uncommitted.worktree.head === null) return false;
  return !branches.branches.some((b) => b.name === branch);
}

export interface ChangesStatus {
  /** The lines above the list, top to bottom. */
  notices: ChangesNotice[];
  /** Every part that failed, in the order the list's parts stand. */
  failures: ChangesFailure[];
  /** The branches the comparison on screen was made with that a complete branch list no longer has. */
  goneBranches: string[];
  /** Whether what is listed stops short of everything: some part of it was cut. */
  partial: boolean;
  /** How many commits are listed when only the newest were, none when all were. */
  commitsCut: number | undefined;
}

export function changesStatus({
  branches,
  uncommitted,
  committed,
  homeDir,
}: {
  branches: BranchList;
  uncommitted: UncommittedState;
  committed: CommittedState;
  homeDir: string | null | undefined;
}): ChangesStatus {
  const notices: ChangesNotice[] = [];
  const failures: ChangesFailure[] = [];
  if (uncommitted.state === "notCheckedOut") {
    notices.push({ id: "notCheckedOut", branch: displayWirePath(uncommitted.branch) });
  } else if (uncommitted.state === "listed") {
    const { worktree, own, list } = uncommitted;
    if (worktree.head === null) notices.push({ id: "unborn" });
    if (!own) notices.push({ id: "worktree", path: abbreviateHome(displayWirePath(worktree.root), homeDir) });
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
      if (commits?.state === "loading") notices.push({ id: "listingCommits" });
      else if (commits?.state === "error") failures.push({ key: "commits", title: "git.changes.commitsFailed", line: "git.changes.commitsError", message: commits.message });
    }
  }

  const shown = committed.state === "compare" && committed.comparison.state === "loaded" ? committed.comparison : undefined;
  const commitList = committed.state === "compare" ? committed.commits : undefined;
  const partial =
    (uncommitted.state === "listed" && uncommitted.list.state === "loaded" && !uncommitted.list.complete) ||
    (commitList === undefined ? shown !== undefined && !shown.complete : commitList.state === "loaded" && !commitList.complete);
  return {
    notices,
    failures,
    goneBranches: shown ? comparisonNews(branches, shown).gone.map(displayWirePath) : [],
    partial,
    commitsCut: commitList?.state === "loaded" && !commitList.commitsComplete ? commitList.count : undefined,
  };
}
