// What a branch list says of the branches a comparison on screen was made from: pure, so the rule
// is one place and tested on its own.
import type { ComparisonEndpoint } from "../protocol";
import type { BranchList } from "./useBranchList";

/**
 * What a branch list read since a comparison says of its branches: `moved` when one of them is
 * now at another commit, so the comparison is to be made again at the commits they are at now, and
 * `gone`, the ones a complete list no longer has; `tips` are the commits the list has the two
 * branches at, left's first, an empty string for one it does not have. A remote-tracking branch is
 * the upstream of a listed branch, or the list's default branch. A comparison with a branch gone is not made again — it would
 * fail for the missing one — and keeps the commits it was made at, so it is never `moved` while
 * one is gone.
 */
export interface ComparisonNews {
  moved: boolean;
  gone: string[];
  tips: string[];
}

/** The commit the list has branch `name` at, or remote-tracking branch `name` when `remote`
 * (the upstream of a listed branch, or the list's default branch); none when it does not have it. */
export function branchTip(list: Extract<BranchList, { state: "loaded" }>, name: string, remote: boolean): string | undefined {
  if (!remote) return list.branches.find((branch) => branch.name === name)?.commit;
  if (list.defaultBranch?.name === name) return list.defaultBranch.commit;
  return list.branches.find((branch) => branch.upstream?.name === name)?.upstream?.commit;
}

/**
 * What `branches` says of the branches of a comparison received at `receivedAt`. A list asked for
 * before the comparison was received says nothing about it, and a list cut short says nothing of a
 * branch it does not have. A branch compared with itself is named once.
 */
export function comparisonNews(
  branches: BranchList,
  comparison: { left: ComparisonEndpoint; right: ComparisonEndpoint; receivedAt: number },
): ComparisonNews {
  if (branches.state !== "loaded" || branches.sentAt <= comparison.receivedAt) return { moved: false, gone: [], tips: [] };
  let moved = false;
  const gone: string[] = [];
  const tips: string[] = [];
  for (const endpoint of [comparison.left, comparison.right]) {
    // The empty tree is no branch, and does not move.
    if (endpoint.empty) continue;
    const now = branchTip(branches, endpoint.branch, endpoint.remote === true);
    tips.push(now ?? "");
    if (now !== undefined && now !== endpoint.commit) moved = true;
    // A remote-tracking branch no branch has as its upstream any more is not gone: the list then
    // gives the branch another base, and this comparison is not the one on screen.
    else if (now === undefined && !endpoint.remote && branches.complete && !gone.includes(endpoint.branch)) gone.push(endpoint.branch);
  }
  return { moved: moved && gone.length === 0, gone, tips };
}
