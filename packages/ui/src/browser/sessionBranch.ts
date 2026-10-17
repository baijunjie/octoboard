import type { BranchInfo, WorktreeInfo } from "../protocol";

/** What the Git mode's selectors hold: the worktree whose changes are shown (none for the one
 * holding the project's directory) and the two branches compared. */
export interface GitSelection {
  worktree: string | undefined;
  left: string | undefined;
  right: string | undefined;
}

/** What the selectors hold with no session's branch to follow. */
const DEFAULT_SELECTION: GitSelection = { worktree: undefined, left: undefined, right: undefined };

/** The branch `session` is responsible for: of those whose origin names it as responsible, the one
 * it became responsible for last. */
export function sessionBranch(branches: readonly BranchInfo[], session: string): string | undefined {
  let latest: { name: string; since: number } | undefined;
  for (const { name, origin } of branches) {
    if (origin?.responsible_session !== session) continue;
    if (!latest || origin.responsible_since > latest.since) latest = { name, since: origin.responsible_since };
  }
  return latest?.name;
}

/**
 * The selectors' values that follow `branch`, a session's branch: the worktree that has it checked
 * out (the default one when none has, as for no branch at all), and a comparison of what it has
 * changed since it forked from what the repository's main working copy has checked out: From is
 * that branch and To `branch`. A main working copy on no branch, or on `branch` itself, leaves
 * From empty: there is nothing to compare it from.
 */
export function selectionForBranch(branch: string | undefined, worktrees: readonly WorktreeInfo[], own: string): GitSelection {
  if (branch === undefined) return DEFAULT_SELECTION;
  const holder = worktrees.find((w) => w.branch === branch);
  const mainBranch = worktrees.find((w) => w.main)?.branch ?? null;
  return {
    worktree: holder && holder.id !== own ? holder.id : undefined,
    left: mainBranch !== null && mainBranch !== branch ? mainBranch : undefined,
    right: branch,
  };
}
