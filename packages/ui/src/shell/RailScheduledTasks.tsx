import { Clock } from "lucide-react";
import React from "react";

import { useT } from "../i18n/react";
import { useRunningTaskCount } from "../scheduledTasks/useRunningTaskCount";
import { ChromeButton } from "./ChromeButton";
import { RailCountBadge } from "./RailCountBadge";

/**
 * The rail's Scheduled tasks button, in the group under the consoles, directly above the Remote devices button where
 * the window has one: a press shows the Scheduled tasks page in place of the session UI, and a press while the page
 * shows does nothing, since the page is left by going to another page (a console's avatar, Back). While the page shows
 * the button is drawn as the current console's tile is and reports `aria-current`, and no console's tile is. Every
 * window of a daemon has it, while that daemon can be used. A badge counts the tasks running (`useRunningTaskCount`), so
 * the rail tells at a glance that a scheduled task is at work; there is none at zero or while the tasks are not listed.
 */
export function RailScheduledTasks({ shown, onPress }: { shown: boolean; onPress: () => void }): React.ReactElement {
  const t = useT();
  const running = useRunningTaskCount() ?? 0;
  return (
    <ChromeButton
      label={running > 0 ? t("rail.scheduledTasks.count", { count: running }) : t("scheduledTasks.label")}
      tooltipPlacement="end"
      current={shown}
      onPress={onPress}
      badge={running > 0 ? <RailCountBadge count={running} overflowLabel={t("rail.countBadgeOverflow")} color="success" /> : undefined}
    >
      <Clock className="size-4" />
    </ChromeButton>
  );
}
