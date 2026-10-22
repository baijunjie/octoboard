import React from "react";

import { errorPresentation } from "../errorPresentation";
import { useT } from "../i18n/react";
import { useDaemon, useDaemonStore } from "../store";
import { SwitchSettingRow } from "./SettingRow";

/** Whether a session's `request_resume_session` call is approved without asking the user (see
 * `Settings.auto_approve_resume_requests` in protocol.ts). The row's control never unmounts on its
 * own, so this needs no `useSectionRefocus`. */
export function AutoApproveResumeRequestsSetting(): React.ReactElement {
  const t = useT();
  const { request, toastError } = useDaemon();
  const enabled = useDaemonStore((s) => s.settings.auto_approve_resume_requests ?? false);
  const description = t(enabled ? "settings.autoApproveResume.description.on" : "settings.autoApproveResume.description.off");

  const setEnabled = (value: boolean) =>
    void request({ type: "update_settings", auto_approve_resume_requests: value }).catch((err) => toastError(errorPresentation(err as Error)));

  return (
    <SwitchSettingRow label={t("settings.autoApproveResume.label")} description={description} isSelected={enabled} onChange={setEnabled} />
  );
}
