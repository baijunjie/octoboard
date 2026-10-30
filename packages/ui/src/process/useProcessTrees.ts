import { useEffect, useRef, useState } from "react";

import { useLatest } from "../components/useLatest";
import { DaemonRequestError } from "../daemon-client";
import { errorPresentation, type ErrorPresentation } from "../errorPresentation";
import { unexpectedFailureDiagnostic } from "../errorText";
import { listProcesses, type ProcessTarget } from "../processes";
import { SESSION_NOT_RUNNING, type ProcessTree } from "../protocol";
import { sameValue } from "../sameValue";
import { useDaemon, type Daemon } from "../store";

/** How long the list waits after an answer before it asks again: `live` while the panel is on screen,
 * `background` while it only has to be known (a phone-width pane, where the panel floats in on
 * demand). The daemon reads each tree's ports when it is asked, so the list is polled at a modest
 * pace rather than followed. */
const POLL_MS = { live: 3000, background: 15000 };

/** How often the list is asked for: not at all, now and then, or as the panel is watched. */
export type ProcessPolling = "off" | "background" | "live";

/** Error messages and stacks are not enumerable; compare the data the panel can show explicitly. */
function failureValue(error: ErrorPresentation | undefined) {
  return typeof error === "object" ? { code: error.code, params: error.params, message: error.message, diagnostic: unexpectedFailureDiagnostic(error) } : error;
}

/** One list request: the trees, or the failure. A session that has no process any more has no trees. */
async function listOnce(request: Daemon["request"], target: ProcessTarget): Promise<{ trees: ProcessTree[] } | { error: unknown }> {
  try {
    return { trees: await listProcesses(request, target) };
  } catch (err) {
    return err instanceof DaemonRequestError && err.code === SESSION_NOT_RUNNING ? { trees: [] } : { error: err };
  }
}

/** The process trees of `target`, asked for now and then at the `polling` pace while the caller is
 * mounted, and not while the document is hidden; `undefined` until the first answer. A failure is
 * `error` for the caller to show. Before a first list succeeds, a failure stops polling until `retry`
 * explicitly asks again, and clears `error` while that answer is awaited. Later failures retain the trees and
 * keep polling. `settle` takes the trees `ids` out of the list at once and asks again, for the trees an action
 * has just stopped or kept: an answer that was already on its way is dropped, so it cannot bring them back. */
export function useProcessTrees(target: ProcessTarget | undefined, polling: ProcessPolling = "live"): {
  trees: ProcessTree[] | undefined;
  error: ErrorPresentation | undefined;
  retry: () => void;
  settle: (ids: string[]) => void;
} {
  const { request } = useDaemon();
  const key = target === undefined ? undefined : JSON.stringify(target);
  const hasList = useRef<{ key: string | undefined; value: boolean }>({ key, value: false });
  const [state, setState] = useState<{ key: string | undefined; trees?: ProcessTree[]; error?: ErrorPresentation }>({ key });
  if (state.key !== key) {
    setState({ key });
  }
  const latest = useLatest(state);
  const { trees, error } = state.key === key ? state : {};
  const [round, setRound] = useState(0);
  useEffect(() => {
    if (hasList.current.key !== key) hasList.current = { key, value: false };
    if (polling === "off" || key === undefined) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    // A poll that falls due while the document is hidden waits for it to be shown again.
    let waiting = false;
    const poll = async () => {
      const answer = await listOnce(request, JSON.parse(key) as ProcessTarget);
      if (!active) return;
      // Even returning the previous state from an updater can invoke the hook's owner again.
      // Only publish a changed answer, so an idle poll never schedules that render.
      const current = latest.current;
      if ("trees" in answer) {
        hasList.current.value = true;
        if (current.key !== key || current.error !== undefined || !sameValue(current.trees, answer.trees)) setState({ key, trees: answer.trees });
      } else {
        const error = errorPresentation(answer.error);
        if (current.key !== key || !sameValue(failureValue(current.error), failureValue(error))) setState({ key, trees: current.trees, error });
      }
      if (hasList.current.value) timer = setTimeout(due, POLL_MS[polling]);
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
  }, [key, request, round, polling, latest]);
  return {
    trees,
    error,
    retry: () => {
      setState((current) => ({ ...current, error: undefined }));
      setRound((value) => value + 1);
    },
    settle: (ids) => {
      setState((current) => current.key === key ? { ...current, trees: current.trees?.filter((tree) => !ids.includes(tree.id)) } : current);
      setRound((value) => value + 1);
    },
  };
}
