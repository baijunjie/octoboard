import React from "react";

import { HelpTip } from "../components/HelpTip";
import { errorPresentation } from "../errorPresentation";
import { useT } from "../i18n/react";
import { useDaemon, useDaemonStore } from "../store";
import { SwitchSettingRow } from "./SettingRow";

/** Whether the daemon starts a usage window that has reset but not begun counting, by sending one
 * small request on its account (see `Settings.auto_start_quota_windows` in protocol.ts). The row's
 * control never unmounts on its own, so this needs no `useSectionRefocus`. */
export function StartQuotaWindowsSetting(): React.ReactElement {
  const t = useT();
  const { request, toastError } = useDaemon();
  const enabled = useDaemonStore((s) => s.settings.auto_start_quota_windows ?? false);
  const description = t(enabled ? "settings.startQuotaWindows.description.on" : "settings.startQuotaWindows.description.off");

  const setEnabled = (value: boolean) =>
    void request({ type: "update_settings", auto_start_quota_windows: value }).catch((err) => toastError(errorPresentation(err as Error), undefined, t("settings.saveFailedTitle")));

  return (
    <SwitchSettingRow
      label={t("settings.startQuotaWindows.label")}
      help={<HelpTip label={t("settings.startQuotaWindows.help")}>{t("settings.startQuotaWindows.helpText")}</HelpTip>}
      description={description}
      isSelected={enabled}
      onChange={setEnabled}
    />
  );
}
