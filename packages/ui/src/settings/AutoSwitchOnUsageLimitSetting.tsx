import React from "react";

import { errorPresentation } from "../errorPresentation";
import { useT } from "../i18n/react";
import { useDaemon, useDaemonStore } from "../store";
import { SwitchSettingRow } from "./SettingRow";

/** Whether Octoboard switches a session to another account by itself when its turn stopped on its account's usage
 * limit (see `Settings.auto_switch_on_usage_limit` in protocol.ts). It governs only that: how a new session picks its
 * account is `AutoSelectAccountSetting`'s, and whether an agent's request for a switch asks the user is the approval's.
 * The row's control never unmounts on its own, so this needs no `useSectionRefocus`. */
export function AutoSwitchOnUsageLimitSetting(): React.ReactElement {
  const t = useT();
  const { request, toastError } = useDaemon();
  const enabled = useDaemonStore((s) => s.settings.auto_switch_on_usage_limit);
  const description = t(enabled ? "settings.autoSwitch.description.on" : "settings.autoSwitch.description.off");

  const setEnabled = (value: boolean) =>
    void request({ type: "update_settings", auto_switch_on_usage_limit: value }).catch((err) => toastError(errorPresentation(err as Error), undefined, t("settings.saveFailedTitle")));

  return (
    <SwitchSettingRow label={t("settings.autoSwitch.label")} description={description} isSelected={enabled} onChange={setEnabled} />
  );
}
