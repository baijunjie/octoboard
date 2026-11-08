import React from "react";

import { useFocusHandoff } from "../components/focus/useFocusHandoff";
import { OptionSelect } from "../components/form/OptionSelect";
import { useListMotion } from "../components/motion/useListMotion";
import { useT } from "../i18n/react";
import { keepAwake, keepAwakeWhen, KEEP_AWAKE_WHEN, KEEP_AWAKE_WHEN_LABEL, type KeepAwakeWhen } from "../lifecycle/keepAwake";
import { SettingRow, SwitchSettingRow } from "./SettingRow";

const WHEN_DESCRIPTION = { always: "settings.keepAwake.when.description.always", working: "settings.keepAwake.when.description.working" } as const;

/** Whether the app keeps the computer and its display awake, and when: always, or while a session is working. Rendered
 * only where the platform can hold the computer awake (`AppBehaviorSection`); what to hold reaches the shell through
 * `useKeepAwakeSync`. The choice of when is shown only while the switch is on, its value kept while it is off. */
export function KeepAwakeSetting(): React.ReactElement {
  const t = useT();
  const enabled = keepAwake.useValue();
  const when = keepAwakeWhen.useValue();
  const listRef = useListMotion<HTMLDivElement>({ fitHeight: true });
  // An import can turn the switch off while focus is on the choice it hides; focus goes to the switch.
  const handoff = useFocusHandoff(() => listRef.current?.querySelector<HTMLElement>('input[role="switch"]')?.focus());
  const options = KEEP_AWAKE_WHEN.map((value) => ({ value, label: t(KEEP_AWAKE_WHEN_LABEL[value]) }));
  return (
    <div ref={listRef} className="relative" onFocus={handoff.onFocus} onBlur={handoff.onBlur}>
      <SwitchSettingRow
        motionKey="keep_awake"
        label={t("settings.keepAwake.label")}
        description={t(enabled ? "settings.keepAwake.description.on" : "settings.keepAwake.description.off")}
        isSelected={enabled}
        onChange={(value) => keepAwake.set(value)}
      />
      {enabled && (
        <SettingRow fill motionKey="keep_awake_when" label={t("settings.keepAwake.when.label")} description={t(WHEN_DESCRIPTION[when])}>
          <OptionSelect<KeepAwakeWhen> inline label={t("settings.keepAwake.when.label")} options={options} value={when} onChange={(value) => keepAwakeWhen.set(value)} />
        </SettingRow>
      )}
    </div>
  );
}
