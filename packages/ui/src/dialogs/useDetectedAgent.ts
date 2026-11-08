import { useEffect } from "react";

import { useStableHandler } from "../components/hooks/useLatest";
import { useLatestRequest } from "../components/hooks/useLatestRequest";
import type { ErrorPresentation } from "../errors/errorPresentation";
import type { Agent, ClientRequestBody } from "../protocol";
import type { RequestSender } from "../requests";
import { shownError } from "./shownError";

/** `idle`: nothing to ask about yet. `checking`: the answer for what is entered now is not in yet,
 * which includes the wait before it is asked. `ready`: the daemon has answered for it. `failed`:
 * the daemon refused, with `error`. */
export type DetectionStatus = "idle" | "checking" | "ready" | "failed";

/** The state of the check, with the daemon's refusal when it is `failed`, whether a retry of it is out, and how many
 * retries have answered with a refusal since the first (`attempt`, for an alert that announces a failure repeated in
 * the same words again). */
export type Detection =
  | { status: "idle" | "checking" }
  | { status: "ready"; directoryExists?: boolean }
  | { status: "failed"; error: ErrorPresentation; retrying?: boolean; attempt?: number };

/** What the form can ask the daemon about: a directory's agent, or a git remote's. */
export type DetectionQuery = Extract<ClientRequestBody, { type: "detect_directory_agent" | "probe_git_remote" }>;

const CHECKING: Detection = { status: "checking" };

/**
 * Asks the daemon which agent the project the user is entering is set up for (`detect_directory_agent`
 * or `probe_git_remote`), once `delayMs` has passed without `query` changing, and hands the answer
 * to `onDetected` (`""` for none, the "Same as console" value of the form). Only the latest query's answer
 * counts: a reply for one the user has since changed, or for a dialog that has closed, is dropped.
 * `query` is `undefined` while there is nothing to ask.
 *
 * Returns the check's state and `retry`, which asks a failed git remote check again at once, as a person's request
 * (`asked_by_person`), which looks for a `git` installed since the daemon last looked; the failure stays held, marked
 * `retrying`, until the answer replaces it (a refusal again counts in `attempt`). The checks made as the user types
 * never do, so they stay on the cached look.
 */
export function useDetectedAgent(
  request: RequestSender,
  query: DetectionQuery | undefined,
  delayMs: number,
  onDetected: (agent: Agent | "") => void,
): { detection: Detection; retry: () => void } {
  const key = query ? JSON.stringify(query) : undefined;
  const { value, send, reset, hold, asked, reconnects } = useLatestRequest<Detection>({
    scope: key, initial: CHECKING,
  });
  const ask = useStableHandler((query: DetectionQuery) => {
    void send({
      request: () => request(query),
      accept: (event) => {
        onDetected(event.agent ?? "");
        return () => ({ status: "ready", directoryExists: event.directory_exists });
      },
      failed: (failure, held, t) => ({
        status: "failed",
        error: shownError(failure, t),
        attempt: held.status === "failed" ? (held.attempt ?? 0) + 1 : 0,
      }),
    });
  });
  const retry = () => {
    if (query?.type !== "probe_git_remote") return;
    hold((held) => held.status === "failed" ? { ...held, retrying: true } : held);
    ask({ ...query, asked_by_person: true });
  };

  useEffect(() => {
    // With nothing to ask, what was answered before is dropped, so coming back to the same entry starts checking again.
    if (key === undefined) {
      if (asked()) reset();
      return;
    }
    // Whatever was asked or held before this run is no answer for it, even for the same query: the user may have
    // gone elsewhere and come back within the delay, with a request for it still out.
    reset();
    hold(() => CHECKING, key);
    const timer = setTimeout(() => ask(query!), delayMs);
    return () => clearTimeout(timer);
  }, [key, delayMs, reconnects]);

  return { detection: key === undefined ? { status: "idle" } : value, retry };
}
