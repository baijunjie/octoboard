// What the Changes view reads for a branch, from the repository's worktrees: pure, so the rule is
// one place and tested on its own.
import type { GitSourceInfo, WorktreeInfo } from "../protocol";

/**
 * Where the changes of a branch are read from. `branch` is the one chosen, or by default the one
 * checked out in the worktree holding the project's directory (`ownBranch`); none when neither
 * names one. `holder` is the worktree that has it checked out, where its uncommitted changes are,
 * and `holderId` its id for a request, none when it is the worktree holding the project's
 * directory or there is none. `committed` is what the branch has committed, as a comparison with
 * the branch the main working copy has checked out: none without a branch, or when it would be
 * compared and either branch has no commit yet (the worktree it is checked out in has no
 * `HEAD`), `noBase` when the main
 * working copy is on no branch, `isBase` when the branch is that very one, and `compare`, with
 * that branch as `base`, otherwise.
 */
export interface ChangesScope {
  ownBranch: string | undefined;
  branch: string | undefined;
  holder: WorktreeInfo | undefined;
  holderId: string | undefined;
  committed: { state: "none" | "noBase" } | { state: "isBase"; branch: string } | { state: "compare"; base: string; branch: string };
}

export function changesScope(git: GitSourceInfo | null, chosen: string | undefined): ChangesScope {
  const ownBranch = git?.worktrees.find((w) => w.id === git.worktree)?.branch ?? undefined;
  const branch = chosen ?? ownBranch;
  const holder = branch === undefined ? undefined : git?.worktrees.find((w) => w.branch === branch);
  const holderId = holder && git && holder.id !== git.worktree ? holder.id : undefined;
  const main = git?.worktrees.find((w) => w.main);
  const baseBranch = main?.branch ?? undefined;
  const unborn = baseBranch !== undefined && baseBranch !== branch && (holder?.head === null || main?.head === null);
  const committed: ChangesScope["committed"] =
    branch === undefined || unborn ? { state: "none" } : baseBranch === undefined ? { state: "noBase" } : baseBranch === branch ? { state: "isBase", branch } : { state: "compare", base: baseBranch, branch };
  return { ownBranch, branch, holder, holderId, committed };
}
