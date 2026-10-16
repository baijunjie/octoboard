import React, { useState } from "react";

import { choiceGroups, choiceKey, findEntry } from "../accountChoices";
import { changedAgentFields, readAgentFields } from "../agentFields";
import {
  AGENT_ACCOUNT_FIELD,
  AGENT_CONSOLE_EFFORT_FIELD,
  AGENT_CONSOLE_MODEL_FIELD,
  AGENT_LABEL,
  AGENT_OPTIONS,
  selectableAgent,
} from "../agents";
import { imageToAvatar } from "../avatarImage";
import { agentIconPickerOptions } from "../components/AgentIcon";
import { useT } from "../i18n/react";
import type { Agent, Console } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { AgentAccountField } from "./AgentAccountField";
import { AvatarField } from "./AvatarField";
import { Dialog, DialogError, useDialogAction, useSubmitValidation } from "./Dialog";
import { LaunchSettingsFields } from "./LaunchSettingsFields";
import { TextAreaInput } from "./TextAreaInput";
import { TextInput } from "./TextInput";

/** Every per-agent field of a console: each agent's account, model and effort. */
const FIELDS = AGENT_OPTIONS.flatMap(({ value: agent }) => [
  AGENT_ACCOUNT_FIELD[agent],
  AGENT_CONSOLE_MODEL_FIELD[agent],
  AGENT_CONSOLE_EFFORT_FIELD[agent],
]);

export function ConsoleDialog({
  console: editing,
  onClose,
}: {
  /** Editing an existing console when set, creating a new one otherwise. */
  console?: Console;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const agentOptions = agentIconPickerOptions(t, agentAvailability);
  const [name, setName] = useState(editing?.name ?? "");
  // A new console opens on Claude Code, or on the first agent that may be picked when that one is
  // known to be missing, so the agent never starts on an entry the picker would refuse.
  const initialAgent = selectableAgent(agentAvailability, "claude");
  const [defaultAgent, setDefaultAgent] = useState<Agent>(editing?.default_agent ?? initialAgent);
  // What the console holds for each agent when the dialog opens, and the edits since: the account
  // (`null` is that agent's default account) and what its console session is launched with (`null`
  // being the agent's own default), kept per agent as the names are the agent's own, so a value set
  // for one says nothing about another.
  const [initial] = useState(() => readAgentFields(FIELDS, editing ?? {}));
  const [fields, setFields] = useState(initial);
  const [icon, setIcon] = useState<string | null>(editing?.icon ?? null);
  const [sessionPrompt, setSessionPrompt] = useState(editing?.console_session_prompt ?? "");
  const { error, setError, busy, run } = useDialogAction();

  // While a picked image is being read, saving would send the old avatar.
  const [decoding, setDecoding] = useState(false);

  const chooseImage = async (file: File) => {
    setError(undefined);
    setDecoding(true);
    try {
      setIcon(await imageToAvatar(file));
    } catch {
      setError(t("dialog.console.avatarUnreadable"));
    } finally {
      setDecoding(false);
    }
  };

  const groups = choiceGroups(accounts, agentAvailability, t("settings.accounts.defaultName"));

  // The account and the launch settings shown are the selected agent's; the other agents' are kept
  // as they were set and still saved, so switching agent never discards an edit. The account
  // picker is offered only where there is something to choose between — an agent with no account
  // of its own has its default account alone. A reference to an account that was since removed is
  // shown as the default account but stays as stored, and is sent only if another account is chosen:
  // choosing the entry already shown is not a change, to the picker or to the diff.
  const accountField = AGENT_ACCOUNT_FIELD[defaultAgent];
  const modelField = AGENT_CONSOLE_MODEL_FIELD[defaultAgent];
  const effortField = AGENT_CONSOLE_EFFORT_FIELD[defaultAgent];
  const accountEntries = groups.find((group) => group.agent === defaultAgent)?.entries ?? [];
  const accountLabel = t("dialog.agentSettings.account", { agent: AGENT_LABEL[defaultAgent] });
  const pickedAccount = accountEntries.some((entry) => entry.account === fields[accountField]) ? fields[accountField] : null;

  // The avatar's own failure is not a field's, and stays at the foot of the dialog.
  const { shown, attempt } = useSubmitValidation();
  const nameError = name ? undefined : t("dialog.nameRequired");

  const submit = () => {
    if (decoding) return;
    if (!attempt(nameError)) return;
    const prompt = sessionPrompt.trim();
    void run(async () => {
      if (editing) {
        await request({
          type: "update_console",
          console: editing.id,
          name,
          default_agent: defaultAgent,
          ...changedAgentFields("update", fields, initial),
          // The avatar is cleared the same way, with an explicit `null`.
          ...(icon !== (editing.icon ?? null) && { icon }),
          // Cleared the same way, with an explicit `null`.
          ...(prompt !== (editing.console_session_prompt ?? "") && {
            console_session_prompt: prompt || null,
          }),
        });
      } else {
        await request({
          type: "create_console",
          name,
          default_agent: defaultAgent,
          ...(icon && { icon }),
          ...(prompt && { console_session_prompt: prompt }),
          ...changedAgentFields("create", fields, initial),
        });
      }
      onClose();
    });
  };

  return (
    <Dialog
      title={editing ? t("dialog.console.edit") : t("dialog.console.new")}
      onClose={onClose}
      submitLabel={editing ? t("common.save") : t("common.create")}
      busy={busy || decoding}
      onSubmit={submit}
    >
      <AvatarField
        icon={icon}
        onPick={(file) => void chooseImage(file)}
        onRemove={() => setIcon(null)}
        isDisabled={busy}
        isPending={decoding}
      />
      <TextInput
        label={t("common.name")}
        value={name}
        onChange={setName}
        errorMessage={shown(nameError)}
        autoFocus
      />
      <AgentAccountField
        label={t("dialog.console.defaultAgent")}
        options={agentOptions}
        value={defaultAgent}
        onChange={setDefaultAgent}
        account={
          accountEntries.length > 1
            ? {
              label: accountLabel,
              options: accountEntries.map((entry) => ({ value: entry.key, label: entry.name })),
              value: choiceKey({ agent: defaultAgent, account: pickedAccount }),
              onChange: (key) => {
                const entry = findEntry(groups, key);
                if (entry) setFields({ ...fields, [accountField]: entry.account });
              },
            }
            : undefined
        }
      />
      {/* What is inside is the console session's alone: its launch settings and its instructions. */}
      <LaunchSettingsFields
        agent={defaultAgent}
        account={pickedAccount}
        model={fields[modelField]}
        effort={fields[effortField]}
        onModelChange={(value) => setFields({ ...fields, [modelField]: value })}
        onEffortChange={(value) => setFields({ ...fields, [effortField]: value })}
        note={t("dialog.console.launchSettingsNote")}
      >
        <TextAreaInput
          label={t("dialog.console.sessionInstructions")}
          value={sessionPrompt}
          onChange={setSessionPrompt}
          placeholder={t("dialog.console.sessionInstructionsPlaceholder")}
          description={t("dialog.console.sessionInstructionsNote")}
        />
      </LaunchSettingsFields>
      <DialogError message={error} />
    </Dialog>
  );
}
