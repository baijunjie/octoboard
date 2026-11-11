import React from "react";

import { useFocusHandoff } from "../components/focus/useFocusHandoff";
import { OptionSelect } from "../components/form/OptionSelect";
import { useListMotion } from "../components/motion/useListMotion";
import { useT } from "../i18n/react";
import { KEEP_AWAKE_WHEN, KEEP_AWAKE_WHEN_DESCRIPTION, KEEP_AWAKE_WHEN_LABEL } from "../configuration/keepAwakeWhen";
import type { KeepAwakeWhen } from "../protocol";
import { useDaemonStore } from "../store";
import { DaemonSwitchSetting } from "./DaemonSwitchSetting";
import { SettingRow } from "./SettingRow";
import { useSettingsWrite } from "./useSettingsWrite";

/** Whether the daemon keeps the computer and its display awake, and when: always, or while a session is working
 * (`Settings.keep_awake` and `keep_awake_when`, changed through `update_settings`, each write based on the version the
 * store holds, as `useSettingsWrite` sends it). Rendered only where the daemon's host can hold the computer awake
 * (`AppBehaviorSection`, from `host.keep_awake`). The choice of when is shown only while the switch is on, its value
 * kept while it is off. */
export function KeepAwakeSetting(): React.ReactElement {
  const t = useT();
  const write = useSettingsWrite();
  const enabled = useDaemonStore((s) => s.settings.keep_awake);
  const when = useDaemonStore((s) => s.settings.keep_awake_when);
  const listRef = useListMotion<HTMLDivElement>({ fitHeight: true });
  // An import can turn the switch off while focus is on the choice it hides; focus goes to the switch.
  const handoff = useFocusHandoff(() => listRef.current?.querySelector<HTMLElement>('input[role="switch"]')?.focus());
  const options = KEEP_AWAKE_WHEN.map((value) => ({ value, label: t(KEEP_AWAKE_WHEN_LABEL[value]) }));
  const setWhen = (value: KeepAwakeWhen) => write({ keep_awake_when: value }, ["keep_awake_when"]);
  return (
    // An item of the Settings section's own list motion too, so that it slides as a whole, with its rows, when what is
    // above it comes or goes.
    <div ref={listRef} data-list-motion="keep-awake" className="relative" onFocus={handoff.onFocus} onBlur={handoff.onBlur}>
      <DaemonSwitchSetting field="keep_awake" motionKey="keep_awake" />
      {enabled && (
        <SettingRow fill searchId="keep_awake_when" motionKey="keep_awake_when" label={t("settings.keepAwake.when.label")} description={t(KEEP_AWAKE_WHEN_DESCRIPTION[when])}>
          <OptionSelect<KeepAwakeWhen> inline label={t("settings.keepAwake.when.label")} options={options} value={when} onChange={setWhen} />
        </SettingRow>
      )}
    </div>
  );
}
