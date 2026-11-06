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
import type { Console, ExportedConsoles } from "../protocol";
import { shortcutOverrides } from "../shortcuts/store";
import { useDaemon } from "../store";
import { heldClientSettings } from "./clientSettings";
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

/**
 * Exports configuration to a file the user picks the place of. Its regions are chosen independently: the consoles and
 * projects (the console in hand, `console`, absent when there is none; every console; or none), and under the global
 * settings the application settings and the accounts. Full project paths of the daemon's host are left out unless asked
 * for, and while they are asked for a warning Alert under that choice says what sharing them exposes, so it is read
 * before the document exists. The choice is about project paths alone, whatever else is exported (the accounts'
 * directories are always in the file), so it is not shown while no console is exported, as there is no path to record,
 * and then it is not sent; and Export is unavailable while nothing at all is chosen. The daemon builds the document;
 * this saves it as the daemon sent it, through the platform's save dialog, and says what became of it: the name it was
 * saved under, or only that it was downloaded where the platform cannot say more. What is added are the settings this
 * client holds, which the daemon does not know of: a document with the application settings always gets this client's
 * keyboard shortcut changes (an empty set when none is changed), its appearance, Markdown style and, where the platform
 * has it, whether it runs in the background (see `heldClientSettings`), and the language it shows in place of the one
 * the daemon holds. Dismissing the save dialog leaves this one open.
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
  const { configurationFile, backgroundRun } = usePlatform();
  const { choice: appearance } = useOctoboardTheme();
  const language = useCurrentLanguage();
  const { busy, run } = useDialogAction(undefined, t("configuration.export.alertTitle"));
  const [consoles, setConsoles] = useState<ExportedConsoles>(inHand ? "one" : "all");
  const [includeSettings, setIncludeSettings] = useState(true);
  const [includeAccounts, setIncludeAccounts] = useState(false);
  const [includePaths, setIncludePaths] = useState(false);
  const warningId = useId();
  const exported = consoles === "one" ? inHand : undefined;
  const nothing = consoles === "none" && !includeSettings && !includeAccounts;
  // A project's path is the only one the choice is about: with no console exported the option is not shown.
  const pathsApply = consoles !== "none";
  const pathsIncluded = includePaths && pathsApply;

  const submit = () => {
    if (!configurationFile || busy || nothing) return;
    void run(async () => {
      const reply = await request({
        type: "export_configuration",
        console: exported?.id,
        consoles,
        include_settings: includeSettings,
        include_accounts: includeAccounts,
        ...(pathsApply && { include_paths: pathsIncluded }),
      });
      // The daemon knows nothing of this client's own settings: a document with the settings gets them here.
      const file = reply.document.settings
        ? {
          ...reply.document,
          settings: {
            ...reply.document.settings,
            keyboard_shortcuts: shortcutOverrides.get(),
            ...heldClientSettings({ appearance, language }, { backgroundRun: backgroundRun !== undefined }),
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
      <LabelledGroup label={t("configuration.export.global")}>
        <div className="flex flex-col gap-3">
          <ConfirmOption
            isSelected={includeSettings}
            onChange={setIncludeSettings}
            label={t("configuration.export.settings.include")}
            hint={
              <Message
                id="configuration.export.settings.description"
                params={{
                  generalSection: <InlineReference>{t("settings.section.general")}</InlineReference>,
                  gitSection: <InlineReference>{t("settings.section.git")}</InlineReference>,
                  behaviorSection: <InlineReference>{t("settings.section.agentPermissions")}</InlineReference>,
                  shortcutsSection: <InlineReference>{t("settings.section.shortcuts")}</InlineReference>,
                }}
              />
            }
          />
          <ConfirmOption
            isSelected={includeAccounts}
            onChange={setIncludeAccounts}
            label={t("configuration.export.accounts.include")}
            hint={t("configuration.export.accounts.description")}
          />
        </div>
      </LabelledGroup>
    </Dialog>
  );
}
