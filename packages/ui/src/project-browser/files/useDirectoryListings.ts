import { useCallback, useEffect, useRef, useState } from "react";

import { listingLane } from "../../browse/budget";
import { CHANGED_RETRIES, projectBrowseFailure } from "../../browse/failure";
import { usePoll } from "../../components/hooks/usePoll";
import { useCurrentLanguage, useT } from "../../i18n/react";
import { useDaemon, useDaemonStore } from "../../store";
import { AUTO_REFRESH_CATCH_UP_MS, AUTO_REFRESH_MS } from "../autoRefresh";
import { treeEntries, type DirListing } from "../tree";

/** How many listings a browser has out at once: the daemon reads four at a time across all its
 * clients, and the viewer's read must never wait behind a burst of directory expansions. The
 * window's lane of listings (`listingLane`), shared by every browser mounted, holds more than one
 * browser's share, so a browser stalled on one project never starves the next. */
const MAX_IN_FLIGHT = 3;

export interface DirectoryListings {
  listings: ReadonlyMap<string, DirListing>;
  /** The directories on screen whose listing is wanted (the project's own and the expanded ones):
   * each is listed once when it first appears, and these are what a refresh lists again. */
  want: (dirs: readonly string[]) => void;
  /** Lists `dirs` again, showing what is held meanwhile; for an expanded directory and a retry. */
  refresh: (dirs: readonly string[]) => void;
  /** Lists every directory on screen again: the manual refresh, and the periodic one. Settles once
   * each has been listed or has failed. */
  refreshAll: () => Promise<void>;
}

/**
 * The listings of one project's directories, asked of the daemon on demand: each directory the
 * caller `want`s once when it first appears, again on `refresh`, every `AUTO_REFRESH_MS` while
 * `active`, and all of them after a reconnect, since the daemon keeps nothing from an earlier
 * connection.
 *
 * A reply is taken only for the listing most recently asked for its directory, and only while this
 * hook is mounted: the caller mounts one per project, so a reply for another project, an earlier
 * listing or an earlier connection never reaches the tree. Each directory has its own slot, so a
 * newer listing of it cancels the older one in the daemon too. When the project's directory turns
 * out to be another one than before (`root_id`), everything held is dropped and listed afresh.
 */
