import React, { useState } from "react";

import { changedAgentFields, readAgentFields } from "../agentFields";
import {
  AGENT_ACCOUNT_FIELD,
  AGENT_CONSOLE_EFFORT_FIELD,
  AGENT_CONSOLE_MODEL_FIELD,
  AGENT_CONSOLE_PROJECT_EFFORT_FIELD,
  AGENT_CONSOLE_PROJECT_MODEL_FIELD,
  AGENT_LABEL,
  AGENT_OPTIONS,
  selectableAgent,
} from "../agents";
import { imageToAvatar } from "../avatarImage";
import { AgentIcon, agentIconPickerOptions } from "../components/AgentIcon";
import { useT } from "../i18n/react";
import type { Agent, Console } from "../protocol";
import { SettingsDialog } from "../settings/SettingsDialog";
import { useDaemon, useDaemonStore } from "../store";
import { AvatarField } from "./AvatarField";
import { ConsoleAccountsField } from "./ConsoleAccountsField";
import { ConsoleLaunchFields } from "./ConsoleLaunchFields";
import { Dialog, DialogError, useDialogAction, useSubmitValidation } from "./Dialog";
import { GroupRow, GroupRows } from "./GroupRows";
import { LabelledGroup } from "./LabelledGroup";
import { OptionSelect } from "./OptionSelect";
import { TextAreaInput } from "./TextAreaInput";
import { TextInput } from "./TextInput";

/** Every per-agent field of a console: each agent's account, and the model and effort of its console
 * sessions and of its project sessions. */
const FIELDS = AGENT_OPTIONS.flatMap(({ value: agent }) => [
  AGENT_ACCOUNT_FIELD[agent],
  AGENT_CONSOLE_MODEL_FIELD[agent],
  AGENT_CONSOLE_EFFORT_FIELD[agent],
  AGENT_CONSOLE_PROJECT_MODEL_FIELD[agent],
  AGENT_CONSOLE_PROJECT_EFFORT_FIELD[agent],
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
  const [name, setName] = useState(editing?.name ?? "");
  // A new console opens on Claude Code, or on the first agent that may be picked when that one is
  // known to be missing, so the agent never starts on an entry the picker would refuse.
  const initialAgent = selectableAgent(agentAvailability, "claude");
  const [defaultAgent, setDefaultAgent] = useState<Agent>(editing?.default_agent ?? initialAgent);
  // `""` is "Same as console sessions": the console sets no agent of its own for its project
  // sessions and they follow the console sessions' agent, which on the wire is an explicit `null`
  // when saving.
  const [projectAgent, setProjectAgent] = useState<Agent | "">(editing?.project_session_agent ?? "");
  // What the console holds for each agent when the dialog opens, and the edits since: the account
  // (`null` is that agent's default account) and what its console session is launched with (`null`
  // being the agent's own default), kept per agent as the names are the agent's own, so a value set
  // for one says nothing about another.
  const [initial] = useState(() => readAgentFields(FIELDS, editing ?? {}));
  const [fields, setFields] = useState(initial);
  const [icon, setIcon] = useState<string | null>(editing?.icon ?? null);
  const [sessionPrompt, setSessionPrompt] = useState(editing?.console_session_prompt ?? "");
  const { error, setError, busy, run } = useDialogAction();
  // Settings, opened over this dialog from the accounts table. It is rendered beside the dialog
  // rather than inside it, as its forms would otherwise submit this one.
  const [managingAccounts, setManagingAccounts] = useState(false);

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
          // Back to Same as console sessions is an explicit `null`.
          ...(projectAgent !== (editing.project_session_agent ?? "") && { project_session_agent: projectAgent || null }),
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
          ...(projectAgent && { project_session_agent: projectAgent }),
          ...(icon && { icon }),
          ...(prompt && { console_session_prompt: prompt }),
          ...changedAgentFields("create", fields, initial),
        });
      }
      onClose();
    });
  };

  return (
    <>
      <Dialog
        title={editing ? t("dialog.console.edit") : t("dialog.console.new")}
        onClose={onClose}
        submitLabel={editing ? t("common.save") : t("common.create")}
        busy={busy || decoding}
        onSubmit={submit}
        size="wide"
      >
        {/* The avatar is centred on the name's label and input by a fixed offset rather than by
          alignment, so it stays put when the name's error appears under them. The two are 66px
          together, and 62px from `sm` up, where HeroUI's input drops to a smaller text size. */}
        <div className="flex items-start gap-3">
          <div className="mt-2.25 sm:mt-1.75">
            <AvatarField
              icon={icon}
              onPick={(file) => void chooseImage(file)}
              onRemove={() => setIcon(null)}
              isDisabled={busy}
              isPending={decoding}
            />
          </div>
          <div className="min-w-0 flex-1">
            <TextInput
              label={t("common.name")}
              value={name}
              onChange={setName}
              errorMessage={shown(nameError)}
              autoFocus
            />
          </div>
        </div>
        <LabelledGroup label={t("dialog.console.defaultAgent")} description={t("dialog.console.defaultAgentNote")}>
          <GroupRows columns>
            <GroupRow label={t("dialog.console.defaultAgentConsole")}>
              <OptionSelect
                hideLabel
                label={t("dialog.console.defaultAgentConsole")}
                options={agentIconPickerOptions(t, agentAvailability, defaultAgent)}
                value={defaultAgent}
                onChange={setDefaultAgent}
              />
            </GroupRow>
            <GroupRow label={t("dialog.console.defaultAgentProject")}>
              <OptionSelect<Agent | "">
                hideLabel
                label={t("dialog.console.defaultAgentProject")}
                options={[
                  {
                    value: "",
                    label: t("dialog.console.projectAgentSame", { agent: AGENT_LABEL[defaultAgent] }),
                    icon: <AgentIcon agent={defaultAgent} className="size-4 opacity-50" />,
                  },
                  ...agentIconPickerOptions(t, agentAvailability, projectAgent || undefined),
                ]}
                value={projectAgent}
                onChange={setProjectAgent}
              />
            </GroupRow>
          </GroupRows>
        </LabelledGroup>
        <ConsoleAccountsField
          label={t("dialog.console.accounts")}
          description={t("dialog.console.accountsNote")}
          values={fields}
          onChange={(field, value) => setFields({ ...fields, [field]: value })}
          onManageAccounts={() => setManagingAccounts(true)}
          managingAccounts={managingAccounts}
        />
        {/* What is inside is the console's own: its sessions' launch settings and its instructions. */}
        <ConsoleLaunchFields
          defaultAgent={defaultAgent}
          values={fields}
          onChange={(field, value) => setFields({ ...fields, [field]: value })}
        >
          <TextAreaInput
            label={t("dialog.console.sessionInstructions")}
            value={sessionPrompt}
            onChange={setSessionPrompt}
            placeholder={t("dialog.console.sessionInstructionsPlaceholder")}
            description={t("dialog.console.sessionInstructionsNote")}
          />
        </ConsoleLaunchFields>
        <DialogError message={error} />
      </Dialog>
      {managingAccounts && <SettingsDialog initialSection="accounts" onClose={() => setManagingAccounts(false)} />}
    </>
  );
}
