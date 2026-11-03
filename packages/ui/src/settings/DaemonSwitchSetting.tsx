import React from "react";

import { alertAppDataFailure } from "../components/alerts/raiseError";
import { HelpTip } from "../components/hint/HelpTip";
import { useT } from "../i18n/react";
import { useDaemon, useDaemonStore } from "../store";
import { booleanSetting, type BooleanField } from "../configuration/booleanSettings";
import { SwitchSettingRow } from "./SettingRow";

/** A setting that is one on/off switch the daemon holds (`BOOLEAN_SETTINGS` lists each with its copy; see
 * `Settings.<field>` in protocol.generated.ts for what it does). The row's control never unmounts on its own, so this
 * needs no `useSectionRefocus`. */
export function DaemonSwitchSetting({ field }: { field: BooleanField }): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const enabled = useDaemonStore((s) => s.settings[field]);
  const { label, description, help } = booleanSetting(field);

  const setEnabled = (value: boolean) =>
    void request({ type: "update_settings", [field]: value }).catch(alertAppDataFailure(t("settings.saveFailedTitle")));

  return (
    <SwitchSettingRow
      label={t(label)}
      help={help && <HelpTip label={t(help.label)}>{t(help.text)}</HelpTip>}
      description={t(enabled ? description.on : description.off)}
      isSelected={enabled}
      onChange={setEnabled}
    />
  );
}
