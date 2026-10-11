import { Switch } from "@heroui/react";
import React from "react";

import { useT } from "../i18n/react";
import { runInBackground } from "../lifecycle/runInBackground";
import { SettingRow } from "./SettingRow";

/** Whether closing the window keeps the app running in the background. Rendered only where the
 * platform has the capability (`GeneralSection`); the choice reaches the shell through
 * `useBackgroundRunSync`. */
export function BackgroundRunSetting(): React.ReactElement {
  const t = useT();
  const enabled = runInBackground.useValue();
  const description = t(enabled ? "settings.background.description.on" : "settings.background.description.off");
  return (
    <SettingRow label={t("settings.background.label")} description={description}>
      <Switch aria-label={t("settings.background.label")} isSelected={enabled} onChange={(value) => runInBackground.set(value)}>
        <Switch.Content>
          <Switch.Control>
            <Switch.Thumb />
          </Switch.Control>
        </Switch.Content>
      </Switch>
    </SettingRow>
  );
}
