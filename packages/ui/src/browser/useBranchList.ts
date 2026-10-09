import { useEffect, useRef, useState } from "react";

import { useCurrentLanguage, useT } from "../i18n/react";
import { abbreviateHome } from "../pathDisplay";
import type { BranchInfo } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { browseFailure } from "./browseError";
import { sendGitRequest } from "./gitRequests";
import { AUTO_REFRESH_MS } from "./useDirectoryListings";

/** A repository's local branches as the client holds them. `sentAt` is when the request this list
 * answers was sent, so a list can be told to be newer than something received before it. A list or
 * a failure stays on screen while a refresh is out, and a list stays through a refresh that fails,
 * with that failure beside it (`error`) until a refresh succeeds. */
export type BranchList =
  | { state: "loading" }
  | { state: "loaded"; branches: BranchInfo[]; complete: boolean; sentAt: number; error?: string }
  | { state: "error"; message: string };

/**
 * The local branches of a project's repository, asked of the daemon while `active`: once at first,
 * every `AUTO_REFRESH_MS` while the window is in front, on coming back to it after a while, on
 * `refresh`, and after a reconnect — so a branch created, moved or deleted shows in the selectors,
 * and a comparison can tell that a branch it compared has moved since. A reply is taken only for
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
  live.current.active = active;

  const send = (): Promise<void> => {
    const s = live.current;
    s.asked = true;
    const sequence = ++s.sequence;
    const current = () => s.alive && s.sequence === sequence;
    s.inFlight = true;
    s.lastRefresh = Date.now();
    const slot = `branches:${project}`;
    let sentAt = 0;
    return sendGitRequest(slot, () => {
      if (!current()) return undefined;
      sentAt = performance.now();
      return request({ type: "list_project_branches", project, slot });
    })
      .then((reply) => {
        if (!reply || !current() || reply.type !== "project_branches" || reply.project !== project) return;
        setBranches({ state: "loaded", branches: reply.branches, complete: reply.complete, sentAt });
      })
      .catch((err: unknown) => {
        if (!current()) return;
        const state = store.getState();
        const root = state.projects.get(project)?.path;
        const failure = browseFailure(t, language, err, state, root && abbreviateHome(root, state.homeDir));
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
    if (!active) return;
    const tick = () => {
      if (document.visibilityState !== "visible" || live.current.inFlight) return;
      send();
    };
    if (!live.current.asked) send();
    const timer = setInterval(tick, AUTO_REFRESH_MS);
    const onVisible = () => {
      if (Date.now() - live.current.lastRefresh > AUTO_REFRESH_MS / 2) tick();
    };
    onVisible();
    document.addEventListener("visibilitychange", onVisible);
    window.addEventListener("focus", onVisible);
    return () => {
      clearInterval(timer);
      document.removeEventListener("visibilitychange", onVisible);
      window.removeEventListener("focus", onVisible);
    };
  }, [active]);

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
