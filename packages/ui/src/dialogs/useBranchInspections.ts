import { useEffect, useState } from "react";

import { useLatest } from "../components/hooks/useLatest";
import { useLatestRequest } from "../components/hooks/useLatestRequest";
import { DaemonRequestError } from "../daemon-client";
import { errorPresentation, type ErrorPresentation } from "../errors/errorPresentation";
import { useT } from "../i18n/react";
import type { ProcessTree, SessionBranchItem } from "../protocol";
import { useDaemon } from "../store";
import { shownError } from "./shownError";
import { inspectSessionBranches, isRemovalRefusal, type Inspected, type SessionInspection } from "./branchInspection";

/** The inspection last answered, for the sessions `key` names (the sorted IDs); none before the first answer. */
type Answered = { key: string | undefined; items: SessionBranchItem[]; processes: ProcessTree[] };

const UNANSWERED: Answered = { key: undefined, items: [], processes: [] };

/** Inspects the bound branches and worktrees of the whole archive or deletion scope for its
 * confirmation, and with `withProcesses` (an archive's) the process trees it would stop. `initial` is
 * an answer already in hand; only an exact match of its inspected sessions is reused, and never a failed one,
 * which says nothing of what the scope holds.
 * When the scope changes, the last answer stands until the new one arrives, and an answer that
 * arrives after a newer question is dropped. A failure clears branch-removal choices but retains
 * previously listed processes that still belong to the scope, so keep choices are not lost. */
function useInspection(sessionIds: string[], initial: SessionInspection | undefined, withProcesses: boolean): Inspected & Pick<SessionInspection, "processes"> {
  const { request } = useDaemon();
  const key = JSON.stringify([...sessionIds].sort());
  const { value, send } = useLatestRequest<Answered>({
    scope: undefined,
    initial: initial && !initial.failed ? { key: JSON.stringify([...initial.sessions].sort()), ...initial } : UNANSWERED,
  });
  const known = value.key === key;
  const latest = useLatest({ request, send });
  useEffect(() => {
    if (known) return;
    const { request, send } = latest.current;
    void send({
      request: () => inspectSessionBranches(request, JSON.parse(key) as string[], withProcesses),
      accept: (inspection) => (held) => ({
        key,
        items: inspection.items,
        processes: inspection.failed
          ? held.processes.filter((tree) => tree.session_id !== undefined && inspection.sessions.includes(tree.session_id))
          : inspection.processes,
      }),
      // `inspectSessionBranches` answers a failure as a failed inspection, so nothing is rejected.
      failed: (_failure, held) => held,
    });
  }, [key, known]);
  return { items: value.items, processes: value.processes, pending: !known };
}

/** What an archive of `sessionIds` would find (`useInspection`): the bound branches and worktrees,
 * and the process trees it would stop. `initial` is an inspection already made for this archive. */
export function useArchiveInspection(sessionIds: string[], initial?: SessionInspection): Inspected & Pick<SessionInspection, "processes"> {
  return useInspection(sessionIds, initial, true);
}

/** What a deletion of `sessionIds` would find (`useInspection`): the bound branches and worktrees. */
export function useSessionBranches(sessionIds: string[]): Inspected {
  const { items, pending } = useInspection(sessionIds, undefined, false);
  return { items, pending };
}

/** What a project's removal inspection last answered, in the `round` it was asked. */
type RemovalAnswer = { round: number; items: SessionBranchItem[]; failed: boolean; inspection?: string };

/** What removing project `projectId` with its folder moved to the Trash would also remove, asked
 * afresh each time `asking` turns on (the Trash option ticked) and each time `reinspect` is called;
 * `failed` when the daemon could not say, in which case anything may go. A removal that would be
 * refused lists nothing and has not failed; `inspection` is the token its refusal carried, or its
 * answer where it held worktrees back, for the removal to send back. */
export function useProjectRemovalBranches(projectId: string, asking: boolean): Inspected & { failed: boolean; inspection: string | undefined; reinspect: () => void } {
  const { request } = useDaemon();
  const [round, setRound] = useState(0);
  const { value, send } = useLatestRequest<RemovalAnswer | undefined>({ scope: undefined, initial: undefined });
  const latest = useLatest({ request, send, asking });
  // Each turn of `asking` on is a question of its own, so what changed while the option was off is
  // found.
  const [asked, setAsked] = useState(false);
  if (asking !== asked) {
    setAsked(asking);
    if (asking) setRound(round + 1);
  }
  useEffect(() => {
    if (!asking) return;
    const { request, send } = latest.current;
    const inspectionFailed: RemovalAnswer = { round, items: [], failed: true };
    void send({
      request: async () => {
        try {
          return await request({ type: "inspect_project_removal", project: projectId });
        } catch (err) {
          if (isRemovalRefusal(err)) return err;
          throw err;
        }
      },
      accept: (reply) => () => {
        if (reply instanceof DaemonRequestError) return { round, items: [], failed: false, inspection: reply.params.inspection || undefined };
        if (reply.type !== "project_removal_inspected") return inspectionFailed;
        return { round, items: reply.items, failed: false, inspection: reply.inspection || undefined };
      },
      failed: () => inspectionFailed,
      disconnected: () => inspectionFailed,
      wanted: () => latest.current.asking,
    });
  }, [projectId, asking, round]);
  const current = asking && value?.round === round ? value : undefined;
  return {
    items: value?.items ?? [],
    pending: asking && !current,
    failed: !!value?.failed,
    inspection: current?.inspection,
    reinspect: () => { if (asking) setRound((n) => n + 1); },
  };
}

/** What the handoff's inspection last answered, in the `attempt` it was asked. */
type ResponsibleAnswer = { attempt: number; items: SessionBranchItem[]; error?: ErrorPresentation };

/** The branches `sessionId` is responsible for, as the daemon finds them now, asked once while
 * `enabled`. Unlike `useSessionBranches` it says when the daemon could not tell (`error`, what went
 * wrong), since a handoff's summary must not read a failure as "none", and `retry` asks again. The
 * last failure stays in `error` while a retry is out (`pending`), so what shows it, and the control
 * that retried, are not taken away under focus. */
export function useResponsibleBranches(sessionId: string, enabled: boolean): { items: SessionBranchItem[]; pending: boolean; error: ErrorPresentation | undefined; retry: () => void } {
  const t = useT();
  const { request } = useDaemon();
  const [attempt, setAttempt] = useState(0);
  const { value, send, reconnects } = useLatestRequest<ResponsibleAnswer | undefined>({ scope: undefined, initial: undefined });
  const latest = useLatest({ request, send, enabled, t });
  useEffect(() => {
    if (!enabled) return;
    const { request, send } = latest.current;
    void send({
      request: () => request({ type: "inspect_session_branches", sessions: [sessionId] }),
      accept: (result) => () =>
        result.type === "session_branches_inspected"
          ? { attempt, items: result.items }
          : { attempt, items: [], error: errorPresentation(new Error(latest.current.t("error.unexpectedReply"))) },
      failed: (failure, _held, t) => ({ attempt, items: [], error: shownError(failure, t) }),
      wanted: () => latest.current.enabled,
    });
  }, [sessionId, enabled, attempt, reconnects]);
  return { items: value?.items ?? [], pending: enabled && value?.attempt !== attempt, error: value?.error, retry: () => setAttempt((n) => n + 1) };
}
