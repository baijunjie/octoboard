import { Button } from "@heroui/react";
import React, { useState } from "react";

import { AGENT_DEFAULT_CONFIG_DIR, AGENT_LABEL } from "../agents";
import { agentIconSetupOptions } from "../components/AgentIcon";
import { DaemonErrorMessage } from "../components/DaemonErrorMessage";
import { TILDE_PARAMS } from "../components/literalParams";
import { Dialog, DialogError, useDialogAction, useSubmitValidation } from "../dialogs/Dialog";
import { DirectoryPicker } from "../dialogs/DirectoryPicker";
import { OptionSelect } from "../dialogs/OptionSelect";
import { TextInput } from "../dialogs/TextInput";
import { isolate } from "../i18n/isolate";
import { InlineChip } from "../components/InlineChip";
import { Message, useT } from "../i18n/react";
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
  const { error, setError, busy, run } = useDialogAction();
  const { shown, attempt } = useSubmitValidation();

  const collision = nameCollision(name, agent, accounts, t("settings.accounts.defaultName"), editing?.id);
  const nameError = !name
    ? t("dialog.nameRequired")
    : collision?.kind === "default"
      ? t("settings.accounts.nameTakenDefault", { name: isolate(collision.name), agent: AGENT_LABEL[agent] })
      : collision && t("settings.accounts.nameTaken", { name: isolate(collision.name), agent: AGENT_LABEL[agent] });
  const dirError = isAbsoluteConfigDir(configDir) ? undefined : "notAbsolute";
  // Whether another account uses the directory is judged by the daemon, which alone knows the home
  // directory and the default account's resolved one; its refusal is worded under the field it is
  // about, and stands until the directory or agent is changed.
  const dirTaken = error !== undefined && typeof error !== "string" && error.code === "config_dir_taken" ? error : undefined;

  // Typing and picking both change the directory, and either one lifts the daemon's refusal of it.
  const changeConfigDir = (value: string) => {
    setConfigDir(value);
    setError(undefined);
  };

  const submit = () => {
    if (!attempt(nameError, dirError)) return;
    void run(async () => {
      if (editing) {
        await request({ type: "update_account", account: editing.id, name, config_dir: configDir });
      } else {
        await request({ type: "create_account", agent, name, config_dir: configDir });
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
            // its agent is installed, so every agent is listed and selectable.
            options={agentIconSetupOptions(t, agentAvailability)}
            value={agent}
            onChange={(value) => {
              setAgent(value);
              setError(undefined);
            }}
          />
        )}
        <TextInput
          label={t("common.name")}
          value={name}
          onChange={setName}
          placeholder={t("settings.accounts.nameExample")}
          errorMessage={shown(nameError) && (collision
            ? <Message id={collision.kind === "default" ? "settings.accounts.nameTakenDefault" : "settings.accounts.nameTaken"} params={{ name: <InlineChip dir="auto" wrap size="inherit">{collision.name}</InlineChip>, agent: AGENT_LABEL[agent] }} />
            : nameError)}
          autoFocus
        />
        <TextInput
          label={t("settings.accounts.configDir")}
          value={configDir}
          onChange={changeConfigDir}
          dir="ltr"
          placeholder={AGENT_DEFAULT_CONFIG_DIR[agent]}
          description={agent === "grok" ? t("settings.accounts.dirHintGrok") : t("settings.accounts.dirHint")}
          errorMessage={dirTaken
            ? <DaemonErrorMessage error={dirTaken} showTitle={false} />
            : shown(dirError) && <Message id="settings.accounts.dirNotAbsolute" params={TILDE_PARAMS} />}
          trailing={
            <Button type="button" variant="secondary" onPress={() => setPickingDirectory(true)}>
              {t("common.browse")}
            </Button>
          }
        />
        <DialogError message={dirTaken ? undefined : error} />
      </Dialog>
      {pickingDirectory && (
        <DirectoryPicker
          title={t("dialog.chooseDirectory")}
          initialPath={isAbsoluteConfigDir(configDir) ? configDir : "~"}
          onPick={(picked) => {
            changeConfigDir(picked);
            setPickingDirectory(false);
          }}
          onClose={() => setPickingDirectory(false)}
        />
      )}
    </>
  );
}
