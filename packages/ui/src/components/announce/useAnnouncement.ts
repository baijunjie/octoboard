import { useEffect, useState } from "react";

/** The announcement of a failure shown on screen, shared by the alert and the empty-state panel
 * that carry one (`ErrorAlert` documents the parameters). `role` is `"alert"` once the failure is to
 * be spoken, and `key` remounts the announcing part when the attempt changes or the delay ends, so
 * a failure that repeats in the same words is spoken again. */
export function useAnnouncement(announce: boolean, announceKey: number | undefined, announceDelayMs: number | undefined): { role: "alert" | undefined; key: string } {
  const [announced] = useState(announce);
  const [waited, setWaited] = useState(announceDelayMs === undefined);
  useEffect(() => {
    if (waited) return;
    const timer = setTimeout(() => setWaited(true), announceDelayMs);
    return () => clearTimeout(timer);
  }, []);
  return { role: announced && waited ? "alert" : undefined, key: `${announceKey}:${waited}` };
}
