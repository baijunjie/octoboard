import React from "react";

import { useT } from "../i18n/react";
import { runInBackground } from "../lifecycle/runInBackground";
import { showMenuBarIcon } from "../lifecycle/showMenuBarIcon";
import { SwitchSettingRow } from "./SettingRow";

/** Whether the menu bar icon (the notification area icon on Windows) is shown. Rendered only where the platform has
 * the icon (`AppBehaviorSection`); the choice reaches the shell through `lifecycle/shellSettings.ts`. With the icon hidden
 * while the app keeps running in the background, the description says how the window comes back instead. */
export function MenuBarIconSetting(): React.ReactElement {
  const t = useT();
  const shown = showMenuBarIcon.useValue();
  const inBackground = runInBackground.useValue();
  const description = t(shown ? "settings.menuBarIcon.description.on" : inBackground ? "settings.menuBarIcon.description.offInBackground" : "settings.menuBarIcon.description.off");
  return <SwitchSettingRow label={t("settings.menuBarIcon.label")} description={description} isSelected={shown} onChange={(value) => showMenuBarIcon.set(value)} />;
}
