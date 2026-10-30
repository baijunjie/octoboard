import React, { useRef } from "react";

import { useFocusHandoff } from "../components/useFocusHandoff";
import { DELIVERY_CHOICES, DEVELOPMENT_CHOICES } from "../gitWorkflow";
import { useT } from "../i18n/react";
import type { GitDelivery, GitDevelopment, GitWorkflowOverride } from "../protocol";
import { useDaemonStore } from "../store";
import { GroupRow, GroupRows } from "./GroupRows";
import { LabelledGroup } from "./LabelledGroup";
import { OptionSelect, type Option } from "./OptionSelect";

/** What a picker holds for an item that follows the application-wide value; an own value is its own wire value. */
const FOLLOW = "follow";
type Follow = typeof FOLLOW;

/** A project's own git workflow settings (see `Project.git_workflow`): each item is "Same as global
 * setting", with the application-wide value in a badge, or a value of its own. `value` holds `null` for an item that
 * follows. The group stands under a tab of its own, which names it on the screen, so its own name is kept off the
 * screen (it still names the group). */
export function GitWorkflowField({
  value,
  onChange,
  isDisabled,
}: {
  value: GitWorkflowOverride;
  onChange: (value: GitWorkflowOverride) => void;
  isDisabled?: boolean;
}): React.ReactElement {
  const t = useT();
  const global = useDaemonStore((s) => s.settings.git_workflow);
  const onOff = (on: boolean) => t(on ? "settings.gitWorkflow.on" : "settings.gitWorkflow.off");
  // Squashing, rebase and delivery only matter while sessions work on a branch of their own (the rule they shape is left out
  // otherwise), so they are not shown on the current branch; their values are kept. The revert gate does not depend on it.
  const onOwnBranch = (value.development ?? global.development) !== "in_place";
  // Another window, or an agent's update, can put the effective development on the current branch under a focused
  // picker, which would leave focus on `<body>` and the modal without Escape and Tab.
  const rows = useRef<HTMLDivElement>(null);
  const handoff = useFocusHandoff(() => rows.current?.querySelector("button")?.focus());
  const sameAsGlobal = t("dialog.project.gitWorkflow.sameAsGlobal");

  const developmentLabel = t("settings.gitWorkflow.development.label");
  const development = DEVELOPMENT_CHOICES.map(({ value: choice, label }) => ({ value: choice, label: t(label) }));
  const developmentOptions: Option<Follow | GitDevelopment>[] = [
    { value: FOLLOW, label: sameAsGlobal, badge: development.find((option) => option.value === global.development)?.label, badgeInLabel: true },
    ...development,
  ];

  const flagPicker = (item: "squash_before_delivery" | "rebase_before_merge" | "revert_gate", label: string) => {
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

  const deliveryLabel = t("settings.gitWorkflow.delivery.label");
  const delivery = DELIVERY_CHOICES.map(({ value: choice, label }) => ({ value: choice, label: t(label) }));
  const deliveryOptions: Option<Follow | GitDelivery>[] = [
    { value: FOLLOW, label: sameAsGlobal, badge: delivery.find((option) => option.value === global.delivery)?.label, badgeInLabel: true },
    ...delivery,
  ];

  return (
    <LabelledGroup label={t("dialog.project.gitWorkflow.title")} description={t("dialog.project.gitWorkflow.description")} isDisabled={isDisabled} hideLabel>
      <div ref={rows} className="contents" onFocus={handoff.onFocus} onBlur={handoff.onBlur}>
        <GroupRows stacked>
          <GroupRow label={developmentLabel}>
            <OptionSelect
              hideLabel
              label={developmentLabel}
              options={developmentOptions}
              value={value.development ?? FOLLOW}
              onChange={(picked) => onChange({ ...value, development: picked === FOLLOW ? null : picked })}
              isDisabled={isDisabled}
            />
          </GroupRow>
          {onOwnBranch && (
            <>
              {flagPicker("squash_before_delivery", t("settings.gitWorkflow.squash.label"))}
              {flagPicker("rebase_before_merge", t("settings.gitWorkflow.rebaseBeforeDelivery.label"))}
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
          {flagPicker("revert_gate", t("settings.gitWorkflow.revertGate.label"))}
        </GroupRows>
      </div>
    </LabelledGroup>
  );
}
