import React from "react";

import { useShortcutScopeDescription } from "../components/hooks/useShortcutScopeDescription";
import { InlineReference } from "../i18n/InlineReference";
import { Message, useT } from "../i18n/react";
import { APPLICATION_SWITCHES } from "./booleanSettings";
import type { ClientSettingValue } from "./clientSettings";
import type { Inspection, shortcutsReview } from "./importPlan";
import { ClientSettingLine, SettingLine } from "./importParts";
import { shortcutChangeValues } from "./importValues";
import { SettingsGroups, type CurrentSettings } from "./SettingsGroups";
import { Checkbox } from "../primitives";

/** This machine's values of the application settings the document carries that the daemon holds, in the shape of the
 * document's own. */
function currentSettings(settings: NonNullable<Inspection["settings"]>): CurrentSettings {
  const switches = Object.fromEntries(APPLICATION_SWITCHES.map(({ field }) => [field, settings[`current_${field}` as const]])) as Pick<CurrentSettings, (typeof APPLICATION_SWITCHES)[number]["field"]>;
  return { ...switches, git_workflow: settings.current_git_workflow };
}

/** The default clone directory of the file, with the directory this machine has where they differ and, there, the choice
 * to keep this machine's own: `use` is whether the file's is applied. */
function CloneDirLine({
  dir,
  current,
  use,
  onUse,
}: {
  dir: string;
  current: string;
  use: boolean;
  onUse: (use: boolean) => void;
}): React.ReactElement {
  const t = useT();
  if (dir === current) return <SettingLine label={t("settings.cloneDir.label")} badge={t("configuration.import.settings.badge.same")} />;
  const fileDir = <InlineReference monospace wrap>{dir}</InlineReference>;
  const currentDir = <InlineReference monospace wrap>{current}</InlineReference>;
  return (
    <SettingLine label={t("settings.cloneDir.label")} badge={t(use ? "configuration.import.settings.badge.changes" : "configuration.import.settings.badge.same")} tone={use ? "accent" : "default"}>
      <div className="flex flex-col gap-1">
        <div>
          {use
            ? <Message id="configuration.import.settingValueChange" params={{ value: fileDir, current: currentDir }} />
            : <Message id="configuration.import.cloneDir.kept" params={{ value: fileDir, current: currentDir }} />}
        </div>
        <Checkbox variant="secondary" selected={use} onSelectedChange={onUse} contentClassName="min-h-6" labelClassName="text-foreground">
          {t("configuration.import.cloneDir.use")}
        </Checkbox>
      </div>
    </SettingLine>
  );
}

/**
 * The application settings of the document, the panel of the review's Application settings tab: cards grouped as the
 * Settings dialog groups them (see `SettingsGroups`), saying what applying each setting would change here. The git
 * workflow lists, when applying would change it, the items that would change with what each is now. The General and
 * App behavior cards take the settings only this client holds (`clientSettings`, each with the value this client has),
 * the Git card the default clone directory with the choice to keep this machine's own (`cloneDir`; for a directory
 * written on another operating system, only a line saying it is not used here) and the Keyboard shortcuts card
 * `shortcuts`, what the document's keyboard shortcuts would do on this device; each is absent when the document carries
 * none.
 */
export function ImportSettings({
  settings,
  clientSettings,
  shortcuts,
  cloneDir,
}: {
  settings: NonNullable<Inspection["settings"]>;
  clientSettings: ClientSettingValue[];
  shortcuts?: ReturnType<typeof shortcutsReview>;
  /** Whether the file's default clone directory is applied, and the change of that choice. */
  cloneDir: { use: boolean; onUse: (use: boolean) => void };
}): React.ReactElement {
  const t = useT();
  const shortcutScope = useShortcutScopeDescription();
  const shortcutChanges = shortcuts ? shortcutChangeValues(t, shortcuts.changed) : [];
  return (
    <SettingsGroups
      settings={settings}
      current={currentSettings(settings)}
      clientSettings={clientSettings}
      clientLine={(setting) => <ClientSettingLine setting={setting} />}
      cloneDir={
        settings.default_clone_dir !== null
          ? <CloneDirLine dir={settings.default_clone_dir} current={settings.current_default_clone_dir} use={cloneDir.use} onUse={cloneDir.onUse} />
          : settings.foreign_default_clone_dir
            ? <SettingLine label={t("settings.cloneDir.label")} badge={t("configuration.import.settings.badge.same")}>{t("configuration.import.cloneDir.foreign")}</SettingLine>
            : undefined
      }
      shortcuts={
        shortcuts &&
        (shortcutChanges.length === 0
          ? <SettingLine label={t("configuration.import.settings.shortcutsLine")} badge={t("configuration.import.settings.badge.same")} />
          : shortcuts.count === 0
            ? <SettingLine label={t("configuration.import.settings.shortcutsLine")} badge={t("configuration.import.settings.badge.reset")} tone="accent" values={shortcutChanges}>{shortcutScope}</SettingLine>
            : <SettingLine label={t("configuration.import.settings.shortcutsLine")} badge={t("configuration.import.settings.badge.changes")} tone="accent" values={shortcutChanges}>{`${t("configuration.import.settings.consequence.shortcutCount", { count: shortcuts.count })}${t("common.sentenceSeparator")}${shortcutScope}`}</SettingLine>)
      }
    />
  );
}
