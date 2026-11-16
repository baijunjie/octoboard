import React from "react";

import { Message, useT } from "../i18n/react";
import { isKeepAwakeWhen, KEEP_AWAKE_WHEN_LABEL } from "./keepAwakeWhen";
import type { DocumentSettings, GitWorkflowSettings, KeepAwakeWhen, SettingsSection } from "../protocol";
import { AGENT_BEHAVIOR_SWITCHES, type ApplicationBooleanField, type BooleanSetting, CLEANUP_SWITCHES, GENERAL_SWITCHES, GIT_SWITCHES } from "./booleanSettings";
import type { ClientSettingValue } from "./clientSettings";
import { BooleanSettingLine, choiceHeader, CompositeLine, type ImportChoice, SettingLine } from "./importParts";
import { FlatCard } from "../components/card/FlatCard";
import { gitWorkflowValues } from "./importValues";
import { SETTINGS_SECTIONS } from "./settingsSections";

/** The application settings the cards show: the ones the daemon holds, each optional as a document carries them. */
type GroupedSettings = Pick<DocumentSettings, ApplicationBooleanField | "keep_awake_when" | "git_workflow">;

/** This machine's values of the application settings a document carries that the daemon holds, in the shape of the
 * document's own. */
export type CurrentSettings = { [F in ApplicationBooleanField]: boolean } & { keep_awake_when: KeepAwakeWhen; git_workflow: GitWorkflowSettings };

/** One group of settings as a card: `title` as its heading, then `lines` (see `SettingLine`) as its rows; nothing when
 * there are none. With a `choice` the heading carries a checkbox named by the title (see `choiceHeader`), the card stays
 * when it has no rows, and a section that is not imported collapses to its heading row, marked as left out. */
export function SettingsGroupCard({ title, lines, choice }: { title: string; lines: React.ReactNode[]; choice?: ImportChoice }): React.ReactElement | null {
  if (!choice && lines.length === 0) return null;
  return (
    <FlatCard>
      {choice ? choiceHeader(title, choice) : (
        <FlatCard.Header>
          <FlatCard.Heading>{title}</FlatCard.Heading>
        </FlatCard.Header>
      )}
      {(!choice || choice.on) && lines.length > 0 && <FlatCard.Rows padded>{lines}</FlatCard.Rows>}
    </FlatCard>
  );
}

/**
 * The application settings of a configuration as cards grouped the way the Settings dialog groups them: General,
 * Appearance, Agent behavior, Git, Keyboard shortcuts and Cleanup, each titled with its section's name, in the order
 * of the sections and, within a group, of the lines that section shows. With `current` (this machine's values) it is the
 * review's: each line says what applying `settings` would change, and the git workflow lists the items that would
 * change with what each is now. Without it, it is the result's: each line says what `settings` were applied as, the git
 * workflow listing its values. The daemon holds the switches, when Prevent sleep holds and the git workflow; the lines
 * of what only the client holds or carries are the caller's: `clientLine` words one of `clientSettings` (appearance,
 * language, which the daemon holds but is worded the same way, Markdown style, terminal font size, run in background,
 * menu bar icon),
 * `sorting` is the session order's line, `cloneDir` is the default clone directory's line and `shortcuts` the keyboard
 * shortcuts', each absent when the document carries none. Only the `sections` given are drawn (all of them when
 * absent), and a group with no line is not drawn: what `settings` lacks has no line. With `choices` (the review's)
 * every section drawn is a card with a checkbox for importing it, drawn even with no line and collapsed while unchecked
 * (see `SettingsGroupCard`).
 */
