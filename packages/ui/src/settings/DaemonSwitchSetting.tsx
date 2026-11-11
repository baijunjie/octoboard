import React from "react";

import { HelpTip } from "../components/hint/HelpTip";
import { useT } from "../i18n/react";
import { useDaemonStore } from "../store";
import { booleanSetting, type BooleanField } from "../configuration/booleanSettings";
import { SwitchSettingRow } from "./SettingRow";
import { useSettingsWrite } from "./useSettingsWrite";

/** A setting that is one on/off switch the daemon holds (`booleanSetting` gives each its copy; see
 * `Settings.<field>` in protocol.generated.ts for what it does). The row's control never unmounts on its own, so this
 * needs no `useSectionRefocus`. The write is based on the setting's version in the store, so another client's change
 * to it is refused as a conflict, and the switch, which shows the store's value, shows that change (see `useSettingsWrite`).
 * `motionKey` names the row for a list that animates the rows around it (`useListMotion`). */
export function DaemonSwitchSetting({ field, motionKey }: { field: BooleanField; motionKey?: string }): React.ReactElement {
  const t = useT();
  const write = useSettingsWrite();
  const enabled = useDaemonStore((s) => s.settings[field]);
  const { label, description, help } = booleanSetting(field);

  const setEnabled = (value: boolean) => write({ [field]: value }, [field]);

  return (
    <SwitchSettingRow
      searchId={field}
      motionKey={motionKey}
      label={t(label)}
      help={help && <HelpTip label={t(help.label)}>{t(help.text)}</HelpTip>}
      description={t(enabled ? description.on : description.off)}
      isSelected={enabled}
      onChange={setEnabled}
    />
  );
}
