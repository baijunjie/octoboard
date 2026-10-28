import { useEffect, useState } from "react";

import { DaemonRequestError } from "../daemon-client";
import { errorPresentation, type ErrorPresentation } from "../errorPresentation";
import { useT } from "../i18n/react";
import type { SessionBranchItem, SessionBranchTarget } from "../protocol";
import { useDaemon, type Daemon } from "../store";

/** The git branches and worktrees bound to `sessionIds`, as the daemon finds them now. A failed
 * inspection reads as none: nothing is then forced, and the daemon still removes what can go. */
export async function inspectSessionBranches(request: Daemon["request"], sessionIds: string[]): Promise<SessionBranchItem[]> {
  try {
    const result = await request({ type: "inspect_session_branches", sessions: sessionIds });
    return result.type === "session_branches_inspected" ? result.items : [];
  } catch {
    return [];
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

/** Inspects the bound branches and worktrees of the whole archive or deletion scope for its
 * confirmation. `initial` is an answer already in hand for this scope, which is not asked again.
 * When the scope changes, the last answer stands until the new one arrives, and an answer that
 * arrives after a newer question is dropped. */
export function useSessionBranches(sessionIds: string[], initial?: SessionBranchItem[]): Inspected {
  const { request } = useDaemon();
  const key = JSON.stringify([...sessionIds].sort());
  const [latest, setLatest] = useState<{ key: string; items: SessionBranchItem[] } | undefined>(() => initial && { key, items: initial });
  const known = latest?.key === key;
  useEffect(() => {
    if (known) return;
    let active = true;
    void inspectSessionBranches(request, JSON.parse(key) as string[]).then((items) => {
      if (active) setLatest({ key, items });
    });
    return () => { active = false; };
  }, [key, known, request]);
  return { items: latest?.items ?? [], pending: !known };
}

/** The refusals of a project's removal with its folder, which its inspection answers with too: the removal confirmed
 * after it says why it is refused itself, and takes nothing along. A refusal decided by a path that could not be
 * resolved, such as a folder that did not respond in time, carries an `inspection` token, which the removal sends back
 * to have that refusal repeated even once the path resolves (`delete_project` in `apps/daemon/PROTOCOL.md`). */
const REMOVAL_REFUSALS = new Set(["project_directory_protected", "project_directory_shared", "project_directory_trash_failed"]);

/** Whether `err` is a refusal of a project's removal with its folder (`REMOVAL_REFUSALS`). */
export function isRemovalRefusal(err: unknown): err is DaemonRequestError {
  return err instanceof DaemonRequestError && REMOVAL_REFUSALS.has(err.code);
}

/** What removing project `projectId` with its folder moved to the Trash would also remove, asked
 * afresh each time `asking` turns on (the Trash option ticked) and each time `reinspect` is called;
 * `failed` when the daemon could not say, in which case anything may go. A removal that would be
 * refused lists nothing and has not failed; `inspection` is the token its refusal carried, for the
 * removal to send back. */
export function useProjectRemovalBranches(projectId: string, asking: boolean): Inspected & { failed: boolean; inspection: string | undefined; reinspect: () => void } {
  const { request } = useDaemon();
  const [round, setRound] = useState(0);
  const [latest, setLatest] = useState<{ round: number; items: SessionBranchItem[]; failed: boolean; inspection?: string }>();
  // Each turn of `asking` on is a question of its own, so what changed while the option was off is
  // found.
  const [asked, setAsked] = useState(false);
  if (asking !== asked) {
    setAsked(asking);
    if (asking) setRound(round + 1);
  }
  useEffect(() => {
    if (!asking) return;
    let active = true;
    void (async () => {
      let items: SessionBranchItem[] = [];
      let failed = false;
      let inspection: string | undefined;
      try {
        const result = await request({ type: "inspect_project_removal", project: projectId });
        if (result.type === "project_removal_inspected") items = result.items;
        else failed = true;
      } catch (err) {
        if (isRemovalRefusal(err)) inspection = err.params.inspection || undefined;
        else failed = true;
      }
      if (active) setLatest({ round, items, failed, inspection });
    })();
    return () => { active = false; };
  }, [projectId, request, asking, round]);
  const current = asking && latest?.round === round ? latest : undefined;
  return {
    items: latest?.items ?? [],
    pending: asking && !current,
    failed: !!latest?.failed,
    inspection: current?.inspection,
    reinspect: () => { if (asking) setRound((value) => value + 1); },
  };
}

/** The branches `sessionId` is responsible for, as the daemon finds them now, asked once while
 * `enabled`. Unlike `useSessionBranches` it says when the daemon could not tell (`error`, what went
 * wrong), since a handoff's summary must not read a failure as "none", and `retry` asks again. The
 * last failure stays in `error` while a retry is out (`pending`), so what shows it, and the control
 * that retried, are not taken away under focus. */
export function useResponsibleBranches(sessionId: string, enabled: boolean): { items: SessionBranchItem[]; pending: boolean; error: ErrorPresentation | undefined; retry: () => void } {
  const t = useT();
  const { request } = useDaemon();
  const [attempt, setAttempt] = useState(0);
  const [answer, setAnswer] = useState<{ attempt: number; items: SessionBranchItem[]; error?: ErrorPresentation }>();
  useEffect(() => {
    if (!enabled) return;
    let active = true;
    void request({ type: "inspect_session_branches", sessions: [sessionId] }).then(
      (result) => active && setAnswer(result.type === "session_branches_inspected" ? { attempt, items: result.items } : { attempt, items: [], error: errorPresentation(new Error(t("error.unexpectedReply"))) }),
      (err: unknown) => active && setAnswer({ attempt, items: [], error: errorPresentation(err) }),
    );
    return () => { active = false; };
  }, [sessionId, enabled, request, attempt]);
  return { items: answer?.items ?? [], pending: enabled && answer?.attempt !== attempt, error: answer?.error, retry: () => setAttempt((n) => n + 1) };
}
