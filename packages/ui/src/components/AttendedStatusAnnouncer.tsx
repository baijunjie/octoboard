import React, { useEffect, useState } from "react";

import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "./StatusAnnouncer";
import { useWindowAttended } from "./useWindowAttended";

/**
 * A `StatusAnnouncer` for a status that can arrive while the user is out of the window, or as focus moves onto the
 * surface it belongs to (see "Announcing a change" in `docs/ui-accessibility.md`). A status that is there from the
 * region's first render, or written within about 300 ms of a focus move, is not said; and on the way back into the
 * window VoiceOver speaks the control focus is on, so a status written meanwhile is not said either. So every text,
 * the first included, is written `AFTER_FOCUS_MOVE_DELAY_MS` of attended time after it arrived, the wait started over
 * each time the user comes back; a text that is replaced while it waits is never said.
 */
export function AttendedStatusAnnouncer({ text }: { text: string | undefined }): React.ReactElement {
  const attended = useWindowAttended();
  const [said, setSaid] = useState<string>();
  useEffect(() => {
    if (text === said || !attended) return;
    const timer = setTimeout(() => setSaid(text), AFTER_FOCUS_MOVE_DELAY_MS);
    return () => clearTimeout(timer);
  }, [text, said, attended]);
  return <StatusAnnouncer text={said} />;
}
