import { useEffect, useRef, useState } from "react";

import { errorPresentation, type ErrorPresentation } from "../../errors/errorPresentation";
import { unexpectedFailureDiagnostic } from "../../errors/errorText";
import type { WindowFocusCapability } from "../../platform";
import { useOptionalPlatform } from "../../platform/react";
import { sameValue } from "../../values/sameValue";
import { useLatest } from "./useLatest";
import { onWindowFocusReturn } from "./windowFocus";

/** How a poll is paced. The callbacks are read each time they are needed. */
export interface Poll {
  /** How long after the last read the next one falls due, in milliseconds. */
  pace: number;
  /** Reads once. A promise it returns holds the next poll back until it settles, the pace counted from then; one
   * that resolves to `false` stops the polling until it is started again. */
  run: () => unknown;
  /** Whether a read the caller sent itself is out: a poll that falls due meanwhile is put off by another `pace`. */
  busy?: () => boolean;
  /** When the caller last read (`Date.now()`), its own reads besides the poll's included, so that one it sent itself
   * puts the next poll off. Without it, a poll that has not read yet reads at once when it starts, whether the
   * document is hidden or not: what it answers is wanted before anything can be shown. */
  lastAt?: () => number;
  /** On coming back to a document that was hidden or a window that was behind another, and when polling starts: read
   * at once if the last read is at least this old. At most `pace`, which it defaults to, so a poll that fell due
   * while the document was hidden is always read on return. */
  catchUpAfter?: number;
}

const hidden = () => document.hidden || document.visibilityState !== "visible";

/**
 * Polls `poll.run` at its pace until the function this returns is called, never while the document is hidden: a poll
 * that falls due then waits for it to be shown, and is read at once on return (see `catchUpAfter`). `windowFocus` is
 * the platform's, which tells a return to the window where the document's own events do not.
 */
export function startPolling(poll: () => Poll, windowFocus?: WindowFocusCapability): () => void {
  let stopped = false;
  let halted = false;
  let polling = false;
  let ownAt = -Infinity;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const last = () => Math.max(ownAt, poll().lastAt?.() ?? -Infinity);
  const schedule = (delay: number) => {
    clearTimeout(timer);
    timer = setTimeout(due, Math.max(0, delay));
  };
  const read = () => {
    clearTimeout(timer);
    timer = undefined;
    polling = true;
    const settled = (outcome: unknown) => {
      polling = false;
      ownAt = Date.now();
      if (stopped) return;
      if (outcome === false) halted = true;
      else schedule(poll().pace);
    };
    let started: unknown;
    try {
      started = poll().run();
    } catch {
      // A read that throws has failed like one that rejects: the polling goes on.
      return settled(undefined);
    }
    Promise.resolve(started).then(settled, () => settled(undefined));
  };
  function due() {
    timer = undefined;
    if (stopped || halted || polling) return;
    const { pace, busy } = poll();
    const wait = last() + pace - Date.now();
    if (wait > 0) return schedule(wait);
    // Read again by `catchUp` once the document is shown.
    if (hidden()) return;
    if (busy?.()) return schedule(pace);
    read();
  }
  const catchUp = () => {
    if (stopped || halted || polling || hidden()) return;
    const { pace, busy, catchUpAfter = pace } = poll();
    if (busy?.()) {
      if (timer === undefined) schedule(pace);
      return;
    }
    if (Date.now() - last() >= Math.min(catchUpAfter, pace)) read();
    else if (timer === undefined) schedule(last() + pace - Date.now());
  };

  document.addEventListener("visibilitychange", catchUp);
  const undoFocus = onWindowFocusReturn(windowFocus, catchUp);
  if (last() === -Infinity) read();
  else catchUp();
  return () => {
    stopped = true;
    clearTimeout(timer);
    document.removeEventListener("visibilitychange", catchUp);
    undoFocus();
  };
}

/**
 * Polls while `pace` is set (`startPolling`), with the callbacks of the latest render, so a read that fails words its
 * error as the current language does. A new `pace`, `catchUpAfter` or `resetKey` starts it afresh.
 */
