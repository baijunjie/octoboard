import React, { useId, useState } from "react";

import { appConfig } from "../appConfig";
import { ChoiceWarning } from "../components/alerts/ChoiceWarning";
import { InlineReference } from "../i18n/InlineReference";
import { ConfirmOption } from "../components/form/ConfirmOption";
import { Dialog } from "../components/dialog/Dialog";
import { useDialogAction } from "../components/dialog/dialogHooks";
import { LabelledGroup } from "../components/form/LabelledGroup";
import { isolate } from "../i18n/isolate";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { useOctoboardTheme } from "../preferences/theme";
import type { Console, ExportedConsoles, SettingsSection } from "../protocol";
import type { PlainMessageKey } from "../i18n/catalog";
import { shortcutOverrides } from "../shortcuts/store";
import { useDaemon } from "../store";
import { clientSettingsPlatform, heldClientSettings } from "./clientSettings";
import { SECTION_LABEL, SETTINGS_SECTIONS } from "./settingsSections";
import { fileFailure } from "./failureText";
import { Radio, RadioGroup } from "../primitives";

/** The name the save dialog offers: the application's, and for one console that console's, with
 * what a file name cannot hold taken out. */
export function exportFileName(console?: Pick<Console, "name">): string {
  const base = appConfig.name.toLowerCase();
  // eslint-disable-next-line no-control-regex -- File names cannot contain control characters.
  const safe = console?.name.replace(/[\u0000-\u001f\u007f/\\:*?"<>|]/g, " ").replace(/\s+/g, " ").trim();
  return safe ? `${base}-${safe}.json` : `${base}-configuration.json`;
}

const SECTION_NOTE: Record<SettingsSection, PlainMessageKey> = {
  general: "configuration.export.general.description",
  app_behavior: "configuration.export.appBehavior.description",
  git: "configuration.export.git.description",
  agent_behavior: "configuration.export.agentBehavior.description",
  keyboard_shortcuts: "configuration.export.shortcuts.description",
  cleanup: "configuration.export.cleanup.description",
};

/**
 * Exports configuration to a file the user picks the place of. Its regions are chosen independently: the consoles and
 * projects (the console in hand, `console`, absent when there is none; every console; or none), under a Settings group
 * the settings of each Settings section that carries something, and, under an Agent accounts group of their own, the
 * agent accounts with their agent account settings, a set of records the import reviews one by one rather than
 * settings; none of the sections or the accounts by default. Full project paths of the daemon's host are left out
 * unless asked for, and while they are asked for a warning Alert under that choice says what sharing them exposes, so
 * it is read before the document exists. The choice is about project paths alone, whatever else is exported (the
 * accounts' directories are always in the file), so it is not shown while no console is exported, as there is no path
 * to record, and then it is not sent; and Export is unavailable while nothing at all is chosen. The daemon builds the
 * document; this saves it as the daemon sent it, through the platform's save dialog, and says what became of it: the
 * name it was saved under, or only that it was downloaded where the platform cannot say more. What is added are the
 * settings this client holds, which the daemon does not know of: a document that includes the Keyboard shortcuts
 * section gets this client's shortcut changes (an empty set when none is changed), and with the section they belong to
 * its appearance, Markdown style, session order and the language it shows in place of the one the daemon holds
 * (General) and, where the platform has it, whether it runs in the background, whether it shows its menu bar icon and
 * whether it prevents sleep, and when (App behavior; see `heldClientSettings`). Dismissing the save dialog leaves this
 * one open.
 */
export function ExportConfigurationDialog({
  console: inHand,
  onClose,
}: {
  console?: Console;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const { request, showNotice } = useDaemon();
  const platform = usePlatform();
  const { configurationFile, backgroundRun, statusItem, keepAwake } = platform;
  const { choice: appearance } = useOctoboardTheme();
  const language = useCurrentLanguage();
  const { busy, run } = useDialogAction(undefined, t("configuration.export.alertTitle"));
  const [consoles, setConsoles] = useState<ExportedConsoles>(inHand ? "one" : "all");
  const [includeAccounts, setIncludeAccounts] = useState(false);
  const [included, setIncluded] = useState<Record<SettingsSection, boolean>>({
    general: false,
    app_behavior: false,
    git: false,
    agent_behavior: false,
    keyboard_shortcuts: false,
    cleanup: false,
  });
  const [includePaths, setIncludePaths] = useState(false);
  const warningId = useId();
  const exported = consoles === "one" ? inHand : undefined;
  const sections = SETTINGS_SECTIONS.filter((section) => included[section]);
  // The App behavior note lists the settings this client can hold only where the platform has all of them.
  const heldHere = backgroundRun !== undefined && statusItem !== undefined && keepAwake !== undefined;
  const nothing = consoles === "none" && sections.length === 0 && !includeAccounts;
  // A project's path is the only one the choice is about: with no console exported the option is not shown.
  const pathsApply = consoles !== "none";
  const pathsIncluded = includePaths && pathsApply;

  const noteOf = (section: SettingsSection): PlainMessageKey =>
    section === "app_behavior" && !heldHere ? "configuration.export.appBehavior.description.basic" : SECTION_NOTE[section];

  const submit = () => {
    if (!configurationFile || busy || nothing) return;
    void run(async () => {
      const reply = await request({
        type: "export_configuration",
        console: exported?.id,
        consoles,
        settings_sections: sections,
        include_accounts: includeAccounts,
        ...(pathsApply && { include_paths: pathsIncluded }),
      });
      // The daemon knows nothing of this client's own settings: a document with the settings gets them here, for the
      // sections that were chosen.
      const file = reply.document.settings
        ? {
          ...reply.document,
          settings: {
            ...reply.document.settings,
            ...(sections.includes("keyboard_shortcuts") && { keyboard_shortcuts: shortcutOverrides.get() }),
            ...heldClientSettings({ appearance, language }, clientSettingsPlatform(platform), sections),
          },
        }
        : reply.document;
      let saved;
      try {
        saved = await configurationFile.save(exportFileName(exported), `${JSON.stringify(file, null, 2)}\n`);
      } catch (err) {
        throw fileFailure(t, err, "configuration.export.couldNotSave");
      }
      if (saved === undefined) return;
      showNotice(
        saved.kind === "saved" ? t("configuration.export.saved", { name: isolate(saved.name) }) : t("configuration.export.downloaded"),
        saved.kind === "saved" ? <Message id="configuration.export.saved" params={{ name: <InlineReference monospace wrap>{saved.name}</InlineReference> }} /> : undefined,
      );
      onClose();
    });
  };

  return (
    <Dialog
      title={t("configuration.export.title")}
      onClose={onClose}
      submitLabel={t("configuration.export.submit")}
      busy={busy}
      submitDisabled={nothing}
      autoFocusSubmit
      onSubmit={submit}
    >
      <RadioGroup variant="secondary" label={t("configuration.export.scope")} value={consoles} onChange={(value) => setConsoles(value as ExportedConsoles)}>
        {inHand && (
          <Radio value="one">
            <Message id="configuration.export.scope.console" params={{ name: <InlineReference dir="auto" wrap>{inHand.name}</InlineReference> }} />
          </Radio>
        )}
        <Radio value="all">{t("configuration.export.scope.all")}</Radio>
        <Radio value="none">{t("configuration.export.scope.none")}</Radio>
      </RadioGroup>
      {pathsApply && (
        <ConfirmOption
          isSelected={includePaths}
          onChange={setIncludePaths}
          label={t("configuration.export.paths.include")}
          hint={t(includePaths ? "configuration.export.paths.include.description.on" : "configuration.export.paths.include.description.off")}
          describedBy={includePaths ? warningId : undefined}
        />
      )}
      {pathsIncluded && (
        <ChoiceWarning
          id={warningId}
          title={t("configuration.export.paths.title")}
          remedy={t("configuration.export.paths.remedy")}
        >
          <Message id="configuration.export.paths.privacy" />
        </ChoiceWarning>
      )}
      {/* The body spaces its children 16px apart; a group starts 24px after what precedes it, so it reads apart from the
          options of the one before. */}
      <div className="mt-2">
        <LabelledGroup label={t("settings.title")}>
          {/* 16px from the label and between the options, as the radio group above spaces its own. */}
          <div className="mt-2.5 flex flex-col gap-4">
            {SETTINGS_SECTIONS.map((section) => (
              <ConfirmOption
                key={section}
                isSelected={included[section]}
                onChange={(on) => setIncluded((current) => ({ ...current, [section]: on }))}
                label={t(SECTION_LABEL[section])}
                hint={t(noteOf(section))}
              />
            ))}
          </div>
        </LabelledGroup>
      </div>
      <div className="mt-2">
        <LabelledGroup label={t("settings.section.accounts")}>
          <div className="mt-2.5">
            <ConfirmOption
              isSelected={includeAccounts}
              onChange={setIncludeAccounts}
              label={t("configuration.export.accounts.include")}
              hint={t("configuration.export.accounts.description")}
            />
          </div>
        </LabelledGroup>
      </div>
    </Dialog>
  );
}
