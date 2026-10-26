import React from "react";

import { errorPresentation } from "../errorPresentation";
import { Message, useT } from "../i18n/react";
import { useDaemon, useDaemonStore } from "../store";
import { SwitchSettingRow } from "./SettingRow";

/** Whether the daemon deletes, on its own, the archived sessions whose conversation the agent no
 * longer keeps (see `Settings.auto_delete_unresumable_archives` in protocol.ts). The row's control
 * never unmounts on its own, so this needs no `useSectionRefocus`. */
export function ArchiveCleanupSetting(): React.ReactElement {
  const t = useT();
  const { request, toastError } = useDaemon();
  const enabled = useDaemonStore((s) => s.settings.auto_delete_unresumable_archives);
  const description = <Message id={enabled ? "settings.archiveCleanup.description.on" : "settings.archiveCleanup.description.off"} />;

  const setEnabled = (value: boolean) =>
    void request({ type: "update_settings", auto_delete_unresumable_archives: value }).catch((err) => toastError(errorPresentation(err as Error), undefined, t("settings.saveFailedTitle")));

  return (
    <SwitchSettingRow label={t("settings.archiveCleanup.label")} description={description} isSelected={enabled} onChange={setEnabled} />
  );
}
