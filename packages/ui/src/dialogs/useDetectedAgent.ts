import { useEffect, useRef, useState } from "react";

import { useT } from "../i18n/react";
import type { Agent, Event, RequestBody } from "../protocol";

/** `idle`: nothing to ask about yet. `checking`: the answer for what is entered now is not in yet,
 * which includes the wait before it is asked. `ready`: the daemon has answered for it. `failed`:
 * the daemon refused, with `error`. */
export type DetectionStatus = "idle" | "checking" | "ready" | "failed";

/** What the form can ask the daemon about: a directory's agent, or a git remote's. */
export type DetectionQuery = Extract<RequestBody, { type: "detect_directory_agent" | "probe_git_remote" }>;

/**
 * Asks the daemon which agent the project the user is entering is set up for (`detect_directory_agent`
 * or `probe_git_remote`), once `delayMs` has passed without `query` changing, and hands the answer
 * to `onDetected` (`""` for none, the "Inherit from console" value of the form). Only the latest query's answer
 * counts: a reply for one the user has since changed, or for a dialog that has closed, is dropped.
 * `query` is `undefined` while there is nothing to ask.
 */
export function useDetectedAgent(
  request: (body: RequestBody) => Promise<Event>,
  query: DetectionQuery | undefined,
  delayMs: number,
  onDetected: (agent: Agent | "") => void,
): { status: DetectionStatus; error?: string } {
  const t = useT();
  const key = query ? JSON.stringify(query) : undefined;
  const [answered, setAnswered] = useState<{ key: string; error?: string }>();
  const latest = useRef({ request, query, onDetected, t });
  latest.current = { request, query, onDetected, t };

  useEffect(() => {
    // An answer held from an earlier run is no answer for this one, even for the same query: the
    // user may have gone elsewhere and come back while a newer request is out.
    setAnswered(undefined);
    if (key === undefined) return;
    let current = true;
    const timer = setTimeout(() => {
      const { request, query, onDetected, t } = latest.current;
      request(query!).then(
        (event) => {
          if (!current) return;
          if (event.type !== "agent_detected") {
            setAnswered({ key, error: t("dialog.project.detectFailed") });
            return;
          }
          onDetected(event.agent ?? "");
          setAnswered({ key });
        },
        (err: Error) => current && setAnswered({ key, error: err.message }),
      );
    }, delayMs);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [key, delayMs]);

  if (key === undefined) return { status: "idle" };
  if (answered?.key !== key) return { status: "checking" };
  return answered.error === undefined ? { status: "ready" } : { status: "failed", error: answered.error };
}
