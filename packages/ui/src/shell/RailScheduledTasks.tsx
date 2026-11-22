import { Clock } from "lucide-react";
import React from "react";

import { useT } from "../i18n/react";
import { useEnabledTaskCount } from "../scheduledTasks/useEnabledTaskCount";
import { ChromeButton } from "./ChromeButton";
import { RailCountBadge } from "./RailCountBadge";

/**
 * The rail's Scheduled tasks button, in the group under the consoles, directly above the Remote devices button where
 * the window has one: a press shows the Scheduled tasks page in place of the session UI, and a press while the page
 * shows does nothing, since the page is left by going to another page (a console's avatar, Back). While the page shows
 * the button is drawn with the current fills and reports `aria-current`, and no console's avatar is drawn as current. Every
 * window of a daemon has it, while that daemon can be used. A badge counts the tasks enabled (`useEnabledTaskCount`), so
 * the rail tells at a glance how many scheduled tasks are set to run; there is none at zero or while the tasks are not
 * listed.
 */
export function RailScheduledTasks({ shown, onPress }: { shown: boolean; onPress: () => void }): React.ReactElement {
  const t = useT();
  const enabled = useEnabledTaskCount() ?? 0;
  return (
    <ChromeButton
      label={enabled > 0 ? t("rail.scheduledTasks.count", { count: enabled }) : t("scheduledTasks.label")}
      tooltipPlacement="end"
      current={shown}
      onPress={onPress}
      badge={enabled > 0 ? <RailCountBadge count={enabled} overflowLabel={t("rail.countBadgeOverflow")} color="success" /> : undefined}
    >
      <Clock className="size-4" />
    </ChromeButton>
  );
}
