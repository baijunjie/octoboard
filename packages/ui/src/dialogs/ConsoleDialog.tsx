import { Button, Label } from "@heroui/react";
import React, { useId, useRef, useState } from "react";

import { choiceGroups, choiceKey, consoleAccount, findEntry } from "../accountChoices";
import { AGENT_ACCOUNT_FIELD, AGENT_LABEL, AGENT_OPTIONS, selectableAgent } from "../agents";
import { imageToAvatar } from "../avatarImage";
import { agentIconPickerOptions } from "../components/AgentIcon";
import { ConsoleAvatar } from "../components/ConsoleAvatar";
import { useT } from "../i18n/react";
import type { AccountField, Agent, Console } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { Dialog, DialogError, useDialogAction, useSubmitValidation } from "./Dialog";
import { OptionSelect } from "./OptionSelect";
import { TextInput } from "./TextInput";

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
  // known to be missing, so neither agent starts on an entry the picker would refuse.
  const initialAgent = selectableAgent(agentAvailability, "claude");
  const [consoleSessionAgent, setConsoleSessionAgent] = useState<Agent>(editing?.console_session_agent ?? initialAgent);
  const [defaultAgent, setDefaultAgent] = useState<Agent>(editing?.default_agent ?? initialAgent);
  // The account picked for each agent; `null` is that agent's default account.
  const [accountIds, setAccountIds] = useState<Record<AccountField, string | null>>(() => ({
    claude_account_id: editing ? consoleAccount(editing, "claude", accounts) : null,
    codex_account_id: editing ? consoleAccount(editing, "codex", accounts) : null,
    grok_account_id: editing ? consoleAccount(editing, "grok", accounts) : null,
  }));
  const [icon, setIcon] = useState<string | null>(editing?.icon ?? null);
  const { error, setError, busy, run } = useDialogAction();
  const fileInput = useRef<HTMLInputElement>(null);
  const avatarLabelId = useId();

  // While a picked image is being read, saving would send the old avatar.
  const [decoding, setDecoding] = useState(false);

  const chooseImage = async (file: File | undefined) => {
    if (!file) return;
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

  // One row per agent the dialog currently selects, in a fixed order. An agent that is not selected
  // keeps whatever is stored for it: its row is neither shown nor sent.
  const shownAgents = AGENT_OPTIONS.map((option) => option.value).filter(
    (agent) => agent === consoleSessionAgent || agent === defaultAgent,
  );

  const groups = choiceGroups(accounts, agentAvailability, t("settings.accounts.defaultName"));

  // The avatar's own failure is not a field's, and stays at the foot of the dialog.
  const { shown, attempt } = useSubmitValidation();
  const nameError = name ? undefined : t("dialog.nameRequired");

  const submit = () => {
    if (decoding) return;
    if (!attempt(nameError)) return;
    void run(async () => {
      if (editing) {
        const changes: Partial<Record<AccountField | "icon", string | null>> = {};
        for (const agent of shownAgents) {
          const field = AGENT_ACCOUNT_FIELD[agent];
          // Sent only when the picker changed, so saving something else (a rename, say) leaves a
          // reference alone. A change to the default account is an explicit `null` (clear), never
          // an omitted field: omitting it would leave the old account in place while the picker
          // says otherwise.
          if (accountIds[field] !== (editing[field] ?? null)) changes[field] = accountIds[field];
        }
        // Same for the avatar: a removal is an explicit `null`, an unchanged one is left out.
        if (icon !== (editing.icon ?? null)) changes.icon = icon;
        await request({
          type: "update_console",
          console: editing.id,
          name,
          console_session_agent: consoleSessionAgent,
          default_agent: defaultAgent,
          ...changes,
        });
      } else {
        const references: Partial<Record<AccountField, string>> = {};
        for (const agent of shownAgents) {
          const field = AGENT_ACCOUNT_FIELD[agent];
          const picked = accountIds[field];
          if (picked) references[field] = picked;
        }
        await request({
          type: "create_console",
          name,
          console_session_agent: consoleSessionAgent,
          default_agent: defaultAgent,
          ...(icon && { icon }),
          ...references,
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
      <TextInput
        label={t("common.name")}
        value={name}
        onChange={setName}
        errorMessage={shown(nameError)}
        autoFocus
      />
      <div role="group" aria-labelledby={avatarLabelId} className="flex flex-col gap-1.5">
        <Label id={avatarLabelId}>{t("dialog.console.avatar")}</Label>
        <div className="flex items-center gap-3">
          <ConsoleAvatar name={name} icon={icon} className="size-12" />
          <Button type="button" variant="secondary" onPress={() => fileInput.current?.click()} isDisabled={busy || decoding}>
            {t("dialog.console.avatarChoose")}
          </Button>
          <Button type="button" variant="secondary" onPress={() => setIcon(null)} isDisabled={busy || decoding || !icon}>
            {t("dialog.console.avatarRemove")}
          </Button>
          <input
            ref={fileInput}
            type="file"
            accept="image/*"
            hidden
            onChange={(e) => {
              void chooseImage(e.target.files?.[0]);
              // Cleared so choosing the same file again still counts as a change.
              e.target.value = "";
            }}
          />
        </div>
      </div>
      <OptionSelect label={t("dialog.console.consoleSessionAgent")} options={agentOptions} value={consoleSessionAgent} onChange={setConsoleSessionAgent} />
      <OptionSelect label={t("dialog.console.defaultAgent")} options={agentOptions} value={defaultAgent} onChange={setDefaultAgent} />
      {shownAgents.map((agent) => {
        const field = AGENT_ACCOUNT_FIELD[agent];
        const entries = groups.find((group) => group.agent === agent)?.entries ?? [];
        return (
          <OptionSelect
            key={agent}
            label={t("dialog.console.account", { agent: AGENT_LABEL[agent] })}
            options={entries.map((entry) => ({ value: entry.key, label: entry.name }))}
            value={choiceKey({ agent, account: accountIds[field] })}
            onChange={(key) => {
              const entry = findEntry(groups, key);
              if (entry) setAccountIds({ ...accountIds, [field]: entry.account });
            }}
          />
        );
      })}
      <DialogError message={error} />
    </Dialog>
  );
}
