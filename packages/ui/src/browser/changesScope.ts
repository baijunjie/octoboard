// What the Changes view reads for a branch, from the repository's worktrees: pure, so the rule is
// one place and tested on its own.
import type { GitSourceInfo, WorktreeInfo } from "../protocol";
import type { BranchList } from "./useBranchList";

/**
 * Where the changes of a branch are read from. `branch` is the one chosen, or by default the one
 * checked out in the worktree holding the project's directory (`ownBranch`); none when neither
 * names one. `holder` is the worktree that has it checked out, where its uncommitted changes are,
 * and `holderId` its id for a request, none when it is the worktree holding the project's
 * directory or there is none. `committed` is what the branch has committed, as a comparison with
 * the branch the main working copy has checked out: none without a branch, or when it would be
 * compared and either branch has no commit yet (the worktree it is checked out in has no
 * `HEAD`), `noBase` when the main working copy is on no branch, and `compare`, with that branch
 * as `base`, otherwise. When the branch is that very one, it is compared with the remote-tracking
 * branch the branch list gives it instead (`baseKind` "remote", `base` its name): its upstream or
 * the branch of its name on a remote, else the repository's remote default branch. With none of
 * them, all its commits are its own, compared with nothing (`baseKind` "empty", `base` empty).
 * None while the list is not loaded, or lacks the branch, or when it has none of them and says its
 * lookup gave up.
 */
export interface ChangesScope {
  ownBranch: string | undefined;
  branch: string | undefined;
  holder: WorktreeInfo | undefined;
  holderId: string | undefined;
  committed: { state: "none" | "noBase" } | { state: "compare"; base: string; baseKind: "local" | "remote" | "empty"; branch: string };
}

export function changesScope(git: GitSourceInfo | null, chosen: string | undefined, branches: BranchList): ChangesScope {
  const ownBranch = git?.worktrees.find((w) => w.id === git.worktree)?.branch ?? undefined;
  const branch = chosen ?? ownBranch;
  const holder = branch === undefined ? undefined : git?.worktrees.find((w) => w.branch === branch);
  const holderId = holder && git && holder.id !== git.worktree ? holder.id : undefined;
  const main = git?.worktrees.find((w) => w.main);
  const baseBranch = main?.branch ?? undefined;
  const unborn = baseBranch !== undefined && (holder?.head === null || main?.head === null);
  return { ownBranch, branch, holder, holderId, committed: committedScope(branch, baseBranch, unborn, branches) };
}

function committedScope(branch: string | undefined, baseBranch: string | undefined, unborn: boolean, branches: BranchList): ChangesScope["committed"] {
  if (branch === undefined || unborn) return { state: "none" };
  if (baseBranch === undefined) return { state: "noBase" };
  if (baseBranch !== branch) return { state: "compare", base: baseBranch, baseKind: "local", branch };
  if (branches.state !== "loaded") return { state: "none" };
  // A list lacking the branch says nothing of it: it is not listed while it has no commit.
  const listed = branches.branches.find((b) => b.name === branch);
  if (!listed) return { state: "none" };
  const remote = listed.upstream ?? branches.defaultBranch;
  if (remote) return { state: "compare", base: remote.name, baseKind: "remote", branch };
  // A lookup that gave up says nothing of the branch having none.
  return branches.upstreamsComplete === false ? { state: "none" } : { state: "compare", base: "", baseKind: "empty", branch };
}
