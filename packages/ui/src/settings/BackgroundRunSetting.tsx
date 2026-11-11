import React from "react";

import { useT } from "../i18n/react";
import { runInBackground } from "../lifecycle/runInBackground";
import { showMenuBarIcon } from "../lifecycle/showMenuBarIcon";
import { usePlatform } from "../platform/react";
import { SwitchSettingRow } from "./SettingRow";

/** Whether closing the window keeps the app running in the background. Rendered only where the platform has the
 * capability (`AppBehaviorSection`); the choice reaches the shell through `lifecycle/shellSettings.ts`. */
export function BackgroundRunSetting(): React.ReactElement {
  const t = useT();
  const enabled = runInBackground.useValue();
  const { statusItem } = usePlatform();
  const iconShown = showMenuBarIcon.useValue();
  // The description names the menu bar icon as the way back to the window only while it is shown.
  const iconBrings = statusItem !== undefined && iconShown;
  const description = t(
    enabled
      ? iconBrings ? "settings.background.description.on" : "settings.background.description.onIconHidden"
      : iconBrings ? "settings.background.description.off" : "settings.background.description.offIconHidden",
  );
  return (
    <SwitchSettingRow searchId="run-in-background" label={t("settings.background.label")} description={description} isSelected={enabled} onChange={(value) => runInBackground.set(value)} />
  );
}
