import { Switch } from "@heroui/react";
import React from "react";

import { useT } from "../i18n/react";
import { useDaemon, useDaemonStore } from "../store";
import { SettingRow } from "./SettingRow";

/** Whether the daemon fast-forwards a project's branch on its own, rather than only reporting how
 * far ahead or behind it is (see `Settings.auto_sync_repositories` in protocol.ts). The row's
 * control never unmounts on its own, so this needs no `useSectionRefocus`. */
export function GitSection(): React.ReactElement {
  const t = useT();
  const { request, toastError } = useDaemon();
  const autoSync = useDaemonStore((s) => s.settings.auto_sync_repositories);

  const setAutoSync = (value: boolean) =>
    void request({ type: "update_settings", auto_sync_repositories: value }).catch((err) => toastError((err as Error).message));

  return (
    <SettingRow label={t("settings.git.autoSync.label")} description={t("settings.git.autoSync.description")}>
      <Switch aria-label={t("settings.git.autoSync.label")} isSelected={autoSync} onChange={setAutoSync}>
        <Switch.Content>
          <Switch.Control>
            <Switch.Thumb />
          </Switch.Control>
        </Switch.Content>
      </Switch>
    </SettingRow>
  );
}