export function SettingsGroups({
  settings,
  current,
  clientSettings,
  clientLine,
  sorting,
  cloneDir,
  shortcuts,
  sections = SETTINGS_SECTIONS,
  choices,
}: {
  settings: GroupedSettings;
  current?: CurrentSettings;
  clientSettings: ClientSettingValue[];
  clientLine: (setting: ClientSettingValue) => React.ReactElement;
  sorting?: React.ReactElement;
  cloneDir?: React.ReactElement;
  shortcuts?: React.ReactElement;
  sections?: readonly SettingsSection[];
  choices?: Record<SettingsSection, ImportChoice>;
}): React.ReactElement {
  const t = useT();
  const comparing = current !== undefined;
  const switchLines = (switches: readonly BooleanSetting<ApplicationBooleanField>[]) =>
    // A setting `settings` lacks (a document from an older build, a reply from an older daemon) has no line.
    switches.flatMap(({ label, field, consequence }) => {
      const wanted = settings[field];
      return wanted == null ? [] : [<BooleanSettingLine key={field} label={t(label)} wanted={wanted} current={current?.[field]} consequence={consequence} />];
    });
  // When Prevent sleep holds is a choice of two, worded as the switches are; a value this build does not know has no line.
  const keepAwakeWhenLine = () => {
    const wanted = settings.keep_awake_when;
    if (!isKeepAwakeWhen(wanted)) return [];
    const label = t("settings.keepAwake.when.label");
    const now = current?.keep_awake_when;
    if (now === undefined) return [<SettingLine key="keep_awake_when" label={label} badge={t(KEEP_AWAKE_WHEN_LABEL[wanted])} />];
    if (wanted === now) return [<SettingLine key="keep_awake_when" label={label} badge={t("configuration.import.settings.badge.same")} />];
    return [
      <SettingLine key="keep_awake_when" label={label} badge={t("configuration.import.settings.badge.changes")} tone="accent">
        <Message id="configuration.import.settingValueChange" params={{ value: t(KEEP_AWAKE_WHEN_LABEL[wanted]), current: t(KEEP_AWAKE_WHEN_LABEL[now]) }} />
      </SettingLine>,
    ];
  };
  const generalSwitches = (...fields: ApplicationBooleanField[]) => switchLines(GENERAL_SWITCHES.filter(({ field }) => fields.includes(field)));
  const clientLines = (...fields: ClientSettingValue["field"][]) =>
    fields.flatMap((wanted) => clientSettings.filter(({ field }) => field === wanted).map((setting) => React.cloneElement(clientLine(setting), { key: setting.field })));
  const cards: Record<SettingsSection, React.ReactElement> = {
    general: (
      <SettingsGroupCard
        choice={choices?.general}
        title={t("settings.section.general")}
        lines={[
          ...clientLines("interface_language"),
          ...generalSwitches("resume_sessions_on_start"),
          ...clientLines("run_in_background", "show_menu_bar_icon"),
          ...generalSwitches("keep_awake"),
          ...keepAwakeWhenLine(),
          ...generalSwitches("auto_close_idle_shell_terminals"),
          ...(sorting ? [React.cloneElement(sorting, { key: "sorting" })] : []),
        ]}
      />
    ),
    appearance: <SettingsGroupCard choice={choices?.appearance} title={t("settings.section.appearance")} lines={clientLines("appearance", "markdown_style", "terminal_font_size")} />,
    git: (
      <SettingsGroupCard
        choice={choices?.git}
        title={t("settings.section.git")}
        lines={[
          ...(cloneDir ? [React.cloneElement(cloneDir, { key: "clone-dir" })] : []),
          ...switchLines(GIT_SWITCHES),
          ...(settings.git_workflow
            ? [<CompositeLine key="git-workflow" label={t("configuration.import.gitWorkflow")} values={gitWorkflowValues(t, settings.git_workflow, current?.git_workflow)} comparing={comparing} />]
            : []),
        ]}
      />
    ),
    agent_behavior: <SettingsGroupCard choice={choices?.agent_behavior} title={t("settings.section.agentPermissions")} lines={switchLines(AGENT_BEHAVIOR_SWITCHES)} />,
    keyboard_shortcuts: <SettingsGroupCard choice={choices?.keyboard_shortcuts} title={t("settings.section.shortcuts")} lines={shortcuts ? [React.cloneElement(shortcuts, { key: "shortcuts" })] : []} />,
    cleanup: <SettingsGroupCard choice={choices?.cleanup} title={t("settings.section.cleanup")} lines={switchLines(CLEANUP_SWITCHES)} />,
  };
  return <>{SETTINGS_SECTIONS.filter((section) => sections.includes(section)).map((section) => <React.Fragment key={section}>{cards[section]}</React.Fragment>)}</>;
}
