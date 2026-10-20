import React, { useState } from "react";

import { Button } from "../components/Button";
import { AGENT_DEFAULT_CONFIG_DIR, AGENT_LABEL, defaultAccountName, listedAgents } from "../agents";
import { agentIconSetupOptions } from "../components/AgentIcon";
import { DaemonErrorMessage } from "../components/DaemonErrorMessage";
import { TILDE_PARAMS } from "../components/literalParams";
import { Dialog, DialogError, useDialogAction, useSubmitValidation } from "../dialogs/Dialog";
import { DirectoryPicker } from "../dialogs/DirectoryPicker";
import { OptionSelect } from "../dialogs/OptionSelect";
import { TextInput } from "../dialogs/TextInput";
import type { MessageKey } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { InlineChip } from "../components/InlineChip";
import { Message, useT } from "../i18n/react";
import type { Account, Agent } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { isAbsoluteConfigDir, nameCollision, type NameCollision } from "./agentAccounts";

/** What each kind of name collision says. */
const COLLISION_MESSAGE = {
  account: "settings.accounts.nameTaken",
  default: "settings.accounts.nameTakenDefault",
  reserved: "settings.accounts.nameReserved",
} as const satisfies Record<NameCollision["kind"], MessageKey>;

/** Adds an account, or edits `editing`'s name and directory. The agent is chosen only when adding:
 * a directory belongs to one agent's layout, so an existing account never changes it, and the
 * default account's agent is fixed too. With `renameDefault` it edits the name of the agent's
 * default account instead, which has no directory to edit and takes a blank name as clearing it. */
export function AccountDialog({
  account: editing,
  agent: initialAgent,
  renameDefault,
  onClose,
}: {
  account?: Account;
  renameDefault?: boolean;
  /** The agent a new account starts on. */
  agent: Agent;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const addedAgents = useDaemonStore((s) => s.settings.added_agents);
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  const [agent, setAgent] = useState<Agent>(editing?.agent ?? initialAgent);
  const [name, setName] = useState(editing?.name ?? (renameDefault ? defaultNames?.[initialAgent] ?? "" : ""));
  const [configDir, setConfigDir] = useState(editing?.config_dir ?? "");
  const [pickingDirectory, setPickingDirectory] = useState(false);
  const { error, setError, busy, run } = useDialogAction();
  const { shown, attempt } = useSubmitValidation();

  const collision = name.trim() === "" && renameDefault
    ? undefined
    : nameCollision(name, agent, accounts, renameDefault ? null : defaultAccountName(t, defaultNames, agent), editing?.id);
  const nameError = !name && !renameDefault
    ? t("dialog.nameRequired")
    : collision && t(COLLISION_MESSAGE[collision.kind], { name: isolate(collision.name), agent: AGENT_LABEL[agent] });
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
    if (!attempt(nameError, renameDefault ? undefined : dirError)) return;
    void run(async () => {
      if (renameDefault) {
        await request({ type: "set_default_account_name", agent, name });
      } else if (editing) {
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
          renameDefault
            ? t("settings.accounts.dialog.renameDefault", { agent: AGENT_LABEL[agent] })
            : editing
              ? t("settings.accounts.dialog.edit", { agent: AGENT_LABEL[editing.agent] })
              : t("settings.accounts.add")
        }
        onClose={onClose}
        submitLabel={editing || renameDefault ? t("common.save") : t("common.add")}
        busy={busy}
        onSubmit={submit}
      >
        {!editing && !renameDefault && (
          <OptionSelect
            label={t("settings.accounts.agent")}
            // Availability is shown but deliberately not enforced: an account may be set up before
            // its agent is installed, so every added agent is selectable; one not added is not listed.
            options={agentIconSetupOptions(t, agentAvailability, listedAgents(addedAgents))}
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
          placeholder={renameDefault ? t("settings.accounts.defaultName") : t("settings.accounts.nameExample")}
          description={renameDefault ? t("settings.accounts.defaultNameHint") : undefined}
          errorMessage={shown(nameError) && (collision
            ? <Message id={COLLISION_MESSAGE[collision.kind]} params={{ name: <InlineChip dir="auto" wrap size="inherit">{collision.name}</InlineChip>, agent: AGENT_LABEL[agent] }} />
            : nameError)}
          autoFocus
        />
        {!renameDefault && <TextInput
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
        />}
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
