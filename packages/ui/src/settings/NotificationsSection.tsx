import { Button } from "@heroui/react";
import React from "react";

import { useT } from "../i18n/react";
import { useNotificationPermission } from "../lifecycle/useNotificationPermission";
import { usePlatform } from "../platform/react";
import { SettingRow } from "./SettingRow";
import { useSectionRefocus } from "./useSectionRefocus";

/** Whether Octoboard may notify when a session raises its hand, with the ask where the platform
 * wants it made from a user gesture (a browser). Pressing the button is that gesture. */
export function NotificationsSection(): React.ReactElement {
  const t = useT();
  const { notifications } = usePlatform();
  const { status, request } = useNotificationPermission();
  useSectionRefocus([status]);

  let state: string;
  if (!notifications) state = t("settings.notifications.unavailable");
  else if (status === "granted") state = t("settings.notifications.allowed");
  else if (status === "denied") state = t("settings.notifications.blocked");
  else if (status === "undecided") state = t("settings.notifications.undecided");
  // The macOS app: the notification plugin reports "granted" there without ever prompting, so the
  // only switch is the system's own.
  else state = t("settings.notifications.system");

  return (
    <SettingRow label={t("settings.notifications.label")} description={state}>
      {request && status === "undecided" && (
        <Button size="sm" variant="primary" preventFocusOnPress onPress={() => void request()}>
          {t("settings.notifications.enable")}
        </Button>
      )}
    </SettingRow>
  );
}
