import React from "react";

import type { PlainMessageKey } from "../i18n/catalog";
import { Message, useT } from "../i18n/react";
import { sameValue } from "../sameValue";
import { claudeCompatibilityChanges, hasClaudeCompatibilityChoices, type Inspection, type shortcutsReview } from "./importPlan";
import { BOOLEAN_SETTINGS, CARD_CLASS, SettingLine } from "./importParts";

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

/**
 * The application settings of the document, the panel of the review's Application settings tab: a card saying what
 * applying each of them would change here. `shortcuts` says what the document's keyboard shortcuts would do on this
 * device; `undefined` when it carries none.
 */
export function ImportSettings({
  settings,
  shortcuts,
}: {
  settings: NonNullable<Inspection["settings"]>;
  shortcuts?: ReturnType<typeof shortcutsReview>;
}): React.ReactElement {
  const t = useT();
  return (
    <div className={CARD_CLASS}>
      <ul className="flex flex-col px-4 py-1 [&>li]:groove-b [&>li:last-child]:border-b-0">
        {BOOLEAN_SETTINGS.map(({ label, field, consequence }) => {
          const wanted = settings[field];
          const now = settings[`current_${field}` as const];
          return <BooleanSettingLine key={field} label={t(label)} wanted={wanted} current={now} consequence={consequence} />;
        })}
        {sameValue(settings.git_workflow, settings.current_git_workflow)
          ? <SettingLine label={t("configuration.import.gitWorkflow")} badge={t("configuration.import.settings.badge.same")} />
          : <SettingLine label={t("configuration.import.gitWorkflow")} badge={t("configuration.import.settings.badge.changes")} tone="accent" />}
        {hasClaudeCompatibilityChoices(settings.claude_compatibility) && (
          claudeCompatibilityChanges(settings.claude_compatibility, settings.current_claude_compatibility)
            ? <SettingLine label={t("configuration.import.claudeCompatibility")} badge={t("configuration.import.settings.badge.changes")} tone="accent" />
            : <SettingLine label={t("configuration.import.claudeCompatibility")} badge={t("configuration.import.settings.badge.same")} />
        )}
        {shortcuts && (
          !shortcuts.changes
            ? <SettingLine label={t("settings.section.shortcuts")} badge={t("configuration.import.settings.badge.same")} />
            : shortcuts.count === 0
              ? <SettingLine label={t("settings.section.shortcuts")} badge={t("configuration.import.settings.badge.reset")} tone="accent">{t("configuration.import.settings.consequence.shortcutsReset")}</SettingLine>
              : <SettingLine label={t("settings.section.shortcuts")} badge={t("configuration.import.settings.badge.changes")} tone="accent">{t("configuration.import.settings.consequence.shortcuts", { count: shortcuts.count })}</SettingLine>
        )}
      </ul>
    </div>
  );
}
