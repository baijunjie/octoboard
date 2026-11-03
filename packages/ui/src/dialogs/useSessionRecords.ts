import { useEffect } from "react";

import { useLatest } from "../components/hooks/useLatest";
import { useLatestRequest } from "../components/hooks/useLatestRequest";
import type { SessionRecordsPresence } from "../protocol";
import { useDaemon } from "../store";

/** Checks the native records of the whole deletion scope for the confirmation's cleanup option. */
export function useSessionRecords(sessionIds: string[]): SessionRecordsPresence {
  const { request } = useDaemon();
  const key = JSON.stringify([...sessionIds].sort());
  const { value, send, reconnects } = useLatestRequest<SessionRecordsPresence>({
    scope: key, initial: "unknown",
  });
  const latest = useLatest({ request, send });
  useEffect(() => {
    const { request, send } = latest.current;
    void send({
      request: () => request({ type: "inspect_session_records", sessions: JSON.parse(key) as string[] }),
      accept: (result) => () => result.presence,
      // An unavailable inspection must leave the cleanup option available.
      failed: () => "unknown",
    });
  }, [key, reconnects]);
  return value;
}
