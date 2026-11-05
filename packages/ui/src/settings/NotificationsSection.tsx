import React from "react";

import { InlineReference } from "../i18n/InlineReference";
import { ActionableError } from "../errors/actionableError";
import { Message, useT } from "../i18n/react";
import { useNotificationPermission } from "../lifecycle/useNotificationPermission";
import { usePlatform } from "../platform/react";
import { SettingRow } from "./SettingRow";
import { useSectionRefocus } from "./useSectionRefocus";
import { raiseError } from "../components/alerts/raiseError";
import { Button } from "../primitives";

/** Whether Octoboard may notify when a session raises its hand, with the ask where the platform
 * wants it made from a user gesture (a browser). Pressing the Enable button is that gesture. Where the system
 * has a pane for the app's notifications (the desktop app, `notificationSettings`) there is a button to it instead. */
export function NotificationsSection(): React.ReactElement {
  const t = useT();
  const { notifications, host } = usePlatform();
  const { status, request } = useNotificationPermission();
  useSectionRefocus([status]);

  // A promise chain rather than `try`/`catch`, which the React Compiler leaves a function with uncompiled; started inside
  // it so a synchronous throw from the platform reaches the same `catch`.
  const open = () => Promise.resolve().then(() => notifications?.openSystemSettings?.()).catch((cause: unknown) => {
    const pane = t("settings.notifications.systemPane");
    raiseError(new ActionableError(t("settings.notifications.openFailed", { notifications: pane }), "none", { cause }), { title: t("settings.notifications.openFailedTitle"), content: <Message id="settings.notifications.openFailed" params={{ notifications: <InlineReference dir="auto" wrap>{pane}</InlineReference> }} /> });
  });

  let state: React.ReactNode;
  if (!notifications) state = t("settings.notifications.unavailable");
  else if (status === "granted") state = t("settings.notifications.allowed");
  else if (status === "denied") state = t("settings.notifications.blocked");
  else if (status === "undecided") state = t("settings.notifications.undecided");
  // Where the system has a pane of its own for the app's notifications (the desktop app): the notification plugin
  // reports "granted" there without ever prompting and the app keeps no switch of its own, so the only switch is
  // the system's, which the button opens. Elsewhere nothing is asked or managed and notifications are simply on.
  else if (host.notificationSettings === "system-pane") state = <Message id="settings.notifications.managedBySystem" />;
  else state = t("settings.notifications.allowed");

  return (
    <SettingRow compact label={t("settings.notifications.label")} description={state}>
      {request && status === "undecided" && (
        <Button size="sm" variant="primary" focusOnPress={false} onPress={() => void request()}>
          {t("settings.notifications.enable")}
        </Button>
      )}
      {notifications?.openSystemSettings && status === undefined && (
        <Button size="sm" variant="secondary" focusOnPress={false} onPress={() => void open()}>
          {t("settings.notifications.openSettings")}
        </Button>
      )}
    </SettingRow>
  );
}
