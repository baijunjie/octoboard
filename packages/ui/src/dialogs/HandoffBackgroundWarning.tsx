import React from "react";

import { ErrorAlert } from "../components/alerts/ErrorAlert";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { useHasPainted } from "../components/motion/useHasPainted";
import { useT } from "../i18n/react";

/** The warning of a handoff for the work the original left running in the background (`Session.background_tasks`):
 * archiving the original ends its process, and what the agent runs in the background may end with it, while the new
 * session is told of the tasks. `count` is how many there are; nothing is shown for none. The tasks themselves (raw
 * commands, kinds, ids) are for the new session, in the handoff package, and are not listed here. A warning about the
 * whole dialog, so an Alert at the top of its body. */
export function HandoffBackgroundWarning({ count }: { count: number }): React.ReactElement | null {
  const t = useT();
  const painted = useHasPainted();
  if (count === 0) return null;
  return (
    <ErrorAlert status="warning" announce={painted} announceDelayMs={AFTER_FOCUS_MOVE_DELAY_MS} title={t("dialog.handoff.backgroundTitle")}>
      {t("dialog.handoff.background", { count })}
    </ErrorAlert>
  );
}
