import React from "react";

import { useT } from "../i18n/react";
import { runInBackground } from "../lifecycle/runInBackground";
import { SwitchSettingRow } from "./SettingRow";

/** Whether closing the window keeps the app running in the background. Rendered only where the
 * platform has the capability (`GeneralSection`); the choice reaches the shell through
 * `useBackgroundRunSync`. */
export function BackgroundRunSetting(): React.ReactElement {
  const t = useT();
  const enabled = runInBackground.useValue();
  const description = t(enabled ? "settings.background.description.on" : "settings.background.description.off");
  return (
    <SwitchSettingRow label={t("settings.background.label")} description={description} isSelected={enabled} onChange={(value) => runInBackground.set(value)} />
  );
}
