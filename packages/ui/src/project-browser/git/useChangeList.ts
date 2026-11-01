import { useEffect, useLayoutEffect, useRef, useState } from "react";

import type { ErrorPresentation } from "../../errors/errorPresentation";
import { useCurrentLanguage, useT } from "../../i18n/react";
import { WORKTREE_UNAVAILABLE } from "../../protocol";
import { useDaemon, useDaemonStore } from "../../store";
import { projectBrowseFailure } from "../browseError";
import { changeItems, type ChangeItem } from "./changes";
import { sendBrowseListRequest } from "../browseListRequests";
import { useAutoRefresh } from "../autoRefresh";

/** How many times a change list is asked again when what it read changed while it was read. */
const CHANGED_RETRIES = 2;

/** A worktree's change list as the client holds it. A list or a failure stays on screen while a
 * refresh is out (`refreshing`), so a refresh never blanks the list or takes a failure, and the
 * Try again on it, away; the user asking again is what shows `loading`. `unavailable` marks a
 * failure because the worktree is gone: the list is never taken from another one in its place. */
export type ChangeList =
  | { state: "loading" }
  | { state: "loaded"; head: string | null; items: ChangeItem[]; complete: boolean; refreshing: boolean }
  | { state: "error"; message: ErrorPresentation; unavailable: boolean; refreshing: boolean };

/**
 * The uncommitted changes of one worktree of a project, asked of the daemon while `active`: once
 * at first, every `AUTO_REFRESH_MS` while the window is in front, on coming back to it after a
 * while, on `refresh`, and after a reconnect. `worktree` is the worktree's id, or none for the one
 * holding the project's directory; another one is a new source, whose list starts afresh.
 *
 * A reply is taken only for the latest request, of this project and of the worktree it was asked
 * for, and only while the hook is mounted. The list has one slot per project, so a newer request
 * cancels the older one in the daemon too. `refresh` settles once the list it asked for has been
 * taken or has failed, or when a newer request has taken its place.
 */
export function useChangeList(
  project: string,
  worktree: string | undefined,
  active: boolean,
): { list: ChangeList; refresh: () => Promise<void> } {
  const t = useT();
  const language = useCurrentLanguage();
  const { request, store } = useDaemon();
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  // What is held, with the worktree it is of: a list held for another worktree is not this one's.
  const [held, setHeld] = useState<{ worktree: string | undefined; list: ChangeList }>({ worktree, list: { state: "loading" } });
  const live = useRef({ alive: true, active, sequence: 0, inFlight: false, worktree, lastRefresh: 0 });
  useLayoutEffect(() => {
    live.current.worktree = worktree;
    live.current.active = active;
  });

  /** Asks for the list. In the `background` a held list or failure stays on screen meanwhile;
   * otherwise a failure gives way to `loading`, as the user asked again. */
  const send = (background: boolean, attempt = 0): Promise<void> => {
    const s = live.current;
    const sequence = ++s.sequence;
    const asked = s.worktree;
    const current = () => s.alive && s.sequence === sequence && s.worktree === asked;
    const setList = (change: (list: ChangeList) => ChangeList) =>
      setHeld((now) => ({ worktree: asked, list: change(now.worktree === asked ? now.list : { state: "loading" }) }));
    s.inFlight = true;
    // `send` calls itself to retry, which the purity rule takes for a call during render.
    // eslint-disable-next-line react-hooks/purity
    s.lastRefresh = Date.now();
    setList((list) =>
      list.state === "loading" || (list.state === "error" && !background) ? { state: "loading" } : list.refreshing ? list : { ...list, refreshing: true },
    );
    const slot = `changes:${project}`;
    return sendBrowseListRequest(slot, () =>
      current() ? request({ type: "list_project_changes", project, ...(asked === undefined ? {} : { worktree: asked }), slot }) : undefined,
    )
      .then((reply) => {
        if (!reply || !current() || reply.type !== "project_changes" || reply.project !== project || (reply.worktree ?? undefined) !== asked) return;
        setList(() => ({ state: "loaded", head: reply.head, items: changeItems(reply.changes), complete: reply.complete, refreshing: false }));
      })
      .catch((err: unknown) => {
        if (!current()) return;
        const failure = projectBrowseFailure(t, language, err, store.getState(), project);
        if (failure.kind === "superseded") return;
        if (failure.kind === "changed" && attempt < CHANGED_RETRIES) return send(background, attempt + 1);
        if (failure.kind === "disconnected") {
          // Left as it is; the reconnect asks again.
          return setList((list) => (list.state === "loading" ? list : { ...list, refreshing: false }));
        }
        const unavailable = failure.kind === "failed" && failure.code === WORKTREE_UNAVAILABLE;
        const message = failure.kind === "failed" ? failure.message : t("git.error.changing");
        setList(() => ({ state: "error", message, unavailable, refreshing: false }));
      })
      .finally(() => {
        if (s.sequence === sequence) s.inFlight = false;
      });
  };

  // Another worktree is another source: nothing of the previous one's list stands.
  const shownFor = useRef<{ worktree: string | undefined } | undefined>(undefined);
  useEffect(() => {
    if (!active) return;
    if (shownFor.current && shownFor.current.worktree === worktree) return;
    shownFor.current = { worktree };
    send(false);
  }, [active, worktree]);

  const firstEpoch = useRef(snapshotEpoch);
  useEffect(() => {
    if (snapshotEpoch !== firstEpoch.current && shownFor.current && live.current.active) send(true);
  }, [snapshotEpoch]);

  // The periodic refresh, and one on coming back to the window, or to a pane that was hidden,
  // after a while.
  useAutoRefresh(active, worktree, { busy: () => live.current.inFlight, lastAt: () => live.current.lastRefresh, run: () => send(true) });

  useEffect(() => {
    const s = live.current;
    s.alive = true;
    return () => {
      s.alive = false;
    };
  }, []);

  return {
    list: held.worktree === worktree ? held.list : { state: "loading" },
    refresh: () => (live.current.active ? send(false) : Promise.resolve()),
  };
}
