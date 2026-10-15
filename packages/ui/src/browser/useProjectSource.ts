import { useEffect, useRef, useState } from "react";

import { useCurrentLanguage, useT } from "../i18n/react";
import { abbreviateHome } from "../pathDisplay";
import type { ProjectSourceInfo } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { browseFailure } from "./browseError";
import { sendGitRequest } from "./gitRequests";

/** Where a project's files are read from, as far as the client knows: still asked for, known, or
 * failed with why. A known source stays while it is asked for again. */
export type SourceState =
  | { state: "loading" }
  /** `sentAt` is when the request this answers was sent (`performance.now()`), so what was read can be told to be newer than something that happened before it. */
  | { state: "loaded"; source: ProjectSourceInfo; sentAt: number }
  | { state: "error"; message: string };

/**
 * A project's source — whether it is in a Git repository, and that repository's worktrees — asked
 * of the daemon once `wanted` first holds, again on `refresh`, and after a reconnect while wanted.
 * A reply is taken only for the latest request, and only while the hook is mounted; the caller
 * mounts one per project. `refresh` settles once the request has been answered or has failed, or
 * when a newer request has taken its place.
 */
export function useProjectSource(project: string, wanted: boolean): { source: SourceState; refresh: () => Promise<void> } {
  const t = useT();
  const language = useCurrentLanguage();
  const { request, store } = useDaemon();
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  const [source, setSource] = useState<SourceState>({ state: "loading" });
  const live = useRef({ alive: true, sequence: 0, asked: false });

  const refresh = (): Promise<void> => {
    const s = live.current;
    s.asked = true;
    const sequence = ++s.sequence;
    const current = () => s.alive && s.sequence === sequence;
    const slot = `source:${project}`;
    let sentAt = 0;
    return sendGitRequest(slot, () => {
      if (!current()) return undefined;
      sentAt = performance.now();
      return request({ type: "get_project_source", project, slot });
    })
      .then((reply) => {
        if (!reply || !current() || reply.type !== "project_source" || reply.source.project !== project) return;
        setSource({ state: "loaded", source: reply.source, sentAt });
      })
      .catch((err: unknown) => {
        if (!current()) return;
        const state = store.getState();
        const root = state.projects.get(project)?.path;
        const failure = browseFailure(t, language, err, state, root && abbreviateHome(root, state.homeDir));
        if (failure.kind === "superseded" || failure.kind === "disconnected") return;
        const message = failure.kind === "failed" ? failure.message : t("browser.error.folderChanged");
        setSource({ state: "error", message });
      });
  };

  useEffect(() => {
    if (wanted && !live.current.asked) refresh();
  }, [wanted]);

  const firstEpoch = useRef(snapshotEpoch);
  useEffect(() => {
    if (snapshotEpoch !== firstEpoch.current && live.current.asked) refresh();
  }, [snapshotEpoch]);

  useEffect(() => {
    const s = live.current;
    s.alive = true;
    return () => {
      s.alive = false;
    };
  }, []);

  return { source, refresh };
}
