import { useEffect, useState, useSyncExternalStore } from "react";

import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { useWindowAttended } from "../components/announce/useWindowAttended";
import { useAccessibilityHidden } from "../components/focus/useAccessibilityHidden";
import { errorAlerts } from "../errorAlerts";

/** How long the result of a batch of account switches or of a model change stays in the status region once it can be
 * heard, after the focus-move wait that `AttendedStatusAnnouncer` adds. */
const ANNOUNCEMENT_MS = 10_000;

/**
 * What the window says once a batch of account switches or a model change is done, for assistive technology.
 * `covered` is whether a modal of the window's own (a dialog, a question, Settings) is open. The caller wraps the
 * status region in an element given `announcementRef`, and gives it `announcementText`, which is undefined while the
 * announcement has to wait.
 */
export function useAnnouncement({ covered }: { covered: boolean }) {
  const [announcement, setAnnouncement] = useState<string>();
  // An announcement waits while a modal covers the status region (it hides what lies outside it, and closing it moves
  // focus) or the window is not attended, and counts its lifetime only once it can be heard.
  const attended = useWindowAttended();
  const alertOpen = useSyncExternalStore(errorAlerts.subscribe, errorAlerts.getSnapshot).length > 0;
  const { ref: announcementRef, hidden: announcementCovered } = useAccessibilityHidden();
  const announcementHidden = covered || alertOpen || announcementCovered;
  useEffect(() => {
    if (announcement === undefined || announcementHidden || !attended) return;
    const timer = setTimeout(() => setAnnouncement(undefined), AFTER_FOCUS_MOVE_DELAY_MS + ANNOUNCEMENT_MS);
    return () => clearTimeout(timer);
  }, [announcement, announcementHidden, attended]);

  /** Hands `text` to the window's status region, replacing one that has not been said yet. The region says it
   * once the window is attended and no modal covers it, a focus-move wait after that (`AttendedStatusAnnouncer`),
   * so a result that arrives as a dialog closes is not lost to the focus move; it is emptied again later, so that an
   * identical result of a later change is written afresh. */
  const announce = (text: string): void => setAnnouncement(text);

  const announcementText = announcementHidden ? undefined : announcement;
  return { announce, announcementRef, announcementText };
}
