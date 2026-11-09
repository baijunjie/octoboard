import React from "react";

import { alertAppDataFailure } from "../components/alerts/raiseError";
import { useFocusHandoff } from "../components/focus/useFocusHandoff";
import { OptionSelect } from "../components/form/OptionSelect";
import { useListMotion } from "../components/motion/useListMotion";
import { useT } from "../i18n/react";
import { KEEP_AWAKE_WHEN, KEEP_AWAKE_WHEN_LABEL } from "../configuration/keepAwakeWhen";
import type { KeepAwakeWhen } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { DaemonSwitchSetting } from "./DaemonSwitchSetting";
import { SettingRow } from "./SettingRow";

const WHEN_DESCRIPTION = { always: "settings.keepAwake.when.description.always", working: "settings.keepAwake.when.description.working" } as const;

/** Whether the daemon keeps the computer and its display awake, and when: always, or while a session is working
 * (`Settings.keep_awake` and `keep_awake_when`, changed through `update_settings`). Rendered only where the daemon's
 * host can hold the computer awake (`AppBehaviorSection`, from `host.keep_awake`). The choice of when is shown only
 * while the switch is on, its value kept while it is off. */
export function KeepAwakeSetting(): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const enabled = useDaemonStore((s) => s.settings.keep_awake);
  const when = useDaemonStore((s) => s.settings.keep_awake_when);
  const listRef = useListMotion<HTMLDivElement>({ fitHeight: true });
  // An import can turn the switch off while focus is on the choice it hides; focus goes to the switch.
  const handoff = useFocusHandoff(() => listRef.current?.querySelector<HTMLElement>('input[role="switch"]')?.focus());
  const options = KEEP_AWAKE_WHEN.map((value) => ({ value, label: t(KEEP_AWAKE_WHEN_LABEL[value]) }));
  const setWhen = (value: KeepAwakeWhen) =>
    request({ type: "update_settings", keep_awake_when: value }).catch(alertAppDataFailure(t("settings.saveFailedTitle")));
  return (
    <div ref={listRef} className="relative" onFocus={handoff.onFocus} onBlur={handoff.onBlur}>
      <DaemonSwitchSetting field="keep_awake" motionKey="keep_awake" />
      {enabled && (
        <SettingRow fill motionKey="keep_awake_when" label={t("settings.keepAwake.when.label")} description={t(WHEN_DESCRIPTION[when])}>
          <OptionSelect<KeepAwakeWhen> inline label={t("settings.keepAwake.when.label")} options={options} value={when} onChange={setWhen} />
        </SettingRow>
      )}
    </div>
  );
}
