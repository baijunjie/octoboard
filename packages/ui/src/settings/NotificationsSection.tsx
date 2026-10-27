import React from "react";

import { Button } from "../components/Button";
import { InlineReference } from "../components/InlineReference";
import { ActionableError } from "../errorRecovery";
import { Message, useT } from "../i18n/react";
import { useNotificationPermission } from "../lifecycle/useNotificationPermission";
import { usePlatform } from "../platform/react";
import { reportError } from "../reportError";
import { SettingRow } from "./SettingRow";
import { useSectionRefocus } from "./useSectionRefocus";

/** Whether Octoboard may notify when a session raises its hand, with the ask where the platform
 * wants it made from a user gesture (a browser). Pressing the Enable button is that gesture. The macOS
 * app has a button to the system's notification settings instead. */
export function NotificationsSection(): React.ReactElement {
  const t = useT();
  const { notifications } = usePlatform();
  const { status, request } = useNotificationPermission();
  useSectionRefocus([status]);

  const open = async () => {
    try {
      await notifications?.openSystemSettings?.();
    } catch (cause) {
      const pane = t("settings.notifications.systemPane");
      reportError(
        new ActionableError(t("settings.notifications.openFailed", { notifications: pane }), "none", { cause }),
        undefined,
        undefined,
        <Message id="settings.notifications.openFailed" params={{ notifications: <InlineReference dir="auto" wrap>{pane}</InlineReference> }} />,
        t("settings.notifications.openFailedTitle"),
      );
    }
  };

  let state: React.ReactNode;
  if (!notifications) state = t("settings.notifications.unavailable");
  else if (status === "granted") state = t("settings.notifications.allowed");
  else if (status === "denied") state = t("settings.notifications.blocked");
  else if (status === "undecided") state = t("settings.notifications.undecided");
  // The macOS app: the notification plugin reports "granted" there without ever prompting and the
  // app keeps no switch of its own, so the only switch is the system's, which the button opens.
  else state = <Message id="settings.notifications.managedBySystem" />;

  return (
    <SettingRow label={t("settings.notifications.label")} description={state}>
      {request && status === "undecided" && (
        <Button size="sm" variant="primary" preventFocusOnPress onPress={() => void request()}>
          {t("settings.notifications.enable")}
        </Button>
      )}
      {notifications?.openSystemSettings && status === undefined && (
        <Button size="sm" variant="secondary" preventFocusOnPress onPress={() => void open()}>
          {t("settings.notifications.openSettings")}
        </Button>
      )}
    </SettingRow>
  );
}
