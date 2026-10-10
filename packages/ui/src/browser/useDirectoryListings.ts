import { useCallback, useEffect, useRef, useState } from "react";

import { useCurrentLanguage, useT } from "../i18n/react";
import { abbreviateHome } from "../pathDisplay";
import { useDaemon, useDaemonStore } from "../store";
import { browseFailure } from "./browseError";
import { treeEntries, type DirListing } from "./tree";

/** How many listings a browser has out at once: the daemon reads four at a time across all its
 * clients, and the viewer's read must never wait behind a burst of directory expansions. */
const MAX_IN_FLIGHT = 3;
/** How many listings are out at once in the window, every browser it has mounted counted, those
 * left out by a browser whose project's place was taken included (on a volume slow to answer, say),
 * and above one browser's own share, so a browser stalled on one project never starves the next.
 * Sized under the daemon's bound of 16 outstanding requests a connection together with the
 * viewer's read, its read of a change's bodies (a slot of its own, `useChangeReader.ts`) and the
 * Git mode's 2 list requests (`gitRequests.ts`), with room for requests that replace another in
 * their slot: the daemon counts the newer one before it gives the older one up, so for that moment
 * both are outstanding. A unit is reserved for each slot that can be replaced, as the viewer's two
 * can at the same moment. 10 + 1 + 2 + 2 = 15: the listings, the viewer's read, the Git mode's
 * list requests, and the last 2 the bodies slot's read and the allowance for a replacement. */
const MAX_WINDOW_IN_FLIGHT = 10;
const shared = { inFlight: 0, pumps: new Set<() => void>() };

/** How often the directories on screen are listed again while the browser is on screen and the
 * window is in front, so files an agent adds or removes show up without asking. */
export const AUTO_REFRESH_MS = 10_000;

/** How many times a listing is asked again when the directory changed while it was read. */
const CHANGED_RETRIES = 2;

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

  // The mutable side, read from callbacks that outlive the render that made them.
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
    // A browser lists what it wants as it first appears, which counts as its first refresh.
    lastRefresh: Date.now(),
  });
  state.current.connected = connected;

  const commit = (change: (current: Map<string, DirListing>) => void) => {
    const next = new Map(state.current.listings);
    change(next);
    state.current.listings = next;
    setListings(next);
  };

  const pump = useRef<() => void>(() => {});
  const send = (dir: string, attempt: number) => {
    const s = state.current;
    const sequence = s.sequence.get(dir) ?? 0;
    s.inFlight += 1;
    shared.inFlight += 1;
    const current = () => s.alive && s.sequence.get(dir) === sequence;
    let retried = false;
    request({ type: "list_project_dir", project, path: dir, slot: `list:${project}:${dir}` })
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
        const state = store.getState();
        const root = state.projects.get(project)?.path;
        const failure = browseFailure(t, language, err, state, root && abbreviateHome(root, state.homeDir));
        if (failure.kind === "superseded") return;
        if (failure.kind === "changed" && attempt < CHANGED_RETRIES) {
          retried = true;
          return void send(dir, attempt + 1);
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
        s.inFlight -= 1;
        shared.inFlight -= 1;
        // A listing that was asked for again meanwhile, or is being tried again, settles the
        // waiters when it does.
        if (current() && !retried) {
          for (const resolve of s.waiting.get(dir) ?? []) resolve();
          s.waiting.delete(dir);
        }
        for (const other of [...shared.pumps]) other();
      });
  };
  pump.current = () => {
    const s = state.current;
    while (s.alive && s.inFlight < MAX_IN_FLIGHT && shared.inFlight < MAX_WINDOW_IN_FLIGHT && s.queue.length > 0) {
      send(s.queue.shift()!, 0);
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
  useEffect(() => {
    if (!active) return;
    const tick = () => {
      if (document.visibilityState !== "visible" || state.current.inFlight > 0) return;
      refreshWanted(true);
    };
    const timer = setInterval(tick, AUTO_REFRESH_MS);
    const onVisible = () => {
      if (Date.now() - state.current.lastRefresh > AUTO_REFRESH_MS / 2) tick();
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

  useEffect(() => {
    const s = state.current;
    const myPump = () => pump.current();
    s.alive = true;
    shared.pumps.add(myPump);
    return () => {
      s.alive = false;
      shared.pumps.delete(myPump);
    };
  }, []);

  return { listings, want, refresh, refreshAll };
}