export function usePoll({ resetKey, ...poll }: Omit<Poll, "pace"> & { pace: number | undefined; resetKey?: unknown }): void {
  const latest = useLatest(poll);
  const { pace, catchUpAfter } = poll;
  const windowFocus = useOptionalPlatform()?.windowFocus;
  useEffect(() => {
    if (pace === undefined) return;
    return startPolling(() => ({ ...latest.current, pace, catchUpAfter }), windowFocus);
  }, [pace, catchUpAfter, resetKey, latest, windowFocus]);
}

/** Error messages and stacks are not enumerable; compare the data a caller can show explicitly. */
function failureValue(error: ErrorPresentation | undefined) {
  return typeof error === "object" ? { code: error.code, params: error.params, message: error.message, diagnostic: unexpectedFailureDiagnostic(error) } : error;
}

/** What `usePolled` answers for the caller's `key`; `revise` and `retry` act on that answer. */
export interface Polled<T> {
  value: T | undefined;
  error: ErrorPresentation | undefined;
  /** Clears the error and asks again now. */
  retry: () => void;
  /** Replaces the answer with `change` of it at once and asks again now: an answer that was already on its way is
   * dropped, so it cannot undo the change. */
  revise: (change: (value: T | undefined) => T | undefined) => void;
}

/** What `ask` answers for `key`, asked for at once and then `pace` milliseconds after each answer while the caller is
 * mounted (`startPolling`), and not while the document is hidden; `undefined` until the first answer, and again when
 * `key` changes. `key` is `undefined`, or `pace` is, for nothing to ask. A failure is `error` for the caller to show,
 * and a later failure keeps the last answer. Only a changed answer or failure is published (`sameValue`, or `same`
 * when given), so an idle poll never renders its owner again. With `stopsOnFirstFailure`, a failure before the first
 * answer stops polling until `retry` asks again. `ask` must keep its identity while it asks for the same thing. */
export function usePolled<T>(
  key: string | undefined,
  ask: (key: string) => Promise<T>,
  pace: number | undefined,
  { stopsOnFirstFailure = false, same = sameValue }: { stopsOnFirstFailure?: boolean; same?: (held: T | undefined, answer: T) => boolean } = {},
): Polled<T> {
  const answered = useRef<{ key: string | undefined; value: boolean }>({ key, value: false });
  const [state, setState] = useState<{ key: string | undefined; value?: T; error?: ErrorPresentation }>({ key });
  if (state.key !== key) {
    setState({ key });
  }
  const latest = useLatest(state);
  const latestSame = useLatest(same);
  const { value, error } = state.key === key ? state : {};
  const [round, setRound] = useState(0);
  const windowFocus = useOptionalPlatform()?.windowFocus;
  useEffect(() => {
    if (answered.current.key !== key) answered.current = { key, value: false };
    if (pace === undefined || key === undefined) return;
    let active = true;
    const run = async () => {
      let answer: { value: T } | { error: unknown };
      try {
        answer = { value: await ask(key) };
      } catch (err) {
        answer = { error: err };
      }
      if (!active) return;
      // Even returning the previous state from an updater can invoke the hook's owner again.
      // Only publish a changed answer, so an idle poll never schedules that render.
      const current = latest.current;
      if ("value" in answer) {
        answered.current.value = true;
        if (current.key !== key || current.error !== undefined || !latestSame.current(current.value, answer.value)) setState({ key, value: answer.value });
      } else {
        const error = errorPresentation(answer.error);
        if (current.key !== key || !sameValue(failureValue(current.error), failureValue(error))) setState({ key, value: current.value, error });
      }
      return answered.current.value || !stopsOnFirstFailure;
    };
    const stop = startPolling(() => ({ pace, run }), windowFocus);
    return () => {
      active = false;
      stop();
    };
  }, [key, ask, round, pace, stopsOnFirstFailure, latest, latestSame, windowFocus]);
  return {
    value,
    error,
    retry: () => {
      setState((current) => ({ ...current, error: undefined }));
      setRound((count) => count + 1);
    },
    revise: (change) => {
      setState((current) => current.key === key ? { ...current, value: change(current.value) } : current);
      setRound((count) => count + 1);
    },
  };
}
