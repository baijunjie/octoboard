import { useEffect } from "react";

import { listLane } from "../../browse/budget";
import { useLatestRequest } from "../../components/hooks/useLatestRequest";
import type { ErrorPresentation } from "../../errors/errorPresentation";
import type { ProjectSourceInfo } from "../../protocol";
import { useDaemon } from "../../store";
import { gitMoment } from "./gitRequests";

/** Where a project's files are read from, as far as the client knows: still asked for, known, or
 * failed with why. A known source stays while it is asked for again. */
export type SourceState =
  | { state: "loading" }
  /** `sentAt` is when the request this answers was sent (`gitMoment`), so what was read can be told to be newer than something that happened before it. */
  | { state: "loaded"; source: ProjectSourceInfo; sentAt: number }
  | { state: "error"; message: ErrorPresentation };

/**
 * A project's source — whether it is in a Git repository, and that repository's worktrees — asked
 * of the daemon once `wanted` first holds, again on `refresh`, and after a reconnect while wanted.
 * A reply is taken only for the latest request, and only while the hook is mounted; the caller
 * mounts one per project. `refresh` settles once the request has been answered or has failed, or
 * when a newer request has taken its place. `askedByPerson` is for a refresh a person asked for
 * (Refresh, Try again), on which the daemon looks again for a Git it found missing.
 */
export function useProjectSource(project: string, wanted: boolean): { source: SourceState; refresh: (askedByPerson?: boolean) => Promise<void> } {
  const { request } = useDaemon();
  const latest = useLatestRequest<SourceState>({
    scope: project,
    initial: { state: "loading" },
    project,
    onReconnect: ({ asked }) => {
      if (asked) void refresh();
    },
  });

  const refresh = (askedByPerson = false): Promise<void> => {
    const slot = `source:${project}`;
    let sentAt = 0;
    return latest.send({
      lane: listLane,
      slot,
      request: () => {
        sentAt = gitMoment();
        return request({ type: "get_project_source", project, slot, ...(askedByPerson && { asked_by_person: true }) });
      },
      accept: (reply) =>
        reply.source.project === project ? () => ({ state: "loaded", source: reply.source, sentAt }) : undefined,
      failed: (failure, _held, t) => ({ state: "error", message: failure.kind === "failed" ? failure.message : t("browser.error.folderChanged") }),
    });
  };

  useEffect(() => {
    if (wanted && !latest.asked()) void refresh();
  }, [wanted]);

  return { source: latest.value, refresh };
}
