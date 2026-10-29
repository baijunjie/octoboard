import { Label, Radio, RadioGroup } from "@heroui/react";
import React, { useId, useState } from "react";

import { appConfig } from "../appConfig";
import { ChoiceWarning } from "../components/ChoiceWarning";
import { InlineReference } from "../components/InlineReference";
import { ConfirmOption } from "../dialogs/ConfirmOption";
import { Dialog, useDialogAction } from "../dialogs/Dialog";
import { LabelledGroup } from "../dialogs/LabelledGroup";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import type { Console, Event, ExportedConsoles } from "../protocol";
import { shortcutOverrides } from "../shortcuts/store";
import { useDaemon } from "../store";
import { fileFailure } from "./failureText";

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
 * saved under, or only that it was downloaded where the platform cannot say more. The one thing added is this client's
 * keyboard shortcut changes, which the daemon does not know of: a document with the application settings always gets
 * them (an empty set when none is changed). Dismissing the save dialog leaves this one open.
 */
export function ExportConfigurationDialog({
  console: inHand,
  onClose,
}: {
  console?: Console;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const { request, toastNotice } = useDaemon();
  const { configurationFile } = usePlatform();
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
      const reply = (await request({
        type: "export_configuration",
        console: exported?.id,
        consoles,
        include_settings: includeSettings,
        include_accounts: includeAccounts,
        ...(pathsApply && { include_paths: pathsIncluded }),
      })) as Extract<Event, { type: "configuration_exported" }>;
      // The daemon knows nothing of this client's shortcuts: a document with the settings gets them here.
      const file = reply.document.settings
        ? { ...reply.document, settings: { ...reply.document.settings, keyboard_shortcuts: shortcutOverrides.get() } }
        : reply.document;
      let saved;
      try {
        saved = await configurationFile.save(exportFileName(exported), `${JSON.stringify(file, null, 2)}\n`);
      } catch (err) {
        throw fileFailure(t, err, "configuration.export.couldNotSave");
      }
      if (saved === undefined) return;
      toastNotice(
        saved.kind === "saved" ? t("configuration.export.saved", { name: isolate(saved.name) }) : t("configuration.export.downloaded"),
        saved.kind === "saved" ? <Message id="configuration.export.saved" params={{ name: <InlineReference monospace wrap>{saved.name}</InlineReference> }} /> : undefined,
      );
      onClose();
    });
  };

  return (
    <Dialog
      title={t("configuration.export.title")}
      onClose={busy ? () => {} : onClose}
      submitLabel={t("configuration.export.submit")}
      busy={busy}
      submitDisabled={nothing}
      autoFocusSubmit
      onSubmit={submit}
    >
      <RadioGroup variant="secondary" value={consoles} onChange={(value) => setConsoles(value as ExportedConsoles)} isReadOnly={busy}>
        <Label>{t("configuration.export.scope")}</Label>
        {inHand && (
          <Radio value="one">
            <Radio.Content>
              <Radio.Control>
                <Radio.Indicator />
              </Radio.Control>
              <Label><Message id="configuration.export.scope.console" params={{ name: <InlineReference dir="auto" wrap>{inHand.name}</InlineReference> }} /></Label>
            </Radio.Content>
          </Radio>
        )}
        <Radio value="all">
          <Radio.Content>
            <Radio.Control>
              <Radio.Indicator />
            </Radio.Control>
            <Label>{t("configuration.export.scope.all")}</Label>
          </Radio.Content>
        </Radio>
        <Radio value="none">
          <Radio.Content>
            <Radio.Control>
              <Radio.Indicator />
            </Radio.Control>
            <Label>{t("configuration.export.scope.none")}</Label>
          </Radio.Content>
        </Radio>
      </RadioGroup>
      {pathsApply && (
        <ConfirmOption
          isSelected={includePaths}
          onChange={setIncludePaths}
          isReadOnly={busy}
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
          <Message id="configuration.export.paths.description" />
        </ChoiceWarning>
      )}
      <LabelledGroup label={t("configuration.export.global")}>
        <div className="flex flex-col gap-3">
          <ConfirmOption
            isSelected={includeSettings}
            onChange={setIncludeSettings}
            isReadOnly={busy}
            label={t("configuration.export.settings.include")}
            hint={t("configuration.export.settings.description")}
          />
          <ConfirmOption
            isSelected={includeAccounts}
            onChange={setIncludeAccounts}
            isReadOnly={busy}
            label={t("configuration.export.accounts.include")}
            hint={t("configuration.export.accounts.description")}
          />
        </div>
      </LabelledGroup>
    </Dialog>
  );
}
