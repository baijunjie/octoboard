import { useCallback, useEffect, useRef, useState } from "react";

import type { HistoryMessage } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";

export type SessionHistory =
  | { state: "loading" }
  /** The request itself failed (the connection dropped, the daemon refused): worth trying again. */
  | { state: "failed" }
  /** The daemon answered that the agent's record is not found or cannot be read. */
  | { state: "unavailable" }
  | { state: "loaded"; messages: HistoryMessage[] };

/** Reads what an archived session's conversation record says was said, once per session shown.
 * Nothing is cached: the record of an archived session does not change, and a reopened one leaves
 * the view altogether. A failed request is read again by `reload`, and by itself when the daemon
 * connection delivers a fresh snapshot, since a dropped connection is the usual cause. */
export function useSessionHistory(sessionId: string): { history: SessionHistory; reload: () => void } {
  const { request } = useDaemon();
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  const [attempt, setAttempt] = useState(0);
  const [read, setRead] = useState<{ key: string; history: SessionHistory }>();
  const key = `${sessionId}\0${attempt}`;
  const failed = useRef(false);
  useEffect(() => {
    let active = true;
    void (async () => {
      let history: SessionHistory = { state: "failed" };
      try {
        const reply = await request({ type: "read_session_history", session: sessionId });
        if (reply.type === "session_history") history = reply.unavailable ? { state: "unavailable" } : { state: "loaded", messages: reply.messages };
      } catch {
        // Shown as a failure with a way to try again.
      }
      if (!active) return;
      failed.current = history.state === "failed";
      setRead({ key, history });
    })();
    return () => { active = false; };
  }, [key, sessionId, request]);
  useEffect(() => {
    if (failed.current) setAttempt((n) => n + 1);
    // Keyed on the epoch alone: a fresh snapshot is the chance to recover, not every render.
  }, [snapshotEpoch]);
  const reload = useCallback(() => setAttempt((n) => n + 1), []);
  return { history: read?.key === key ? read.history : { state: "loading" }, reload };
}
