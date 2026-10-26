import React from "react";

import { errorPresentation } from "../errorPresentation";
import { useT } from "../i18n/react";
import { useDaemon, useDaemonStore } from "../store";
import { SwitchSettingRow } from "./SettingRow";

/** Whether sessions are told which language the interface is set to and asked to talk to the user
 * in it (see `Settings.reply_in_interface_language` in protocol.ts). The row's control never
 * unmounts on its own, so this needs no `useSectionRefocus`. */
export function ReplyInInterfaceLanguageSetting(): React.ReactElement {
  const t = useT();
  const { request, toastError } = useDaemon();
  const enabled = useDaemonStore((s) => s.settings.reply_in_interface_language ?? true);
  const description = t(enabled ? "settings.replyInInterfaceLanguage.description.on" : "settings.replyInInterfaceLanguage.description.off");

  const setEnabled = (value: boolean) =>
    void request({ type: "update_settings", reply_in_interface_language: value }).catch((err) => toastError(errorPresentation(err as Error)));

  return (
    <SwitchSettingRow label={t("settings.replyInInterfaceLanguage.label")} description={description} isSelected={enabled} onChange={setEnabled} />
  );
}
