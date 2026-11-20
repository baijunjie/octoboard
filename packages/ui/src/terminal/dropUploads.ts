import { useSyncExternalStore } from "react";

/** Where the uploads of a session's dropped files stand, for the notice bar: the file now going (or, while others go
 * first, the next one), its place among all the files queued for the session, and how many there are. */
export interface DropProgress {
  index: number;
  total: number;
  name: string;
  /** How much of the file now going the daemon has acknowledged, from 0 to 1; `undefined` while that cannot be told (a
   * file that is not sent in parts, such as an image saved as a pasted one, or one not yet started). */
  fraction?: number;
}

/** What one drop's upload task is given: whether the user cancelled, and the way to say where it is. Every file of the
 * drop is accounted for by `begin` and `end`; the files it does not reach are counted off when it ends. */
export interface DropTurn {
  /** Aborted when the user cancels the session's uploads. */
  signal: AbortSignal;
  begin(name: string): void;
  /** `uploaded` of the current file's `size` bytes are acknowledged, for a file sent in parts. */
  advance(uploaded: number, size: number): void;
  end(): void;
}

/** The files counted together for the notice: those of the drops made since the session's uploads were last cancelled. */
interface Counting {
  done: number;
  remaining: number;
}

interface SessionUploads {
  /** The counting new drops join, and the notice shows. */
  group: Counting;
  /** Drops not yet ended, of every counting. */
  drops: number;
  current: string | undefined;
  /** The bytes of the current file acknowledged, and its size, where it goes in parts. */
  sent: { uploaded: number; size: number } | undefined;
  cancel: AbortController;
}

/**
 * The window's queue of dropped-file uploads. A connection may hold only two attachment uploads at once, so every drop,
 * on any session's terminal, waits for the ones before it and runs alone; within it the files go one after the other.
 * It also keeps, per session, the numbers the notice bar shows, counting every file queued for the session, and the
 * cancellation that stops them all.
 */
export function createDropUploads() {
  const sessions = new Map<string, SessionUploads>();
  let snapshots = new Map<string, DropProgress>();
  const listeners = new Set<() => void>();
  let tail: Promise<unknown> = Promise.resolve();
  let queued = 0;

  const publish = () => {
    snapshots = new Map(
      [...sessions].filter(([, state]) => state.group.remaining > 0).map(([id, { group, ...state }]) => [id, { index: Math.min(group.done + 1, group.done + group.remaining), total: group.done + group.remaining, name: state.current ?? "", fraction: state.sent && state.sent.size > 0 ? state.sent.uploaded / state.sent.size : undefined }]),
    );
    for (const listener of listeners) listener();
  };

  return {
    subscribe(listener: () => void): () => void {
      listeners.add(listener);
      return () => void listeners.delete(listener);
    },
    /** The session's progress, or `undefined` when nothing is queued for it. The same object until it changes. */
    progress(sessionId: string): DropProgress | undefined {
      return snapshots.get(sessionId);
    },
    /** Stops the uploads queued for the session: the one under way ends at its next step, the others do not start. */
    cancel(sessionId: string): void {
      const state = sessions.get(sessionId);
      if (!state) return;
      state.cancel.abort();
      // A drop made after this, while the cancelled ones wind down, is a new one and goes on, counted from nothing.
      state.cancel = new AbortController();
      state.group = { done: 0, remaining: 0 };
      state.current = undefined;
      state.sent = undefined;
      publish();
    },
    /** Runs `task` for a drop of the files named `names` on the session, after every drop queued before it. */
    run<T>(sessionId: string, names: string[], task: (turn: DropTurn) => Promise<T>): Promise<T> {
      const state = sessions.get(sessionId) ?? { group: { done: 0, remaining: 0 }, drops: 0, current: undefined, sent: undefined, cancel: new AbortController() };
      sessions.set(sessionId, state);
      const group = state.group;
      state.drops++;
      group.remaining += names.length;
      state.current = state.current ?? names[0];
      publish();
      let handled = 0;
      const alone = queued++ === 0;
      const turn: DropTurn = {
        signal: state.cancel.signal,
        // What is shown of the file under way belongs to the counting being shown, not to one a cancel left behind.
        begin(name) {
          if (group === state.group) {
            state.current = name;
            state.sent = undefined;
          }
          publish();
        },
        advance(uploaded, size) {
          if (group === state.group) state.sent = { uploaded, size };
          publish();
        },
        end() {
          handled++;
          if (group === state.group) state.sent = undefined;
          group.done++;
          group.remaining--;
          publish();
        },
      };
      // A drop with none before it starts at once, so it judges its first file by what is true now.
      const result = (alone ? task(turn) : tail.then(() => task(turn))).finally(() => {
        queued--;
        group.remaining -= names.length - handled;
        if (--state.drops === 0) sessions.delete(sessionId);
        publish();
      });
      tail = result.catch(() => {});
      return result;
    },
  };
}

/** The window's one queue. */
export const dropUploads = createDropUploads();

/** The progress of the uploads queued for `sessionId`, which re-renders as it moves. */
export function useDropProgress(sessionId: string | undefined): DropProgress | undefined {
  return useSyncExternalStore(dropUploads.subscribe, () => (sessionId === undefined ? undefined : dropUploads.progress(sessionId)));
}

/** Whether any uploads are queued for `sessionId`: a boolean, so that what only follows the notice's appearance does not
 * render with every part. */
export function useDropUploadsShown(sessionId: string | undefined): boolean {
  return useSyncExternalStore(dropUploads.subscribe, () => sessionId !== undefined && dropUploads.progress(sessionId) !== undefined);
}
