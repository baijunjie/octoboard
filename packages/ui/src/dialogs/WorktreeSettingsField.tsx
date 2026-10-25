import React, { useRef } from "react";

import { useFocusHandoff } from "../components/useFocusHandoff";
import { useT } from "../i18n/react";
import type { WorktreeDelivery, WorktreeOverride } from "../protocol";
import { useDaemonStore } from "../store";
import { WORKTREE_DELIVERY_CHOICES } from "../worktreeSettings";
import { GroupRow, GroupRows } from "./GroupRows";
import { LabelledGroup } from "./LabelledGroup";
import { OptionSelect, type Option } from "./OptionSelect";

/** What a picker holds for an item that follows the application-wide value; an own value is its own wire value. */
const FOLLOW = "follow";
type Follow = typeof FOLLOW;

/** A project's own worktree settings (see `Project.worktrees`): each of the four items is "Same as app setting",
 * with the application-wide value in a badge, or a value of its own. `value` holds `null` for an item that follows. */
export function WorktreeSettingsField({
  value,
  onChange,
  isDisabled,
}: {
  value: WorktreeOverride;
  onChange: (value: WorktreeOverride) => void;
  isDisabled?: boolean;
}): React.ReactElement {
  const t = useT();
  const global = useDaemonStore((s) => s.settings.worktrees);
  const onOff = (on: boolean) => t(on ? "settings.worktrees.on" : "settings.worktrees.off");
  // Rebase, delivery and the revert gate only matter while the project uses worktrees (the rule they shape is left out
  // and the gate does not run otherwise), so they are not shown then; their values are kept.
  const usesWorktrees = value.use_worktrees ?? global.use_worktrees;
  // Another window, or an agent's update, can turn the effective Use worktrees off under a focused picker, which
  // would leave focus on `<body>` and the modal without Escape and Tab.
  const rows = useRef<HTMLDivElement>(null);
  const handoff = useFocusHandoff(() => rows.current?.querySelector("button")?.focus());
  const sameAsGlobal = t("dialog.project.worktrees.sameAsGlobal");

  const flagPicker = (item: "use_worktrees" | "rebase_before_merge" | "revert_gate", label: string) => {
    const options: Option<Follow | "on" | "off">[] = [
      { value: FOLLOW, label: sameAsGlobal, badge: onOff(global[item]), badgeInLabel: true },
      { value: "on", label: onOff(true) },
      { value: "off", label: onOff(false) },
    ];
    return (
      <GroupRow label={label}>
        <OptionSelect
          hideLabel
          label={label}
          options={options}
          value={value[item] === null ? FOLLOW : value[item] ? "on" : "off"}
          onChange={(picked) => onChange({ ...value, [item]: picked === FOLLOW ? null : picked === "on" })}
          isDisabled={isDisabled}
        />
      </GroupRow>
    );
  };

  const deliveryLabel = t("settings.worktrees.delivery.label");
  const delivery = WORKTREE_DELIVERY_CHOICES.map(({ value: choice, label }) => ({ value: choice, label: t(label) }));
  const deliveryOptions: Option<Follow | WorktreeDelivery>[] = [
    { value: FOLLOW, label: sameAsGlobal, badge: delivery.find((option) => option.value === global.delivery)?.label, badgeInLabel: true },
    ...delivery,
  ];

  return (
    <LabelledGroup label={t("settings.section.worktrees")} description={t("dialog.project.worktrees.description")} isDisabled={isDisabled}>
      <div ref={rows} className="contents" onFocus={handoff.onFocus} onBlur={handoff.onBlur}>
        <GroupRows stacked>
          {flagPicker("use_worktrees", t("settings.worktrees.useWorktrees.label"))}
          {usesWorktrees && (
            <>
              {flagPicker("rebase_before_merge", t("settings.worktrees.rebase.label"))}
              {flagPicker("revert_gate", t("settings.worktrees.revertGate.label"))}
              <GroupRow label={deliveryLabel}>
                <OptionSelect
                  hideLabel
                  label={deliveryLabel}
                  options={deliveryOptions}
                  value={value.delivery ?? FOLLOW}
                  onChange={(picked) => onChange({ ...value, delivery: picked === FOLLOW ? null : picked })}
                  isDisabled={isDisabled}
                />
              </GroupRow>
            </>
          )}
        </GroupRows>
      </div>
    </LabelledGroup>
  );
}
