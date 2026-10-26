import React from "react";

import { errorPresentation } from "../errorPresentation";
import { useT } from "../i18n/react";
import { ordersByStatus } from "../sidebar/order";
import { useDaemon, useDaemonStore } from "../store";
import { SwitchSettingRow } from "./SettingRow";

/** Whether the sidebar ranks sessions and projects by status (see
 * `Settings.sidebar_order_by_status` in protocol.ts). The row's control never unmounts on its
 * own, so this needs no `useSectionRefocus`. */
export function SidebarOrderSetting(): React.ReactElement {
  const t = useT();
  const { request, toastError } = useDaemon();
  const enabled = useDaemonStore((s) => ordersByStatus(s.settings));
  const description = t(enabled ? "settings.sidebarOrder.description.on" : "settings.sidebarOrder.description.off");

  const setEnabled = (value: boolean) =>
    void request({ type: "update_settings", sidebar_order_by_status: value }).catch((err) => toastError(errorPresentation(err as Error), undefined, t("settings.saveFailedTitle")));

  return (
    <SwitchSettingRow label={t("settings.sidebarOrder.label")} description={description} isSelected={enabled} onChange={setEnabled} />
  );
}
