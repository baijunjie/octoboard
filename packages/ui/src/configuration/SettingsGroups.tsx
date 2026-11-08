import React from "react";

import { useT } from "../i18n/react";
import type { DocumentSettings, GitWorkflowSettings } from "../protocol";
import { AGENT_BEHAVIOR_SWITCHES, APP_BEHAVIOR_SWITCHES, type ApplicationBooleanField, type BooleanSetting, CLEANUP_SWITCHES, GENERAL_SWITCHES, GIT_SWITCHES } from "./booleanSettings";
import type { ClientSettings, ClientSettingValue } from "./clientSettings";
import { BooleanSettingLine, Card, CardHeading, CompositeLine } from "./importParts";
import { gitWorkflowValues } from "./importValues";

/** This machine's values of the application settings a document carries that the daemon holds, in the shape of the
 * document's own. */
export type CurrentSettings = { [F in ApplicationBooleanField]: boolean } & { git_workflow: GitWorkflowSettings };

/** One group of settings as a card: `title` as its heading, then `lines` (see `SettingLine`) as its rows; nothing when
 * there are none. */
export function SettingsGroupCard({ title, lines }: { title: string; lines: React.ReactNode[] }): React.ReactElement | null {
  if (lines.length === 0) return null;
  return (
    <Card header={(headingId) => <CardHeading id={headingId}>{title}</CardHeading>}>
      <ul className="flex flex-col px-4 py-1 [&>li]:groove-b [&>li:last-child]:border-b-0">{lines}</ul>
    </Card>
  );
}

/**
 * The application settings of a configuration as cards grouped the way the Settings dialog groups them: General, App
 * behavior, Git, Agent behavior, Keyboard shortcuts and Cleanup, each titled with its section's name, in the order of
 * the sections and, within a group, of the lines that section shows. With `current` (this machine's values) it is the
 * review's: each line says what applying `settings` would change, and the git workflow lists the items that would
 * change with what each is now. Without it, it is the result's: each line says what `settings` were applied as, the
 * git workflow listing its values. The daemon holds the switches and the git workflow; the lines of what only the
 * client holds or carries are the caller's: `clientLine` words one of `clientSettings` (appearance, language, Markdown
 * style, run in background, menu bar icon, prevent sleep and when), `cloneDir` is the default clone directory's line
 * and `shortcuts` the keyboard shortcuts', each absent when the document carries none. A group with no line is not
 * drawn.
 */
export function SettingsGroups({
  settings,
  current,
  clientSettings,
  clientLine,
  cloneDir,
  shortcuts,
}: {
  settings: DocumentSettings;
  current?: CurrentSettings;
  clientSettings: ClientSettingValue[];
  clientLine: (setting: ClientSettingValue) => React.ReactElement;
  cloneDir?: React.ReactElement;
  shortcuts?: React.ReactElement;
}): React.ReactElement {
  const t = useT();
  const comparing = current !== undefined;
  const switchLines = (switches: readonly BooleanSetting<ApplicationBooleanField>[]) =>
    // A setting `settings` lacks (a document from an older build, a reply from an older daemon) has no line.
    switches.flatMap(({ label, field, consequence }) => {
      const wanted = settings[field];
      return wanted == null ? [] : [<BooleanSettingLine key={field} label={t(label)} wanted={wanted} current={current?.[field]} consequence={consequence} />];
    });
  const appBehaviorSwitches = (...fields: ApplicationBooleanField[]) => switchLines(APP_BEHAVIOR_SWITCHES.filter(({ field }) => fields.includes(field)));
  const clientLines = (...fields: (keyof ClientSettings)[]) =>
    clientSettings.filter(({ field }) => fields.includes(field)).map((setting) => React.cloneElement(clientLine(setting), { key: setting.field }));
  return (
    <>
      <SettingsGroupCard
        title={t("settings.section.general")}
        lines={[...clientLines("appearance", "interface_language", "markdown_style"), ...switchLines(GENERAL_SWITCHES)]}
      />
      <SettingsGroupCard
        title={t("settings.section.appBehavior")}
        lines={[
          ...appBehaviorSwitches("resume_sessions_on_start"),
          ...clientLines("run_in_background", "show_menu_bar_icon", "keep_awake", "keep_awake_when"),
          ...appBehaviorSwitches("auto_close_idle_shell_terminals"),
        ]}
      />
      <SettingsGroupCard
        title={t("settings.section.git")}
        lines={[
          ...(cloneDir ? [React.cloneElement(cloneDir, { key: "clone-dir" })] : []),
          ...switchLines(GIT_SWITCHES),
          <CompositeLine key="git-workflow" label={t("configuration.import.gitWorkflow")} values={gitWorkflowValues(t, settings.git_workflow, current?.git_workflow)} comparing={comparing} />,
        ]}
      />
      <SettingsGroupCard title={t("settings.section.agentPermissions")} lines={switchLines(AGENT_BEHAVIOR_SWITCHES)} />
      <SettingsGroupCard title={t("settings.section.shortcuts")} lines={shortcuts ? [React.cloneElement(shortcuts, { key: "shortcuts" })] : []} />
      <SettingsGroupCard title={t("settings.section.cleanup")} lines={switchLines(CLEANUP_SWITCHES)} />
    </>
  );
}
