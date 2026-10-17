import { useEffect, useState } from "react";

import type { SessionRecordsPresence } from "../protocol";
import { useDaemon } from "../store";

/** Checks the native records of the whole deletion scope for the confirmation's cleanup option. */
export function useSessionRecords(sessionIds: string[]): SessionRecordsPresence {
  const { request } = useDaemon();
  const key = JSON.stringify([...sessionIds].sort());
  const [checked, setChecked] = useState<{ key: string; presence: SessionRecordsPresence }>();
  useEffect(() => {
    let active = true;
    void (async () => {
      let presence: SessionRecordsPresence = "unknown";
      try {
        const result = await request({ type: "inspect_session_records", sessions: JSON.parse(key) as string[] });
        if (result.type === "session_records_inspected") presence = result.presence;
      } catch {
        // An unavailable inspection must leave the cleanup option available.
      }
      if (active) setChecked({ key, presence });
    })();
    return () => { active = false; };
  }, [key, request]);
  return checked?.key === key ? checked.presence : "unknown";
}
