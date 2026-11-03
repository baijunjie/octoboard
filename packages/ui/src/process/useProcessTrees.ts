import { useCallback } from "react";

import { CodedError } from "../errors/codedError";
import type { ErrorPresentation } from "../errors/errorPresentation";
import { listProcesses, type ProcessTarget } from "./processes";
import { type ProcessTree } from "../protocol";
import { CODE } from "../errors/codes";
import { useDaemon } from "../store";
import { usePolled } from "../components/hooks/usePoll";
import { pollPace, type ProcessPolling } from "./processPolling";

/** The process trees of `target`, asked for now and then at the `polling` pace while the caller is
 * mounted, and not while the document is hidden; `undefined` until the first answer. A session that has no process
 * any more has no trees. A failure is `error` for the caller to show. Before a first list succeeds, a failure stops
 * polling until `retry` explicitly asks again, and clears `error` while that answer is awaited. Later failures retain
 * the trees and keep polling. `settle` takes the trees `ids` out of the list at once and asks again, for the trees an
 * action has just stopped or kept: an answer that was already on its way is dropped, so it cannot bring them back. */
export function useProcessTrees(target: ProcessTarget | undefined, polling: ProcessPolling = "live"): {
  trees: ProcessTree[] | undefined;
  error: ErrorPresentation | undefined;
  retry: () => void;
  settle: (ids: string[]) => void;
} {
  const { request } = useDaemon();
  const key = target === undefined ? undefined : JSON.stringify(target);
  const list = useCallback(async (key: string) => {
    try {
      return await listProcesses(request, JSON.parse(key) as ProcessTarget);
    } catch (err) {
      // The session ended meanwhile: it has no process left to show, so the list reads as empty.
      if (err instanceof CodedError && err.code === CODE.session_not_running) return [];
      throw err;
    }
  }, [request]);
  const { value, error, retry, revise } = usePolled(key, list, pollPace(polling), { stopsOnFirstFailure: true });
  return {
    trees: value,
    error,
    retry,
    settle: (ids) => revise((trees) => trees?.filter((tree) => !ids.includes(tree.id))),
  };
}
