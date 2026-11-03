import React from "react";

import { HelpTip } from "../components/hint/HelpTip";
import { InlineReference } from "../i18n/InlineReference";
import { useFocusHandoff } from "../components/focus/useFocusHandoff";
import { useListMotion } from "../components/motion/useListMotion";
import { OptionSelect } from "../components/form/OptionSelect";
import { DELIVERY_CHOICES, DEVELOPMENT_CHOICES, GIT_WORKFLOW_ITEMS } from "../git/gitWorkflow";
import { Message, useT } from "../i18n/react";
import type { GitDelivery, GitDevelopment, GitWorkflowSettingsChanges } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { CloneDirSetting } from "./CloneDirSetting";
import { DaemonSwitchSetting } from "./DaemonSwitchSetting";
import { SettingRow, SwitchSettingRow } from "./SettingRow";
import { SettingsGroup } from "./SettingsGroup";
import { alertAppDataFailure } from "../components/alerts/raiseError";

/** The Git settings, in three groups: the repository (where clones go, whether branches are pulled), development (see
 * `Settings.git_workflow` in protocol.generated.ts: where a project session develops, then how it merges and delivers finished
 * work) and protection (the revert gate). Each workflow change sends only its own item. A project may hold its own
 * value for any of the workflow items, which the project dialog sets. */
export function GitSection(): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const listRef = useListMotion<HTMLDivElement>({ fitHeight: true });
  const gitWorkflow = useDaemonStore((s) => s.settings.git_workflow);
  // Another window, or an agent's update, can put development on the current branch while focus is on a row it hides.
  const handoff = useFocusHandoff(() => listRef.current?.querySelector("button")?.focus());

  const change = (changes: GitWorkflowSettingsChanges) =>
    void request({ type: "update_settings", git_workflow: changes }).catch(alertAppDataFailure(t("settings.saveFailedTitle")));

  return (
    <>
      <SettingsGroup title={t("settings.git.group.repository")}>
        <CloneDirSetting />
        <DaemonSwitchSetting field="auto_pull_repositories" />
      </SettingsGroup>
      <SettingsGroup title={t("settings.git.group.development")}>
        <div ref={listRef} className="relative" onFocus={handoff.onFocus} onBlur={handoff.onBlur}>
          <SettingRow
            fill
            motionKey="development"
            label={t(GIT_WORKFLOW_ITEMS.development.label)}
            description={t(DEVELOPMENT_CHOICES.find(({ value }) => value === gitWorkflow.development)!.description)}
          >
            <OptionSelect<GitDevelopment>
              inline
              label={t(GIT_WORKFLOW_ITEMS.development.label)}
              options={DEVELOPMENT_CHOICES.map(({ value, label }) => ({ value, label: t(label) }))}
              value={gitWorkflow.development}
              onChange={(value) => change({ development: value })}
            />
          </SettingRow>
          {/* Squashing, rebase and delivery shape the rule of a session that works on a branch of its own, so they are not shown while
            sessions work on the current branch; their values are kept. */}
          {gitWorkflow.development !== "in_place" && (
            <>
              <SwitchSettingRow
                motionKey="squash_before_delivery"
                label={t(GIT_WORKFLOW_ITEMS.squash_before_delivery.label)}
                description={t(GIT_WORKFLOW_ITEMS.squash_before_delivery.description[gitWorkflow.squash_before_delivery ? "on" : "off"])}
                isSelected={gitWorkflow.squash_before_delivery}
                onChange={(value) => change({ squash_before_delivery: value })}
              />
              <SwitchSettingRow
                motionKey="rebase_before_merge"
                label={t(GIT_WORKFLOW_ITEMS.rebase_before_merge.label)}
                description={t(GIT_WORKFLOW_ITEMS.rebase_before_merge.description[gitWorkflow.rebase_before_merge ? "on" : "off"])}
                isSelected={gitWorkflow.rebase_before_merge}
                onChange={(value) => change({ rebase_before_merge: value })}
              />
              <SettingRow fill motionKey="delivery" label={t(GIT_WORKFLOW_ITEMS.delivery.label)} description={t(GIT_WORKFLOW_ITEMS.delivery.description)}>
                <OptionSelect<GitDelivery>
                  inline
                  label={t(GIT_WORKFLOW_ITEMS.delivery.label)}
                  options={DELIVERY_CHOICES.map(({ value, label }) => ({ value, label: t(label) }))}
                  value={gitWorkflow.delivery}
                  onChange={(value) => change({ delivery: value })}
                />
              </SettingRow>
            </>
          )}
        </div>
      </SettingsGroup>
      <SettingsGroup title={t("settings.git.group.protection")}>
        <SwitchSettingRow
          label={t(GIT_WORKFLOW_ITEMS.revert_gate.label)}
          help={
            <HelpTip label={t("settings.gitWorkflow.revertGate.help")}>
              <Message
                id="settings.gitWorkflow.revertGate.deliberate"
                params={{
                  gitRevert: <InlineReference monospace>git revert</InlineReference>,
                  reverts: <InlineReference monospace>Reverts: &lt;sha&gt;</InlineReference>,
                }}
              />
            </HelpTip>
          }
          description={t(GIT_WORKFLOW_ITEMS.revert_gate.description[gitWorkflow.revert_gate ? "on" : "off"])}
          isSelected={gitWorkflow.revert_gate}
          onChange={(value) => change({ revert_gate: value })}
        />
      </SettingsGroup>
    </>
  );
}
