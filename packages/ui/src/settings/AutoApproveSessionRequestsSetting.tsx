import React from "react";

import { errorPresentation } from "../errorPresentation";
import { useT } from "../i18n/react";
import { useDaemon, useDaemonStore } from "../store";
import { SwitchSettingRow } from "./SettingRow";

/** Whether a session's `request_session` call is approved without asking the user (see
 * `Settings.auto_approve_session_requests` in protocol.ts). The row's control never unmounts on its
 * own, so this needs no `useSectionRefocus`. */
export function AutoApproveSessionRequestsSetting(): React.ReactElement {
  const t = useT();
  const { request, toastError } = useDaemon();
  const enabled = useDaemonStore((s) => s.settings.auto_approve_session_requests ?? false);
  const description = t(enabled ? "settings.autoApproveSession.description.on" : "settings.autoApproveSession.description.off");

  const setEnabled = (value: boolean) =>
    void request({ type: "update_settings", auto_approve_session_requests: value }).catch((err) => toastError(errorPresentation(err as Error), undefined, t("settings.saveFailedTitle")));

  return (
    <SwitchSettingRow label={t("settings.autoApproveSession.label")} description={description} isSelected={enabled} onChange={setEnabled} />
  );
}
