import { useEffect, useRef } from "react";

import type { ChangeItem } from "./changes";
import type { ChangesScope } from "./changesScope";
import { branchTip, comparisonNews } from "./comparisonNews";
import { useBranchComparison, type Comparison } from "./useBranchComparison";
import type { BranchList } from "./useBranchList";
import { useCommitList, type CommitList } from "./useCommitList";

/** What a list with nothing in it is, so that one is no new array on every render. */
const NO_ITEMS: ChangeItem[] = [];

/** The comparison as it is on screen. */
export type ShownComparison = Extract<Comparison, { state: "loaded" }>;

/** The two commits the commits of a comparison run between, and the branches it compares. */
export interface ForkedRange {
  branch: string;
  baseBranch: string;
  base: string;
  tip: string;
}

export interface CommittedChanges {
  comparison: Comparison;
  /** The comparison while one is on screen. */
  shown: ShownComparison | undefined;
  /** The range of the comparison's commits, when it has any. */
  forked: ForkedRange | undefined;
  commits: CommitList;
  /** The commits, once they are listed for the very range on screen. */
  listedCommits: Extract<CommitList, { state: "loaded" }> | undefined;
  /** The committed part of the list, in list order: the comparison's changes, or, grouped by
   * commit, the commits' changes; none while what it is made of is still read. */
  items: ChangeItem[] | undefined;
  /** Asks again for the comparison and the commits; settles once both have been answered or have
   * failed. */
  refresh: () => Promise<void>;
}

/**
 * What the branch of `committed` has committed: its comparison with its base and, while `byCommit`,
 * the commits since the fork, each with its own changes. The comparison is asked for while `active`
 * once the repository is known to be one (`readable`); the commits while `active` and `byCommit`.
 *
 * The comparison is of the commits the branches were at when it was made; once a branch list read
 * since (`branchList`) shows either branch at another commit, it is made again, so the Committed
 * section follows the branch as it commits what was uncommitted.
 */
export function useCommittedChanges(
  project: string,
  committed: ChangesScope["committed"],
  branchList: BranchList,
  byCommit: boolean,
  active: boolean,
  readable: boolean,
): CommittedChanges {
  const comparing = committed.state === "compare";
  const comparisonHook = useBranchComparison(project, comparing ? committed.base : undefined, comparing ? committed.baseKind : "local", comparing ? committed.branch : undefined, active && readable);
  const comparison = comparisonHook.comparison;
  const shown = comparison.state === "loaded" ? comparison : undefined;
  const forked = shown && shown.base !== shown.right.commit ? { branch: shown.right.branch, baseBranch: shown.left.branch, base: shown.base, tip: shown.right.commit } : undefined;
  const commitsHook = useCommitList(project, forked, active && byCommit);
  const commits = commitsHook.commits;

  // A branch list read since the comparison that shows either branch at another commit makes it
  // again: what the branch has committed since moves from its uncommitted sections to its committed
  // ones. It is asked once for the commits the branches are at, so a comparison that takes a while
  // is not restarted by every list read meanwhile; branches at other commits are asked for again.
  // One that failed has nothing on screen to read the news from, so it is asked for again only
  // when a later list has the branches at commits other than the last ones seen or asked for in
  // the background.
  const askedFor = useRef<{ pair: string; tips: string } | undefined>(undefined);
  useEffect(() => {
    if (!active) return;
    let pair: string;
    let tips: string[];
    if (shown) {
      const news = comparisonNews(branchList, shown);
      pair = JSON.stringify([shown.left.branch, shown.left.remote === true, shown.right.branch]);
      if (!news.moved) {
        if (news.tips.length > 0) askedFor.current = { pair, tips: JSON.stringify(news.tips) };
        return;
      }
      tips = news.tips;
    } else if (committed.state === "compare" && comparison.state === "error" && branchList.state === "loaded") {
      pair = JSON.stringify([committed.base, committed.baseKind === "remote", committed.branch]);
      if (askedFor.current?.pair !== pair) return;
      tips = [branchTip(branchList, committed.branch, false) ?? ""];
      if (committed.baseKind !== "empty") tips.unshift(branchTip(branchList, committed.base, committed.baseKind === "remote") ?? "");
      if (tips.includes("")) return;
    } else {
      return;
    }
    const asked = { pair, tips: JSON.stringify(tips) };
    if (askedFor.current?.pair === pair && askedFor.current.tips === asked.tips) return;
    askedFor.current = asked;
    void comparisonHook.update();
  }, [active, branchList, shown, comparison]);

  const items = !shown ? undefined : !byCommit ? shown.items : commits.state === "loaded" ? commits.items : forked ? undefined : NO_ITEMS;
  const listedCommits = commits.state === "loaded" && forked && commits.base === forked.base && commits.tip === forked.tip ? commits : undefined;
  const refresh = async () => {
    await Promise.all([comparisonHook.refresh(), commitsHook.refresh()]);
  };
  return { comparison, shown, forked, commits, listedCommits, items, refresh };
}
