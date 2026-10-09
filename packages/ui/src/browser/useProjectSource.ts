import { useEffect, useRef, useState } from "react";

import { useCurrentLanguage, useT } from "../i18n/react";
import { abbreviateHome } from "../pathDisplay";
import type { ProjectSourceInfo } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { browseFailure } from "./browseError";
import { sendGitRequest } from "./gitRequests";

/** Where a project's files are read from, as far as the client knows: still asked for, known, or
 * failed with why. A known source stays while it is asked for again. */
export type SourceState = { state: "loading" } | { state: "loaded"; source: ProjectSourceInfo } | { state: "error"; message: string };

/**
 * A project's source — whether it is in a Git repository, and that repository's worktrees — asked
 * of the daemon once `wanted` first holds, again on `refresh`, and after a reconnect while wanted.
 * A reply is taken only for the latest request, and only while the hook is mounted; the caller
 * mounts one per project.
 */
export function useProjectSource(project: string, wanted: boolean): { source: SourceState; refresh: () => void } {
  const t = useT();
  const language = useCurrentLanguage();
  const { request, store } = useDaemon();
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  const [source, setSource] = useState<SourceState>({ state: "loading" });
  const live = useRef({ alive: true, sequence: 0, asked: false });

  const refresh = () => {
    const s = live.current;
    s.asked = true;
    const sequence = ++s.sequence;
    const current = () => s.alive && s.sequence === sequence;
    const slot = `source:${project}`;
    sendGitRequest(slot, () => (current() ? request({ type: "get_project_source", project, slot }) : undefined))
      .then((reply) => {
        if (!reply || !current() || reply.type !== "project_source" || reply.source.project !== project) return;
        setSource({ state: "loaded", source: reply.source });
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
