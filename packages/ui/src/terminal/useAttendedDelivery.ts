import { useEffect, useState } from "react";

import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { useWindowAttended } from "../components/announce/useWindowAttended";
import { useLatest } from "../components/hooks/useLatest";

/**
 * `sentence` for a terminal view's `StatusAnnouncer` once it is due, for an announcement queued under `key` (the
 * subject it is about) and dropped as `key` goes `undefined`. It is due after `AFTER_FOCUS_MOVE_DELAY_MS` of attended
 * time (`useWindowAttended`), the wait started over each time the user comes back to the window, as "Announcing a
 * change" in `docs/ui-standard.md` sets out for a status that can arrive while the user is away or with a focus move.
 *
 * A queued announcement that is hidden, switched to another subject or resolved before it is due is never said: the
 * caller drops or changes `key`, which clears the sentence at once, so a switch to another subject with the same
 * sentence empties the region before writing it again. `stillDue` is read as the wait ends; when it says no, the
 * announcement counts as delivered without being written (a focused region that already reads the same copy), and is
 * not queued again for that `key`.
 */
export function useAttendedDelivery(
  key: string | undefined,
  sentence: string,
  { stillDue }: { stillDue?: () => boolean } = {},
): string | undefined {
  const attended = useWindowAttended();
  const stillDueRef = useLatest(stillDue);
  const [settled, setSettled] = useState<{ key: string; said: boolean }>();
  // A key that went away and came back is a new announcement, so what was settled for it is forgotten at once.
  const [seen, setSeen] = useState(key);
  if (seen !== key) {
    setSeen(key);
    setSettled(undefined);
  }
  useEffect(() => {
    if (key === undefined || settled?.key === key || !attended) return;
    const timer = setTimeout(() => setSettled({ key, said: stillDueRef.current?.() ?? true }), AFTER_FOCUS_MOVE_DELAY_MS);
    return () => clearTimeout(timer);
  }, [key, settled, attended]);
  return key !== undefined && settled?.key === key && settled.said ? sentence : undefined;
}
