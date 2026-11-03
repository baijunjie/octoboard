import { CodedError } from "../errors/codedError";
import type { ProcessTree, SessionBranchItem, SessionBranchTarget } from "../protocol";
import type { Daemon } from "../store";

/** What an archive or deletion would find in its scope: the bound branches and worktrees, and, when
 * the inspection asked for them, the process trees an archive would stop (`ProcessTree`). `sessions`
 * records the inspected scope so an answer is not reused for a different archive scope; `failed`
 * distinguishes an unavailable inspection from a successful empty list. */
export type SessionInspection = { sessions: string[]; items: SessionBranchItem[]; processes: ProcessTree[]; failed?: true };

/** The git branches and worktrees bound to `sessionIds`, as the daemon finds them now, and with
 * `withProcesses` the process trees that have run long enough for an archive to ask about them. A
 * failed inspection grants no extra branch-removal permission. A caller with a previous process list
 * can keep its choices for processes that still belong to the inspected scope. */
export async function inspectSessionBranches(request: Daemon["request"], sessionIds: string[], withProcesses = false): Promise<SessionInspection> {
  try {
    const result = await request({ type: "inspect_session_branches", sessions: sessionIds, ...(withProcesses ? { processes: true } : {}) });
    return { sessions: sessionIds, items: result.items, processes: result.processes ?? [] };
  } catch {
    return { sessions: sessionIds, items: [], processes: [], failed: true };
  }
}

/** The bound items git refuses to remove — a worktree with uncommitted changes or submodules, an
 * unmerged branch — which an archive or deletion leaves unless the user forces their removal. The
 * other items that cannot go safely stay whatever is asked, so they are not offered. */
export function forcibleBranches(items: SessionBranchItem[]): SessionBranchItem[] {
  return items.filter((item) => item.state === "uncommitted_changes" || item.state === "submodules" || item.state === "unmerged");
}

/** The locked worktrees among `items`, which go only when the user chooses so, apart from the
 * other items git refuses to remove. */
export function lockedWorktrees(items: SessionBranchItem[]): SessionBranchItem[] {
  return items.filter((item) => item.kind === "worktree" && item.state === "locked");
}

/** `items` as a request names them back: as each was shown, so that the daemon goes no further
 * than that. */
export function branchTargets(items: SessionBranchItem[]): SessionBranchTarget[] {
  return items.map(({ kind, repository, branch, path, state, uncommitted }) => ({ kind, repository, branch, path, state, uncommitted }));
}

/** The `force_remove` and `keep` of a request that forces `forced` and keeps `kept`, each absent
 * when empty, so a request that names nothing reads as before. */
export function branchChoices(forced: SessionBranchItem[], kept: SessionBranchItem[] = []): { force_remove?: SessionBranchTarget[]; keep?: SessionBranchTarget[] } {
  return {
    ...(forced.length > 0 ? { force_remove: branchTargets(forced) } : {}),
    ...(kept.length > 0 ? { keep: branchTargets(kept) } : {}),
  };
}

/** An inspection's answer: what it found, the last answer while a newer one is on its way, and
 * whether one is. */
export type Inspected = { items: SessionBranchItem[]; pending: boolean };

/** The refusals of a project's removal with its folder, which its inspection answers with too: the removal confirmed
 * after it says why it is refused itself, and takes nothing along. A refusal decided by a path that could not be
 * resolved, such as a folder that did not respond in time, carries an `inspection` token, which the removal sends back
 * to have that refusal repeated even once the path resolves (`delete_project` in `apps/daemon/PROTOCOL.md`). */
const REMOVAL_REFUSALS = new Set(["project_directory_protected", "project_directory_shared", "project_directory_trash_failed"]);

/** Whether `err` is a refusal of a project's removal with its folder (`REMOVAL_REFUSALS`). */
export function isRemovalRefusal(err: unknown): err is CodedError {
  return err instanceof CodedError && REMOVAL_REFUSALS.has(err.code);
}
