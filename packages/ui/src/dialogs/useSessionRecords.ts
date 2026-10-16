import { useEffect, useState } from "react";

import { useStableHandler } from "../components/useLatest";
import type { SessionRecordsPresence } from "../protocol";
import { useDaemon } from "../store";

/** Checks the files for the whole deletion scope. An unused session is deleted without opening a
 * confirmation only after the daemon proves absence and accepts its guarded deletion request. */
export function useSessionRecords(
  sessionIds: string[],
  unusedSession: string | undefined,
  onDeleted: () => void,
  { enabled = true, revision = "" }: { enabled?: boolean; revision?: string } = {},
): {
  pending: boolean;
  presence: SessionRecordsPresence;
} {
  const { request } = useDaemon();
  const deleted = useStableHandler(onDeleted);
  const key = JSON.stringify([...sessionIds].sort());
  const checkKey = JSON.stringify([key, unusedSession, enabled, revision]);
  const [checked, setChecked] = useState<{ key: string; presence: SessionRecordsPresence }>();
  useEffect(() => {
    if (!enabled) return;
    let active = true;
    const inspect = async (): Promise<SessionRecordsPresence> => {
      try {
        const result = await request({ type: "inspect_session_records", sessions: JSON.parse(key) as string[] });
        return result.type === "session_records_inspected" ? result.presence : "unknown";
      } catch {
        return "unknown";
      }
    };
    void (async () => {
      let presence = await inspect();
      if (!active) return;
      if (unusedSession && presence === "absent") {
        try {
          await request({ type: "delete_session", session: unusedSession, only_if_unused_without_agent_records: true });
          if (active) deleted();
          return;
        } catch {
          // The daemon rechecks after inspection: a new turn or file requires confirmation.
          presence = await inspect();
        }
      }
      if (active) setChecked({ key: checkKey, presence });
    })();
    return () => { active = false; };
  }, [key, checkKey, unusedSession, enabled, request, deleted]);
  return { pending: checked?.key !== checkKey, presence: checked?.key === checkKey ? checked.presence : "unknown" };
}
