import { ActionableError } from "../errorRecovery";
import { Alert, Description, Label, Radio, RadioGroup } from "@heroui/react";
import React, { useId, useState } from "react";

import { appConfig } from "../appConfig";
import { InlineReference } from "../components/InlineReference";
import { TintedAlert } from "../components/TintedAlert";
import { ConfirmOption } from "../dialogs/ConfirmOption";
import { Dialog, useDialogAction } from "../dialogs/Dialog";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import type { Console, Event } from "../protocol";
import { useDaemon } from "../store";
import { fileFailureText } from "./failureText";

type Scope = "console" | "all";

/** The name the save dialog offers: the application's, and for one console that console's, with
 * what a file name cannot hold taken out. */
export function exportFileName(console?: Pick<Console, "name">): string {
  const base = appConfig.name.toLowerCase();
  // eslint-disable-next-line no-control-regex -- File names cannot contain control characters.
  const safe = console?.name.replace(/[\u0000-\u001f\u007f/\\:*?"<>|]/g, " ").replace(/\s+/g, " ").trim();
  return safe ? `${base}-${safe}.json` : `${base}-configuration.json`;
}

/**
 * Exports configuration to a file the user picks the place of. The scope is chosen here, between
 * the console in hand (`console`, absent when there is none) and everything. Full project paths of
 * the daemon's host are left out unless asked for, and the notice that the file then carries them
 * appears with that choice, so it is read before the document exists. The daemon builds the
 * document; this saves it as the daemon sent it, through the platform's save dialog, and says what
 * became of it: the name it was saved under, or only that it was downloaded where the platform
 * cannot say more. Dismissing the save dialog leaves this one open.
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
  const { busy, run } = useDialogAction();
  const [scope, setScope] = useState<Scope>(inHand ? "console" : "all");
  const [includePaths, setIncludePaths] = useState(false);
  const warningId = useId();
  const exported = scope === "console" ? inHand : undefined;

  const submit = () => {
    if (!configurationFile || busy) return;
    void run(async () => {
      const reply = (await request({ type: "export_configuration", console: exported?.id, include_paths: includePaths })) as Extract<
        Event,
        { type: "configuration_exported" }
      >;
      let saved;
      try {
        saved = await configurationFile.save(exportFileName(exported), `${JSON.stringify(reply.document, null, 2)}\n`);
      } catch (err) {
        throw new ActionableError(fileFailureText(t, err, "configuration.export.failed"), "file", { cause: err });
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
      autoFocusSubmit
      onSubmit={submit}
    >
      {inHand ? (
        <RadioGroup variant="secondary" value={scope} onChange={(value) => setScope(value as Scope)}>
          <Label>{t("configuration.export.scope")}</Label>
          <Radio value="console">
            <Radio.Content>
              <Radio.Control>
                <Radio.Indicator />
              </Radio.Control>
              <Label><Message id="configuration.export.scope.console" params={{ name: <InlineReference dir="auto" wrap>{inHand.name}</InlineReference> }} /></Label>
            </Radio.Content>
            <Description>{t("configuration.export.scope.console.description")}</Description>
          </Radio>
          <Radio value="all">
            <Radio.Content>
              <Radio.Control>
                <Radio.Indicator />
              </Radio.Control>
              <Label>{t("configuration.export.scope.all")}</Label>
            </Radio.Content>
            <Description>{t("configuration.export.scope.all.description")}</Description>
          </Radio>
        </RadioGroup>
      ) : (
        <p className="text-sm">{t("configuration.export.scope.all.description")}</p>
      )}
      <ConfirmOption
        isSelected={includePaths}
        onChange={setIncludePaths}
        isReadOnly={busy}
        label={t("configuration.export.paths.include")}
        hint={t("configuration.export.paths.include.description")}
        describedBy={includePaths ? warningId : undefined}
      />
      {/* HeroUI's own warning tint rather than its default surface, which is the dialog's own fill and
          leaves the callout unmarked. */}
      {includePaths && (
        <TintedAlert id={warningId} status="warning">
          <Alert.Indicator />
          <Alert.Content>
            <Alert.Title>{t("configuration.export.paths.title")}</Alert.Title>
            <Alert.Description><Message id="configuration.export.paths.description" /></Alert.Description>
          </Alert.Content>
        </TintedAlert>
      )}
    </Dialog>
  );
}
