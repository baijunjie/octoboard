import React from "react";

import { errorPresentation } from "../errorPresentation";
import { useT } from "../i18n/react";
import { useDaemon, useDaemonStore } from "../store";
import { SwitchSettingRow } from "./SettingRow";

/** Whether a new session uses the account whose weekly quota resets soonest (see
 * `Settings.auto_select_account` in protocol.ts). The row's control never unmounts on its
 * own, so this needs no `useSectionRefocus`. */
export function AutoSelectAccountSetting(): React.ReactElement {
  const t = useT();
  const { request, toastError } = useDaemon();
  const enabled = useDaemonStore((s) => s.settings.auto_select_account);
  const description = t(enabled ? "settings.autoSelect.description.on" : "settings.autoSelect.description.off");

  const setEnabled = (value: boolean) =>
    void request({ type: "update_settings", auto_select_account: value }).catch((err) => toastError(errorPresentation(err as Error), undefined, t("settings.saveFailedTitle")));

  return (
    <SwitchSettingRow label={t("settings.autoSelect.label")} description={description} isSelected={enabled} onChange={setEnabled} />
  );
}
