import React from "react";

import { useShortcutScopeDescription } from "../components/hooks/useShortcutScopeDescription";
import { useT } from "../i18n/react";
import { BOOLEAN_SETTINGS, type BooleanField } from "./booleanSettings";
import type { Inspection, shortcutsReview } from "./importPlan";
import { SettingLine } from "./importParts";
import { shortcutChangeValues } from "./importValues";
import { SettingsCard, type CurrentSettings } from "./SettingsCard";

/** This machine's values of the settings the document carries, in the shape of the document's own. */
function currentSettings(settings: NonNullable<Inspection["settings"]>): CurrentSettings {
  const switches = Object.fromEntries(BOOLEAN_SETTINGS.map(({ field }) => [field, settings[`current_${field}` as const]])) as Record<BooleanField, boolean>;
  return {
    ...switches,
    git_workflow: settings.current_git_workflow,
    provider_settings: settings.current_provider_settings,
    agent_tags: settings.current_agent_tags,
  };
}

/**
 * The application settings of the document, the panel of the review's Application settings tab: a card saying what
 * applying each of them would change here. A line whose setting is made of several items (the git workflow, provider
 * settings, agent tags, keyboard shortcuts) lists, when applying would change it, the items that would change
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
  const shortcutScope = useShortcutScopeDescription();
  const shortcutChanges = shortcuts ? shortcutChangeValues(t, shortcuts.changed) : [];
  return (
    <SettingsCard settings={settings} current={currentSettings(settings)}>
      {shortcuts && (
        shortcutChanges.length === 0
          ? <SettingLine label={t("settings.section.shortcuts")} badge={t("configuration.import.settings.badge.same")} />
          : shortcuts.count === 0
            ? <SettingLine label={t("settings.section.shortcuts")} badge={t("configuration.import.settings.badge.reset")} tone="accent" values={shortcutChanges}>{shortcutScope}</SettingLine>
            : <SettingLine label={t("settings.section.shortcuts")} badge={t("configuration.import.settings.badge.changes")} tone="accent" values={shortcutChanges}>{`${t("configuration.import.settings.consequence.shortcutCount", { count: shortcuts.count })}${t("common.sentenceSeparator")}${shortcutScope}`}</SettingLine>
      )}
    </SettingsCard>
  );
}
