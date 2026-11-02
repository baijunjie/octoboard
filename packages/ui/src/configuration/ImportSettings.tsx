import React from "react";

import type { PlainMessageKey } from "../i18n/catalog";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import { BOOLEAN_SETTINGS } from "./booleanSettings";
import { hasAgentTags, hasClaudeCompatibilityChoices, type Inspection, type shortcutsReview } from "./importPlan";
import { CARD_CLASS, SettingLine } from "./importParts";
import { agentTagsValues, claudeCompatibilityValues, gitWorkflowValues, shortcutChangeValues, type SettingValue } from "./importValues";

/** A boolean application setting: whether applying turns it on, off, or leaves it as it is. */
function BooleanSettingLine({ label, wanted, current, consequence }: { label: string; wanted: boolean; current: boolean; consequence: PlainMessageKey }): React.ReactElement {
  const t = useT();
  if (wanted === current) return <SettingLine label={label} badge={t("configuration.import.settings.badge.same")} />;
  return wanted ? (
    <SettingLine label={label} badge={t("configuration.import.settings.badge.turnsOn")} tone="accent">
      <Message id={consequence} />
    </SettingLine>
  ) : (
    <SettingLine label={label} badge={t("configuration.import.settings.badge.turnsOff")} />
  );
}

/** A setting made of several items: listing those that would change, or "No change" where none would. */
function CompositeLine({ label, values }: { label: string; values: SettingValue[] }): React.ReactElement {
  const t = useT();
  return values.length === 0
    ? <SettingLine label={label} badge={t("configuration.import.settings.badge.same")} />
    : <SettingLine label={label} badge={t("configuration.import.settings.badge.changes")} tone="accent" values={values} />;
}

/**
 * The application settings of the document, the panel of the review's Application settings tab: a card saying what
 * applying each of them would change here. A line whose setting is made of several items (the git workflow, Claude
 * compatibility, agent tags, keyboard shortcuts) lists, when applying would change it, the items that would change
 * with what each is now. `shortcuts` says what the document's keyboard shortcuts would do on this device;
 * `undefined` when it carries none.
 */
export function ImportSettings({
  settings,
  shortcuts,
}: {
  settings: NonNullable<Inspection["settings"]>;
  shortcuts?: ReturnType<typeof shortcutsReview>;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  // What changes is what each line lists: a line with nothing to list reads "No change".
  const gitWorkflow = gitWorkflowValues(t, settings.git_workflow, settings.current_git_workflow);
  const claudeCompatibility = claudeCompatibilityValues(t, settings.claude_compatibility, settings.current_claude_compatibility);
  const agentTags = agentTagsValues(t, language, settings.agent_tags, settings.current_agent_tags);
  const shortcutChanges = shortcuts ? shortcutChangeValues(t, shortcuts.changed) : [];
  return (
    <div className={CARD_CLASS}>
      <ul className="flex flex-col px-4 py-1 [&>li]:groove-b [&>li:last-child]:border-b-0">
        {BOOLEAN_SETTINGS.map(({ label, field, consequence }) => {
          const wanted = settings[field];
          const now = settings[`current_${field}` as const];
          return <BooleanSettingLine key={field} label={t(label)} wanted={wanted} current={now} consequence={consequence} />;
        })}
        <CompositeLine label={t("configuration.import.gitWorkflow")} values={gitWorkflow} />
        {hasClaudeCompatibilityChoices(settings.claude_compatibility) && <CompositeLine label={t("configuration.import.claudeCompatibility")} values={claudeCompatibility} />}
        {hasAgentTags(settings.agent_tags) && <CompositeLine label={t("configuration.import.agentTags")} values={agentTags} />}
        {shortcuts && (
          shortcutChanges.length === 0
            ? <SettingLine label={t("settings.section.shortcuts")} badge={t("configuration.import.settings.badge.same")} />
            : shortcuts.count === 0
              ? <SettingLine label={t("settings.section.shortcuts")} badge={t("configuration.import.settings.badge.reset")} tone="accent" values={shortcutChanges}>{t("configuration.import.settings.consequence.shortcutsReset")}</SettingLine>
              : <SettingLine label={t("settings.section.shortcuts")} badge={t("configuration.import.settings.badge.changes")} tone="accent" values={shortcutChanges}>{t("configuration.import.settings.consequence.shortcuts", { count: shortcuts.count })}</SettingLine>
        )}
      </ul>
    </div>
  );
}
