import React from "react";

import { errorPresentation } from "../errorPresentation";
import { useT } from "../i18n/react";
import { useDaemon, useDaemonStore } from "../store";
import { SwitchSettingRow } from "./SettingRow";

/** Whether an owner's `handoff_session` calls run without asking the user (see
 * `Settings.auto_approve_handoffs` in protocol.ts). The row's control never unmounts on its own, so
 * this needs no `useSectionRefocus`. */
export function AutoApproveHandoffsSetting(): React.ReactElement {
  const t = useT();
  const { request, toastError } = useDaemon();
  const enabled = useDaemonStore((s) => s.settings.auto_approve_handoffs);
  const description = t(enabled ? "settings.autoApproveHandoffs.description.on" : "settings.autoApproveHandoffs.description.off");

  const setEnabled = (value: boolean) =>
    void request({ type: "update_settings", auto_approve_handoffs: value }).catch((err) => toastError(errorPresentation(err as Error), undefined, t("settings.saveFailedTitle")));

  return (
    <SwitchSettingRow label={t("settings.autoApproveHandoffs.label")} description={description} isSelected={enabled} onChange={setEnabled} />
  );
}
