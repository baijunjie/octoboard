import React from "react";

import { HelpTip } from "../components/HelpTip";
import { InlineReference } from "../components/InlineReference";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { useListMotion } from "../components/useListMotion";
import { OptionSelect } from "../dialogs/OptionSelect";
import { errorPresentation } from "../errorPresentation";
import { Message, useT } from "../i18n/react";
import type { WorktreeDelivery, WorktreeSettingsChanges } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { WORKTREE_DELIVERY_CHOICES } from "../worktreeSettings";
import { SettingRow, SwitchSettingRow } from "./SettingRow";

/** The application-wide worktree settings (see `Settings.worktrees` in protocol.ts): how a project session is asked
 * to use worktrees and deliver finished work, and the revert gate. Each change sends only its own item. A project may
 * hold its own value for any of them, which the project dialog sets. */
export function WorktreesSection(): React.ReactElement {
  const t = useT();
  const { request, toastError } = useDaemon();
  const listRef = useListMotion<HTMLDivElement>();
  const worktrees = useDaemonStore((s) => s.settings.worktrees);
  // Another window, or an agent's update, can turn Use worktrees off while focus is on a row it hides.
  const handoff = useFocusHandoff(() => listRef.current?.querySelector("input")?.focus());

  const change = (changes: WorktreeSettingsChanges) =>
    void request({ type: "update_settings", worktrees: changes }).catch((err) => toastError(errorPresentation(err as Error)));

  return (
    <div ref={listRef} className="relative" onFocus={handoff.onFocus} onBlur={handoff.onBlur}>
      <SwitchSettingRow
        motionKey="use_worktrees"
        label={t("settings.worktrees.useWorktrees.label")}
        description={t(worktrees.use_worktrees ? "settings.worktrees.useWorktrees.description.on" : "settings.worktrees.useWorktrees.description.off")}
        isSelected={worktrees.use_worktrees}
        onChange={(value) => change({ use_worktrees: value })}
      />
      {/* Rebase, delivery and the revert gate only matter while worktrees are used: the rule they shape is left out and
          the gate does not run without them. They are not shown otherwise; their values are kept. */}
      {worktrees.use_worktrees && (
        <>
          <SwitchSettingRow
            motionKey="rebase_before_merge"
            label={t("settings.worktrees.rebase.label")}
            description={t(worktrees.rebase_before_merge ? "settings.worktrees.rebase.description.on" : "settings.worktrees.rebase.description.off")}
            isSelected={worktrees.rebase_before_merge}
            onChange={(value) => change({ rebase_before_merge: value })}
          />
          <SwitchSettingRow
            motionKey="revert_gate"
            label={t("settings.worktrees.revertGate.label")}
            help={
              <HelpTip label={t("settings.worktrees.revertGate.help")}>
                <Message
                  id="settings.worktrees.revertGate.deliberate"
                  params={{
                    gitRevert: <InlineReference monospace>git revert</InlineReference>,
                    reverts: <InlineReference monospace>Reverts: &lt;sha&gt;</InlineReference>,
                  }}
                />
              </HelpTip>
            }
            description={t(worktrees.revert_gate ? "settings.worktrees.revertGate.description.on" : "settings.worktrees.revertGate.description.off")}
            isSelected={worktrees.revert_gate}
            onChange={(value) => change({ revert_gate: value })}
          />
          <SettingRow motionKey="delivery" label={t("settings.worktrees.delivery.label")} description={t("settings.worktrees.delivery.description")}>
            <OptionSelect<WorktreeDelivery>
              inline
              label={t("settings.worktrees.delivery.label")}
              options={WORKTREE_DELIVERY_CHOICES.map(({ value, label }) => ({ value, label: t(label) }))}
              value={worktrees.delivery}
              onChange={(value) => change({ delivery: value })}
            />
          </SettingRow>
        </>
      )}
    </div>
  );
}
