import { useEffect, useRef, useState } from "react";

import { useLatest } from "../components/hooks/useLatest";
import { errorPresentation, type ErrorPresentation } from "../errors/errorPresentation";
import { unexpectedFailureDiagnostic } from "../errors/errorText";
import { sameValue } from "../store";

/** How long a poll waits after an answer before it asks again: `live` while the answer is on screen,
 * `background` while it only has to be known (a phone-width pane, where the panel floats in on demand, or a count
 * on a button). The daemon reads the process table (and, for a list, each tree's ports) when it is asked, so the
 * answer is polled at a modest pace rather than followed. */
export const POLL_MS = { live: 3000, background: 15000 };

/** How often a poll asks: not at all, now and then, or as the answer is watched. */
export type ProcessPolling = "off" | "background" | "live";

/** Error messages and stacks are not enumerable; compare the data a caller can show explicitly. */
function failureValue(error: ErrorPresentation | undefined) {
  return typeof error === "object" ? { code: error.code, params: error.params, message: error.message, diagnostic: unexpectedFailureDiagnostic(error) } : error;
}

/** What `usePolled` answers for the caller's `key`; `revise` and `retry` act on that answer. */
interface Polled<T> {
  value: T | undefined;
  error: ErrorPresentation | undefined;
  /** Clears the error and asks again now. */
  retry: () => void;
  /** Replaces the answer with `change` of it at once and asks again now: an answer that was already on its way is
   * dropped, so it cannot undo the change. */
  revise: (change: (value: T | undefined) => T | undefined) => void;
}

/** What `ask` answers for `key`, asked for now and then at the `pace` while the caller is mounted, and not while the
 * document is hidden; `undefined` until the first answer, and again when `key` changes. `key` is `undefined` for
 * nothing to ask. A failure is `error` for the caller to show, and a later failure keeps the last answer. Only a
 * changed answer or failure is published, so an idle poll never renders its owner again. With `stopsOnFirstFailure`,
 * a failure before the first answer stops polling until `retry` asks again. `ask` must keep its identity while it
 * asks for the same thing. */
export function usePolled<T>(key: string | undefined, ask: (key: string) => Promise<T>, pace: ProcessPolling, { stopsOnFirstFailure = false }: { stopsOnFirstFailure?: boolean } = {}): Polled<T> {
  const answered = useRef<{ key: string | undefined; value: boolean }>({ key, value: false });
  const [state, setState] = useState<{ key: string | undefined; value?: T; error?: ErrorPresentation }>({ key });
  if (state.key !== key) {
    setState({ key });
  }
  const latest = useLatest(state);
  const { value, error } = state.key === key ? state : {};
  const [round, setRound] = useState(0);
  useEffect(() => {
    if (answered.current.key !== key) answered.current = { key, value: false };
    if (pace === "off" || key === undefined) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    // A poll that falls due while the document is hidden waits for it to be shown again.
    let waiting = false;
    const poll = async () => {
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
        if (current.key !== key || current.error !== undefined || !sameValue(current.value, answer.value)) setState({ key, value: answer.value });
      } else {
        const error = errorPresentation(answer.error);
        if (current.key !== key || !sameValue(failureValue(current.error), failureValue(error))) setState({ key, value: current.value, error });
      }
      if (answered.current.value || !stopsOnFirstFailure) timer = setTimeout(due, POLL_MS[pace]);
    };
    const due = () => {
      if (document.hidden) waiting = true;
      else void poll();
    };
    const shown = () => {
      if (!waiting || document.hidden) return;
      waiting = false;
      void poll();
    };
    document.addEventListener("visibilitychange", shown);
    void poll();
    return () => {
      active = false;
      clearTimeout(timer);
      document.removeEventListener("visibilitychange", shown);
    };
  }, [key, ask, round, pace, stopsOnFirstFailure, latest]);
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
