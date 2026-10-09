// What a branch list says of the branches a comparison on screen was made from: pure, so the rule
// is one place and tested on its own.
import type { ComparisonEndpoint } from "../protocol";
import type { BranchList } from "./useBranchList";

/** A compared branch that has moved to another commit since the comparison, or gone. */
export type ComparisonNotice = { kind: "moved"; branch: string } | { kind: "deleted"; branch: string; commit: string };

/**
 * What `branches` says of the branches of a comparison received at `receivedAt`: each one now at
 * another commit has moved, and each one missing from a complete list is gone. A list asked for
 * before the comparison was received says nothing about it, and a list cut short says nothing of a
 * branch it does not have. A branch compared with itself is named once.
 */
export function comparisonNotices(
  branches: BranchList,
  comparison: { left: ComparisonEndpoint; right: ComparisonEndpoint; receivedAt: number },
): ComparisonNotice[] {
  if (branches.state !== "loaded" || branches.sentAt <= comparison.receivedAt) return [];
  const notices: ComparisonNotice[] = [];
  for (const endpoint of [comparison.left, comparison.right]) {
    if (notices.some((notice) => notice.branch === endpoint.branch)) continue;
    const now = branches.branches.find((branch) => branch.name === endpoint.branch);
    if (now && now.commit !== endpoint.commit) notices.push({ kind: "moved", branch: endpoint.branch });
    else if (!now && branches.complete) notices.push({ kind: "deleted", branch: endpoint.branch, commit: endpoint.commit });
  }
  return notices;
}