export function useDirectoryListings(project: string, active: boolean): DirectoryListings {
  const t = useT();
  const language = useCurrentLanguage();
  const { request, store } = useDaemon();
  const connected = useDaemonStore((s) => s.connectionState === "open");
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  const [listings, setListings] = useState<ReadonlyMap<string, DirListing>>(new Map());

  // The mutable side, read from callbacks that outlive the render that made them. The React Compiler
  // leaves this hook uncompiled whatever is done about the rule findings marked below: `send` calls
  // itself to retry, which it takes for a call during render.
  const state = useRef({
    alive: true,
    listings: listings as ReadonlyMap<string, DirListing>,
    wanted: [] as readonly string[],
    connected,
    rootId: undefined as string | undefined,
    sequence: new Map<string, number>(),
    // Who waits for each directory's listing, resolved when the latest one asked for settles.
    waiting: new Map<string, (() => void)[]>(),
    queue: [] as string[],
    inFlight: 0,
    // Turns asked of the window's lane and not yet given: each takes the head of the queue then.
    claimed: 0,
    // A browser lists what it wants as it first appears, which counts as its first refresh.
    // eslint-disable-next-line react-hooks/purity
    lastRefresh: Date.now(),
  });
  // eslint-disable-next-line react-hooks/refs
  state.current.connected = connected;

  const commit = (change: (current: Map<string, DirListing>) => void) => {
    const next = new Map(state.current.listings);
    change(next);
    state.current.listings = next;
    setListings(next);
  };

  const pump = useRef<() => void>(() => {});
  /** Lists `dir`, and again when it changed while it was read; settles once that is over. */
  const send = (dir: string, attempt: number): Promise<void> => {
    const s = state.current;
    const sequence = s.sequence.get(dir) ?? 0;
    const current = () => s.alive && s.sequence.get(dir) === sequence;
    let retried = false;
    return request({ type: "list_project_dir", project, path: dir, slot: `list:${project}:${dir}` })
      .then((reply) => {
        if (!current() || reply.type !== "project_dir" || reply.project !== project || reply.path !== dir) return;
        // The project's directory was replaced by another: nothing listed from the old one stands.
        const replaced = s.rootId !== undefined && s.rootId !== reply.root_id;
        s.rootId = reply.root_id;
        commit((next) => {
          if (replaced) next.clear();
          next.set(dir, { state: "loaded", entries: treeEntries(reply.entries), complete: reply.complete, refreshing: false });
        });
        if (replaced) for (const other of s.wanted) if (other !== dir) enqueue(other);
      })
      .catch((err: unknown) => {
        if (!current()) return;
        const failure = projectBrowseFailure(t, language, err, store.getState(), project);
        if (failure.kind === "superseded") return;
        if (failure.kind === "changed" && attempt < CHANGED_RETRIES) {
          retried = true;
          return send(dir, attempt + 1);
        }
        if (failure.kind === "disconnected") {
          // Left as it is; the reconnect lists it again.
          return commit((next) => {
            const held = next.get(dir);
            if (held?.state === "loaded") next.set(dir, { ...held, refreshing: false });
          });
        }
        const message = failure.kind === "failed" ? failure.message : t("browser.error.folderChanged");
        commit((next) => next.set(dir, { state: "error", message }));
      })
      .finally(() => {
        // A listing that was asked for again meanwhile, or is being tried again, settles the
        // waiters when it does.
        if (current() && !retried) {
          for (const resolve of s.waiting.get(dir) ?? []) resolve();
          s.waiting.delete(dir);
        }
      });
  };
  // eslint-disable-next-line react-hooks/refs
  pump.current = () => {
    const s = state.current;
    while (s.alive && s.inFlight + s.claimed < MAX_IN_FLIGHT && s.queue.length > s.claimed) {
      s.claimed += 1;
      void listingLane
        .send(() => {
          s.claimed -= 1;
          const dir = s.queue.shift();
          if (!s.alive || dir === undefined) return undefined;
          s.inFlight += 1;
          // A retry keeps the place in the lane.
          return send(dir, 0).finally(() => {
            s.inFlight -= 1;
          });
        })
        .then(() => pump.current());
    }
  };

  /** Asks for `dir`'s listing, keeping a loaded one on screen meanwhile. In the `background` (the
   * periodic refresh, a reconnect) a failure stays on screen too, and its Try again with it; asked
   * by the user, it gives way to `loading`. The promise settles with that listing. */
  const enqueue = (dir: string, background = false): Promise<void> => {
    const s = state.current;
    if (!s.connected) return Promise.resolve();
    const held = s.listings.get(dir);
    if (held?.state === "loaded") {
      if (!held.refreshing) commit((next) => next.set(dir, { ...held, refreshing: true }));
    } else if (held?.state !== "loading" && !(held?.state === "error" && background)) {
      commit((next) => next.set(dir, { state: "loading" }));
    }
    // Counted from the moment it is asked for: a reply to an earlier listing of `dir` that is still
    // out is not taken in its place.
    s.sequence.set(dir, (s.sequence.get(dir) ?? 0) + 1);
    if (!s.queue.includes(dir)) s.queue.push(dir);
    const settled = new Promise<void>((resolve) => s.waiting.set(dir, [...(s.waiting.get(dir) ?? []), resolve]));
    pump.current();
    return settled;
  };

  const refresh = useCallback((dirs: readonly string[]) => dirs.forEach((dir) => enqueue(dir)), []);
  const refreshWanted = async (background: boolean) => {
    state.current.lastRefresh = Date.now();
    await Promise.all(state.current.wanted.map((dir) => enqueue(dir, background)));
  };
  const refreshAll = useCallback(() => refreshWanted(false), []);

  // A directory that comes on screen is listed once; after that only a refresh lists it again.
  const want = useCallback((dirs: readonly string[]) => {
    state.current.wanted = dirs;
    for (const dir of dirs) if (!state.current.listings.has(dir)) enqueue(dir);
  }, []);
  useEffect(() => {
    if (connected) want(state.current.wanted);
  }, [connected]);

  // A reconnect (or a lagging connection's fresh snapshot) means nothing held is known to be current.
  const firstEpoch = useRef(snapshotEpoch);
  useEffect(() => {
    if (snapshotEpoch === firstEpoch.current) return;
    commit((next) => {
      for (const dir of [...next.keys()]) if (!state.current.wanted.includes(dir)) next.delete(dir);
    });
    refreshWanted(true);
  }, [snapshotEpoch]);

  // The periodic refresh, and one on coming back to the window, or to a pane that was hidden,
  // after a while.
  usePoll({
    pace: active ? AUTO_REFRESH_MS : undefined,
    catchUpAfter: AUTO_REFRESH_CATCH_UP_MS,
    busy: () => state.current.inFlight > 0,
    lastAt: () => state.current.lastRefresh,
    run: () => void refreshWanted(true),
  });

  useEffect(() => {
    const s = state.current;
    s.alive = true;
    return () => {
      s.alive = false;
    };
  }, []);

  return { listings, want, refresh, refreshAll };
}
