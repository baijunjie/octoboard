import { useEffect } from "react";

import { listLane } from "../../browse/budget";
import { usePoll } from "../../components/hooks/usePoll";
import { useLatestRequest } from "../../components/hooks/useLatestRequest";
import type { ErrorPresentation } from "../../errors/errorPresentation";
import type { BranchInfo, UpstreamInfo } from "../../protocol";
import { useDaemon } from "../../store";
import { sameValue } from "../../values/sameValue";
import { AUTO_REFRESH_CATCH_UP_MS, AUTO_REFRESH_MS } from "../autoRefresh";
import { gitMoment } from "./gitRequests";

/** A repository's local branches as the client holds them. `upstreamsComplete` is false when the
 * daemon could not list the remote-tracking branches, so that no branch carries an upstream (nor the
 * list a `defaultBranch`) though it may have one. `defaultBranch` is the remote branch the
 * repository's remote default branch points at, given when the main working copy's branch has no
 * upstream. `sentAt` is when the request this list
 * answers was sent (`gitMoment`), so a list can be told to be newer than something received
 * before it. A list or a failure stays on screen while a refresh is out, and a list stays through
 * a refresh that fails, with that failure beside it (`error`) until a refresh succeeds. */
export type BranchList =
  | { state: "loading" }
  | { state: "loaded"; branches: BranchInfo[]; complete: boolean; upstreamsComplete?: boolean; defaultBranch?: UpstreamInfo; sentAt: number; error?: ErrorPresentation }
  | { state: "error"; message: ErrorPresentation };

/**
 * The local branches of a project's repository, asked of the daemon while `active`: once at first,
 * every `AUTO_REFRESH_MS` while the window is in front, on coming back to it after a while, on
 * `refresh`, and after a reconnect — so a branch created, moved or deleted shows in the selector,
 * and a comparison made from a branch that has moved since is made again. A reply is taken only for
 * the latest request, and only while the hook is mounted. A reply that lists the same branches keeps
 * the `branches` held, so nothing built from them is built again; only `sentAt` moves on. `refresh`
 * settles once the request has been answered or has failed, or when a newer request has taken its
 * place.
 */
export function useBranchList(project: string, active: boolean): { branches: BranchList; refresh: () => Promise<void> } {
  const { request } = useDaemon();
  const latest = useLatestRequest<BranchList>({
    scope: project,
    initial: { state: "loading" },
    project,
    onReconnect: ({ asked }) => {
      if (asked && active) void send();
    },
  });

  const send = (): Promise<void> => {
    const slot = `branches:${project}`;
    let sentAt = 0;
    return latest.send({
      lane: listLane,
      slot,
      request: () => {
        sentAt = gitMoment();
        return request({ type: "list_project_branches", project, slot });
      },
      accept: (reply) => {
        if (reply.project !== project) return undefined;
        return (held) => {
          const kept = held.state === "loaded" && sameValue(held.branches, reply.branches) ? held.branches : reply.branches;
          return { state: "loaded", branches: kept, complete: reply.complete, upstreamsComplete: reply.upstreams_complete, defaultBranch: reply.default_branch, sentAt };
        };
      },
      // A list held stays through a failed refresh, the failure beside it.
      failed: (failure, held, t) => {
        const message = failure.kind === "failed" ? failure.message : t("git.error.changing");
        return held.state === "loaded" ? { ...held, error: message } : { state: "error", message };
      },
    });
  };

  useEffect(() => {
    if (active && !latest.asked()) void send();
  }, [active]);

  // The periodic refresh, and one on coming back to the window, or to a pane that was hidden,
  // after a while.
  usePoll({ pace: active ? AUTO_REFRESH_MS : undefined, catchUpAfter: AUTO_REFRESH_CATCH_UP_MS, busy: latest.busy, lastAt: latest.sentAt, run: () => void send() });

  return {
    branches: latest.value,
    refresh: () => (active ? send() : Promise.resolve()),
  };
}
