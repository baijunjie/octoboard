import React from "react";

import { HelpTip } from "../components/HelpTip";
import { errorPresentation } from "../errorPresentation";
import { useT } from "../i18n/react";
import { useDaemon, useDaemonStore } from "../store";
import { SwitchSettingRow } from "./SettingRow";

/** Whether the daemon fast-forwards a project's branch on its own, rather than only reporting how
 * far ahead or behind it is (see `Settings.auto_pull_repositories` in protocol.ts). The row's
 * control never unmounts on its own, so this needs no `useSectionRefocus`. */
export function GitSection(): React.ReactElement {
  const t = useT();
  const { request, toastError } = useDaemon();
  const autoPull = useDaemonStore((s) => s.settings.auto_pull_repositories);

  const setAutoPull = (value: boolean) =>
    void request({ type: "update_settings", auto_pull_repositories: value }).catch((err) => toastError(errorPresentation(err as Error)));

  return (
    <SwitchSettingRow
      label={t("settings.git.autoPull.label")}
      help={<HelpTip label={t("settings.git.autoPull.help")}>{t("settings.git.autoPull.remoteCheck")}</HelpTip>}
      description={t("settings.git.autoPull.description")}
      isSelected={autoPull}
      onChange={setAutoPull}
    />
  );
}
