import React from "react";

import { alertAppDataFailure } from "../components/alerts/raiseError";
import { HelpTip } from "../components/hint/HelpTip";
import { useT } from "../i18n/react";
import { useOptionalPlatform } from "../platform/react";
import { useDaemon, useDaemonStore } from "../store";
import { booleanSetting, type BooleanField } from "../configuration/booleanSettings";
import { SwitchSettingRow } from "./SettingRow";

/** A setting that is one on/off switch the daemon holds (`booleanSetting` gives each its copy; see
 * `Settings.<field>` in protocol.generated.ts for what it does). The row's control never unmounts on its own, so this
 * needs no `useSectionRefocus`. `motionKey` names the row for a list that animates the rows around it
 * (`useListMotion`). */
export function DaemonSwitchSetting({ field, motionKey }: { field: BooleanField; motionKey?: string }): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const enabled = useDaemonStore((s) => s.settings[field]);
  const { label, description, help } = booleanSetting(field);
  // A setting that does something more only in the macOS app says so there, and not to a client that cannot do it.
  // The row is also shown with no platform above it (a dialog's Settings opened on its own), which counts as no app.
  const platform = useOptionalPlatform();
  const inMacosApp = platform?.exit !== undefined && platform.host.os === "macos";
  const on = inMacosApp ? (description.onMacos ?? description.on) : description.on;

  const setEnabled = (value: boolean) =>
    request({ type: "update_settings", [field]: value }).catch(alertAppDataFailure(t("settings.saveFailedTitle")));

  return (
    <SwitchSettingRow
      motionKey={motionKey}
      label={t(label)}
      help={help && <HelpTip label={t(help.label)}>{t(help.text)}</HelpTip>}
      description={t(enabled ? on : description.off)}
      isSelected={enabled}
      onChange={setEnabled}
    />
  );
}
