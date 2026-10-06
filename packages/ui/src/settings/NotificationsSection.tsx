import { Button } from "@heroui/react";
import React from "react";

import { useNotificationPermission } from "../lifecycle/useNotificationPermission";
import { usePlatform } from "../platform/react";
import { SettingRow } from "./SettingRow";
import { useSectionRefocus } from "./useSectionRefocus";

/** Whether Octoboard may notify when a session raises its hand, with the ask where the platform
 * wants it made from a user gesture (a browser). Pressing the button is that gesture. */
export function NotificationsSection(): React.ReactElement {
  const { notifications } = usePlatform();
  const { status, request } = useNotificationPermission();
  useSectionRefocus([status]);

  let state: string;
  if (!notifications) state = "Notifications are not available here.";
  else if (status === "granted") state = "Allowed.";
  else if (status === "denied") state = "Blocked. Allow them in this site's settings in your browser.";
  else if (status === "undecided") state = "Not enabled yet.";
  // The macOS app: the notification plugin reports "granted" there without ever prompting, so the
  // only switch is the system's own.
  else state = "Octoboard posts them directly. Turn them off in the system's notification settings.";

  return (
    <SettingRow label="Notify when a session is waiting for you" description={state}>
      {request && status === "undecided" && (
        <Button size="sm" variant="primary" preventFocusOnPress onPress={() => void request()}>
          Enable
        </Button>
      )}
    </SettingRow>
  );
}
