import { useEffect } from "react";

import { gitMoment } from "./gitRequests";
import { sessionBranch } from "./sessionBranch";
import { sessionFollow } from "./sessionFollow";
import type { BranchList } from "./useBranchList";
import type { SourceState } from "./useProjectSource";

/**
 * Has the Git mode's branch follow `session`, the selected session when it belongs to `project`:
 * selecting one sets the branch, through `setBranch`, once, to the branch it most recently became
 * responsible for, and from one responsible for none, back to the default; no session leaves it as
 * it is. Only what was read from the daemon after the selection is weighed, so what a pane held from
 * before it is asked for again while `active`.
 *
 * Returns what a branch the user chooses calls: the choice settles what the session set, which is
 * then not set over it later.
 */
export function useSessionBranchFollow(
  project: string,
  session: string | undefined,
  active: boolean,
  source: { source: SourceState; refresh: () => Promise<void> },
  branches: { branches: BranchList; refresh: () => Promise<void> },
  setBranch: (branch: string | undefined) => void,
): () => void {
  const sourceState = source.source;
  const branchList = branches.branches;

  // Selecting a session sets the branch once the repository's worktrees and branches have been read
  // since, whichever state the view is in. A session with no branch yet leaves it at the default,
  // and one that shows up in a later branch list is set then, unless the user has chosen in the
  // meantime. A branch no worktree has checked out has the worktrees asked for once more: the
  // worktree may have been made after the list was read.
  useEffect(() => {
    if (session === undefined || sourceState.state !== "loaded" || branchList.state === "loading") return;
    const follow = sessionFollow(project, session);
    if (follow.settled) return;
    const { sentAt: sourceSentAt, source: info } = sourceState;
    const fresh = { source: sourceSentAt > follow.sourceSince, branches: branchList.state !== "loaded" || branchList.sentAt > follow.since };
    if (active && !follow.asked) {
      follow.asked = true;
      if (!fresh.source) void source.refresh();
      if (!fresh.branches || branchList.state === "error") void branches.refresh();
    }
    if (!fresh.source || !fresh.branches || info.git === null) return;
    const followed = sessionBranch(branchList.state === "loaded" ? branchList.branches : [], session);
    setBranch(followed);
    if (followed === undefined) return;
    if (info.git.worktrees.some((w) => w.branch === followed) || follow.retried) {
      follow.settled = true;
    } else {
      follow.retried = true;
      follow.sourceSince = gitMoment();
      void source.refresh();
    }
  }, [session, active, sourceState, branchList]);

  return () => {
    if (session !== undefined) sessionFollow(project, session).settled = true;
  };
}
