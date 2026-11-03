import { useEffect, useRef } from "react";

import { listLane } from "../../browse/budget";
import { usePoll } from "../../components/hooks/usePoll";
import { useLatestRequest } from "../../components/hooks/useLatestRequest";
import type { ErrorPresentation } from "../../errors/errorPresentation";
import { WORKTREE_UNAVAILABLE } from "../../protocol";
import { useDaemon } from "../../store";
import { sameValue } from "../../values/sameValue";
import { AUTO_REFRESH_CATCH_UP_MS, AUTO_REFRESH_MS } from "../autoRefresh";
import { changeItems, type ChangeItem } from "./changes";

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
 * cancels the older one in the daemon too. A reply listing the same changes keeps the `items` held,
 * so nothing built from them is built again. `refresh` settles once the list it asked for has been
 * taken or has failed, or when a newer request has taken its place.
 */
export function useChangeList(
  project: string,
  worktree: string | undefined,
  active: boolean,
): { list: ChangeList; refresh: () => Promise<void> } {
  const { request } = useDaemon();
  // Another worktree is another source: nothing of the previous one's list stands.
  const latest = useLatestRequest<ChangeList>({
    scope: worktree,
    initial: { state: "loading" },
    project,
    onReconnect: () => {
      if (shownFor.current && active) void send(true);
    },
  });

  /** Asks for the list. In the `background` a held list or failure stays on screen meanwhile;
   * otherwise a failure gives way to `loading`, as the user asked again. */
  const send = (background: boolean): Promise<void> => {
    const slot = `changes:${project}`;
    return latest.send({
      lane: listLane,
      slot,
      request: () => request({ type: "list_project_changes", project, ...(worktree === undefined ? {} : { worktree }), slot }),
      pending: (list) =>
        list.state === "loading" || (list.state === "error" && !background) ? { state: "loading" } : list.refreshing ? list : { ...list, refreshing: true },
      accept: (reply) => {
        if (reply.type !== "project_changes" || reply.project !== project || (reply.worktree ?? undefined) !== worktree) return undefined;
        const items = changeItems(reply.changes);
        return (held) => {
          const kept = held.state === "loaded" && sameValue(held.items, items) ? held.items : items;
          return { state: "loaded", head: reply.head, items: kept, complete: reply.complete, refreshing: false };
        };
      },
      // Left as it is; the reconnect asks again.
      disconnected: (list) => (list.state === "loading" ? list : { ...list, refreshing: false }),
      failed: (failure, _held, t) => {
        const unavailable = failure.kind === "failed" && failure.code === WORKTREE_UNAVAILABLE;
        const message = failure.kind === "failed" ? failure.message : t("git.error.changing");
        return { state: "error", message, unavailable, refreshing: false };
      },
    });
  };

  const shownFor = useRef<{ worktree: string | undefined } | undefined>(undefined);
  useEffect(() => {
    if (!active) return;
    if (shownFor.current && shownFor.current.worktree === worktree) return;
    shownFor.current = { worktree };
    void send(false);
  }, [active, worktree]);

  // The periodic refresh, and one on coming back to the window, or to a pane that was hidden,
  // after a while.
  usePoll({
    pace: active ? AUTO_REFRESH_MS : undefined,
    catchUpAfter: AUTO_REFRESH_CATCH_UP_MS,
    resetKey: worktree,
    busy: latest.busy,
    lastAt: latest.sentAt,
    run: () => void send(true),
  });

  return {
    list: latest.value,
    refresh: () => (active ? send(false) : Promise.resolve()),
  };
}
