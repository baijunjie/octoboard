import { Bell } from "lucide-react";
import React from "react";

import { useT } from "../i18n/react";
import { useNotificationPermission } from "../lifecycle/useNotificationPermission";
import { ChromeButton } from "./ChromeButton";
import { useFocusHandoff } from "../components/focus/useFocusHandoff";
import { Badge } from "../primitives";

/** The invitation to turn notifications on, where the platform wants the ask to come from a user
 * gesture (a browser) and the answer is still undecided; gone once the answer is given, whichever
 * it is. The dot only draws the eye, the label carries the meaning. The Settings dialog's
 * Notifications section is the other place to see and change the answer.
 *
 * Pressed from the keyboard, the bell holds focus until the answer removes it, and focus would then
 * fall to `<body>`; so once the answer is in, focus that was lost is handed on (`useFocusHandoff`).
 * Not before the answer: while the browser's own prompt is up, keys must not reach the agent. */
export function NotificationsBell({ focusTerminal }: { focusTerminal: () => void }): React.ReactElement | null {
  const t = useT();
  const { status, request } = useNotificationPermission();
  const held = useFocusHandoff(focusTerminal);
  if (!request || status !== "undecided") return null;
  return (
    <span className="contents" {...held}>
      <ChromeButton label={t("rail.notifications.enable")} onPress={() => void request()} tooltipPlacement="end"
        badge={<Badge color="accent" />}>
        <Bell aria-hidden="true" className="size-4" />
      </ChromeButton>
    </span>
  );
}
