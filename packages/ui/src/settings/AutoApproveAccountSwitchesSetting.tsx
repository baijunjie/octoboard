import React from "react";

import { errorPresentation } from "../errorPresentation";
import { useT } from "../i18n/react";
import { useDaemon, useDaemonStore } from "../store";
import { SwitchSettingRow } from "./SettingRow";

/** Whether an owner's `switch_session_account` calls run without asking the user (see
 * `Settings.auto_approve_account_switches` in protocol.ts). The row's control never unmounts on its
 * own, so this needs no `useSectionRefocus`. */
export function AutoApproveAccountSwitchesSetting(): React.ReactElement {
  const t = useT();
  const { request, toastError } = useDaemon();
  const enabled = useDaemonStore((s) => s.settings.auto_approve_account_switches);
  const description = t(enabled ? "settings.autoApproveAccountSwitches.description.on" : "settings.autoApproveAccountSwitches.description.off");

  const setEnabled = (value: boolean) =>
    void request({ type: "update_settings", auto_approve_account_switches: value }).catch((err) => toastError(errorPresentation(err as Error), undefined, t("settings.saveFailedTitle")));

  return (
    <SwitchSettingRow label={t("settings.autoApproveAccountSwitches.label")} description={description} isSelected={enabled} onChange={setEnabled} />
  );
}
