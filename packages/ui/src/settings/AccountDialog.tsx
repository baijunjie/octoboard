import { Button } from "@heroui/react";
import React, { useState } from "react";

import { AGENT_CONFIG_DIR, AGENT_LABEL } from "../agents";
import { agentIconPickerOptions } from "../components/AgentIcon";
import { Dialog, DialogError, useDialogAction, useSubmitValidation } from "../dialogs/Dialog";
import { DirectoryPicker } from "../dialogs/DirectoryPicker";
import { OptionSelect } from "../dialogs/OptionSelect";
import { TextInput } from "../dialogs/TextInput";
import { useT } from "../i18n/react";
import type { Account, Agent } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { isAbsoluteConfigDir, nameCollision } from "./agentAccounts";

/** Adds an account, or edits `editing`'s name and directory. The agent is chosen only when adding:
 * a directory belongs to one agent's layout, so an existing account never changes it. */
export function AccountDialog({
  account: editing,
  onClose,
}: {
  account?: Account;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const [agent, setAgent] = useState<Agent>(editing?.agent ?? "claude");
  const [name, setName] = useState(editing?.name ?? "");
  const [configDir, setConfigDir] = useState(editing?.config_dir ?? "");
  const [pickingDirectory, setPickingDirectory] = useState(false);
  const { error, busy, run } = useDialogAction();
  const { shown, attempt } = useSubmitValidation();

  const collision = nameCollision(name, agent, accounts, t("settings.accounts.defaultName"), editing?.id);
  const nameError = !name.trim()
    ? t("dialog.nameRequired")
    : collision?.kind === "default"
      ? t("settings.accounts.nameTakenDefault", { name: collision.name, agent: AGENT_LABEL[agent] })
      : collision && t("settings.accounts.nameTaken", { name: collision.name, agent: AGENT_LABEL[agent] });
  const dirError = isAbsoluteConfigDir(configDir) ? undefined : t("settings.accounts.dirNotAbsolute");

  const submit = () => {
    if (!attempt(nameError, dirError)) return;
    const trimmedName = name.trim();
    const trimmedDir = configDir.trim();
    void run(async () => {
      if (editing) {
        await request({ type: "update_account", account: editing.id, name: trimmedName, config_dir: trimmedDir });
      } else {
        await request({ type: "create_account", agent, name: trimmedName, config_dir: trimmedDir });
      }
      onClose();
    });
  };

  return (
    <>
      <Dialog
        title={
          editing
            ? t("settings.accounts.dialog.edit", { agent: AGENT_LABEL[editing.agent] })
            : t("settings.accounts.add")
        }
        onClose={onClose}
        submitLabel={editing ? t("common.save") : t("common.add")}
        busy={busy}
        onSubmit={submit}
      >
        {!editing && (
          <OptionSelect
            label={t("settings.accounts.agent")}
            // Availability is shown but deliberately not enforced: an account may be set up before
            // its agent is installed, so every entry stays selectable.
            options={agentIconPickerOptions(t, agentAvailability).map(({ isDisabled: _, ...option }) => option)}
            value={agent}
            onChange={setAgent}
          />
        )}
        <TextInput
          label={t("common.name")}
          value={name}
          onChange={setName}
          placeholder={t("settings.accounts.nameExample")}
          errorMessage={shown(nameError)}
          autoFocus
        />
        <TextInput
          label={t("settings.accounts.configDir")}
          value={configDir}
          onChange={setConfigDir}
          dir="ltr"
          placeholder={AGENT_CONFIG_DIR[agent].placeholder}
          description={agent === "grok" ? t("settings.accounts.dirHintGrok") : t("settings.accounts.dirHint")}
          errorMessage={shown(dirError)}
          trailing={
            <Button type="button" variant="secondary" onPress={() => setPickingDirectory(true)}>
              {t("common.browse")}
            </Button>
          }
        />
        <DialogError message={error} />
      </Dialog>
      {pickingDirectory && (
        <DirectoryPicker
          title={t("dialog.chooseDirectory")}
          initialPath={isAbsoluteConfigDir(configDir) ? configDir.trim() : "~"}
          onPick={(picked) => {
            setConfigDir(picked);
            setPickingDirectory(false);
          }}
          onClose={() => setPickingDirectory(false)}
        />
      )}
    </>
  );
}
