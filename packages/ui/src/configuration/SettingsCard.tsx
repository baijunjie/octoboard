import React from "react";

import type { PlainMessageKey } from "../i18n/catalog";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { DocumentSettings } from "../protocol";
import { BOOLEAN_SETTINGS } from "./booleanSettings";
import { hasAgentTags, hasClaudeCompatibilityChoices } from "./importPlan";
import { Card, CARD_CLASS, CardHeading, SettingLine } from "./importParts";
import { agentTagsValues, claudeCompatibilityValues, gitWorkflowValues, type SettingValue } from "./importValues";

/** A boolean application setting: what applying it would do against `current` (turns it on, off, or leaves it as it
 * is), or, with no `current`, what it was set to. */
function BooleanSettingLine({ label, wanted, current, consequence }: { label: string; wanted: boolean; current: boolean | undefined; consequence: PlainMessageKey }): React.ReactElement {
  const t = useT();
  if (current === undefined) return <SettingLine label={label} badge={t(wanted ? "configuration.import.result.badge.on" : "configuration.import.result.badge.off")} />;
  if (wanted === current) return <SettingLine label={label} badge={t("configuration.import.settings.badge.same")} />;
  return wanted ? (
    <SettingLine label={label} badge={t("configuration.import.settings.badge.turnsOn")} tone="accent">
      <Message id={consequence} />
    </SettingLine>
  ) : (
    <SettingLine label={label} badge={t("configuration.import.settings.badge.turnsOff")} />
  );
}

/** A setting made of several items: against `current`, listing those that would change or "No change" where none
 * would; with none, listing the values it was set to. */
function CompositeLine({ label, values, comparing }: { label: string; values: SettingValue[]; comparing: boolean }): React.ReactElement {
  const t = useT();
  if (!comparing) return <SettingLine label={label} badge={t("configuration.import.result.badge.matches")} values={values} />;
  return values.length === 0
    ? <SettingLine label={label} badge={t("configuration.import.settings.badge.same")} />
    : <SettingLine label={label} badge={t("configuration.import.settings.badge.changes")} tone="accent" values={values} />;
}

/**
 * The application settings of a configuration, as one card of lines: the switches, then the git workflow, Claude
 * compatibility and agent tags. With `current` (this machine's values) it is the review's: each line says what applying
 * `settings` would change, and a line made of several items lists those that would change with what each is now. Without
 * it, it is the result's: each line says what `settings` were applied as, listing the values of those made of several
 * items. `heading` titles the card, which the result gives; `children` are lines the caller adds at its end (the keyboard
 * shortcuts, which the daemon only carries through).
 */
export function SettingsCard({
  settings,
  current,
  heading,
  children,
}: {
  settings: DocumentSettings;
  current?: DocumentSettings;
  heading?: string;
  children?: React.ReactNode;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const comparing = current !== undefined;
  const lines = (
    <ul className="flex flex-col px-4 py-1 [&>li]:groove-b [&>li:last-child]:border-b-0">
      {BOOLEAN_SETTINGS.map(({ label, field, consequence }) => (
        <BooleanSettingLine key={field} label={t(label)} wanted={settings[field]} current={current?.[field]} consequence={consequence} />
      ))}
      <CompositeLine label={t("configuration.import.gitWorkflow")} values={gitWorkflowValues(t, settings.git_workflow, current?.git_workflow)} comparing={comparing} />
      {hasClaudeCompatibilityChoices(settings.claude_compatibility) && (
        <CompositeLine label={t("configuration.import.claudeCompatibility")} values={claudeCompatibilityValues(t, settings.claude_compatibility, current?.claude_compatibility)} comparing={comparing} />
      )}
      {hasAgentTags(settings.agent_tags) && (
        <CompositeLine label={t("configuration.import.agentTags")} values={agentTagsValues(t, language, settings.agent_tags, current?.agent_tags)} comparing={comparing} />
      )}
      {children}
    </ul>
  );
  if (heading === undefined) return <div className={CARD_CLASS}>{lines}</div>;
  return <Card header={(headingId) => <CardHeading id={headingId}>{heading}</CardHeading>}>{lines}</Card>;
}
