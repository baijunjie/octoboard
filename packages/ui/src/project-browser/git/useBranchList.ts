import { useEffect, useLayoutEffect, useRef, useState } from "react";

import type { ErrorPresentation } from "../../errors/errorPresentation";
import { useCurrentLanguage, useT } from "../../i18n/react";
import type { BranchInfo, UpstreamInfo } from "../../protocol";
import { useDaemon, useDaemonStore } from "../../store";
import { projectBrowseFailure } from "../browseError";
import { gitMoment } from "./gitRequests";
import { sendBrowseListRequest } from "../browseListRequests";
import { useAutoRefresh } from "../autoRefresh";

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
 * the latest request, and only while the hook is mounted. `refresh` settles once the request has
 * been answered or has failed, or when a newer request has taken its place.
 */
export function useBranchList(project: string, active: boolean): { branches: BranchList; refresh: () => Promise<void> } {
  const t = useT();
  const language = useCurrentLanguage();
  const { request, store } = useDaemon();
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  const [branches, setBranches] = useState<BranchList>({ state: "loading" });
  const live = useRef({ alive: true, active, sequence: 0, inFlight: false, asked: false, lastRefresh: 0 });
  useLayoutEffect(() => {
    live.current.active = active;
  });

  const send = (): Promise<void> => {
    const s = live.current;
    s.asked = true;
    const sequence = ++s.sequence;
    const current = () => s.alive && s.sequence === sequence;
    s.inFlight = true;
    s.lastRefresh = Date.now();
    const slot = `branches:${project}`;
    let sentAt = 0;
    return sendBrowseListRequest(slot, () => {
      if (!current()) return undefined;
      sentAt = gitMoment();
      return request({ type: "list_project_branches", project, slot });
    })
      .then((reply) => {
        if (!reply || !current() || reply.type !== "project_branches" || reply.project !== project) return;
        setBranches({ state: "loaded", branches: reply.branches, complete: reply.complete, upstreamsComplete: reply.upstreams_complete, defaultBranch: reply.default_branch, sentAt });
      })
      .catch((err: unknown) => {
        if (!current()) return;
        const failure = projectBrowseFailure(t, language, err, store.getState(), project);
        // A list held stays through a lost or superseded request; the reconnect asks again.
        if (failure.kind !== "failed") return;
        const message = failure.message;
        setBranches((held) => (held.state === "loaded" ? { ...held, error: message } : { state: "error", message }));
      })
      .finally(() => {
        if (s.sequence === sequence) s.inFlight = false;
      });
  };

  useEffect(() => {
    if (active && !live.current.asked) send();
  }, [active]);

  // The periodic refresh, and one on coming back to the window, or to a pane that was hidden,
  // after a while.
  useAutoRefresh(active, undefined, { busy: () => live.current.inFlight, lastAt: () => live.current.lastRefresh, run: () => void send() });

  const firstEpoch = useRef(snapshotEpoch);
  useEffect(() => {
    if (snapshotEpoch !== firstEpoch.current && live.current.asked && live.current.active) send();
  }, [snapshotEpoch]);

  useEffect(() => {
    const s = live.current;
    s.alive = true;
    return () => {
      s.alive = false;
    };
  }, []);

  return {
    branches,
    refresh: () => (live.current.active ? send() : Promise.resolve()),
  };
}
