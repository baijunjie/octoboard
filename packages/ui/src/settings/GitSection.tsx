import React from "react";

import { HelpTip } from "../components/HelpTip";
import { InlineReference } from "../components/InlineReference";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { useListMotion } from "../components/useListMotion";
import { OptionSelect } from "../dialogs/OptionSelect";
import { errorPresentation } from "../errorPresentation";
import { DELIVERY_CHOICES, DEVELOPMENT_CHOICES } from "../gitWorkflow";
import { Message, useT } from "../i18n/react";
import type { GitDelivery, GitDevelopment, GitWorkflowSettingsChanges } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { CloneDirSetting } from "./CloneDirSetting";
import { SettingRow, SwitchSettingRow } from "./SettingRow";
import { SettingsGroup } from "./SettingsGroup";

/** The Git settings, in three groups: the repository (where clones go, whether branches are pulled), development (see
 * `Settings.git_workflow` in protocol.ts: where a project session develops, then how it merges and delivers finished
 * work) and protection (the revert gate). Each workflow change sends only its own item. A project may hold its own
 * value for any of the workflow items, which the project dialog sets. */
export function GitSection(): React.ReactElement {
  const t = useT();
  const { request, toastError } = useDaemon();
  const listRef = useListMotion<HTMLDivElement>({ fitHeight: true });
  const autoPull = useDaemonStore((s) => s.settings.auto_pull_repositories);
  const gitWorkflow = useDaemonStore((s) => s.settings.git_workflow);
  // Another window, or an agent's update, can put development on the current branch while focus is on a row it hides.
  const handoff = useFocusHandoff(() => listRef.current?.querySelector("button")?.focus());

  const setAutoPull = (value: boolean) =>
    void request({ type: "update_settings", auto_pull_repositories: value }).catch((err) => toastError(errorPresentation(err as Error)));
  const change = (changes: GitWorkflowSettingsChanges) =>
    void request({ type: "update_settings", git_workflow: changes }).catch((err) => toastError(errorPresentation(err as Error)));

  return (
    <>
      <SettingsGroup title={t("settings.git.group.repository")}>
        <CloneDirSetting />
        <SwitchSettingRow
          label={t("settings.git.autoPull.label")}
          help={<HelpTip label={t("settings.git.autoPull.help")}>{t("settings.git.autoPull.remoteCheck")}</HelpTip>}
          description={t("settings.git.autoPull.description")}
          isSelected={autoPull}
          onChange={setAutoPull}
        />
      </SettingsGroup>
      <SettingsGroup title={t("settings.git.group.development")}>
        <div ref={listRef} className="relative" onFocus={handoff.onFocus} onBlur={handoff.onBlur}>
          <SettingRow
            motionKey="development"
            label={t("settings.gitWorkflow.development.label")}
            description={t(DEVELOPMENT_CHOICES.find(({ value }) => value === gitWorkflow.development)!.description)}
          >
            <OptionSelect<GitDevelopment>
              inline
              label={t("settings.gitWorkflow.development.label")}
              options={DEVELOPMENT_CHOICES.map(({ value, label }) => ({ value, label: t(label) }))}
              value={gitWorkflow.development}
              onChange={(value) => change({ development: value })}
            />
          </SettingRow>
          {/* Rebase and delivery shape the rule of a session that works on a branch of its own, so they are not shown while
            sessions work on the current branch; their values are kept. */}
          {gitWorkflow.development !== "in_place" && (
            <>
              <SwitchSettingRow
                motionKey="rebase_before_merge"
                label={t("settings.gitWorkflow.rebase.label")}
                description={t(gitWorkflow.rebase_before_merge ? "settings.gitWorkflow.rebase.description.on" : "settings.gitWorkflow.rebase.description.off")}
                isSelected={gitWorkflow.rebase_before_merge}
                onChange={(value) => change({ rebase_before_merge: value })}
              />
              <SettingRow motionKey="delivery" label={t("settings.gitWorkflow.delivery.label")} description={t("settings.gitWorkflow.delivery.description")}>
                <OptionSelect<GitDelivery>
                  inline
                  label={t("settings.gitWorkflow.delivery.label")}
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
          label={t("settings.gitWorkflow.revertGate.label")}
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
          description={t(gitWorkflow.revert_gate ? "settings.gitWorkflow.revertGate.description.on" : "settings.gitWorkflow.revertGate.description.off")}
          isSelected={gitWorkflow.revert_gate}
          onChange={(value) => change({ revert_gate: value })}
        />
      </SettingsGroup>
    </>
  );
}
