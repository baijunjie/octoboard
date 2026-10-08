import { Button, Label } from "@heroui/react";
import React, { useId, useRef, useState } from "react";

import { AGENT_CONFIG_DIR, AGENT_LABEL, AGENT_OPTIONS } from "../agents";
import { imageToAvatar } from "../avatarImage";
import { agentIconPickerOptions } from "../components/AgentIcon";
import { ConsoleAvatar } from "../components/ConsoleAvatar";
import { useT } from "../i18n/react";
import type { Agent, ConfigDirField, Console } from "../protocol";
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
  const agentOptions = agentIconPickerOptions(t, agentAvailability);
  const [name, setName] = useState(editing?.name ?? "");
  const [consoleSessionAgent, setConsoleSessionAgent] = useState<Agent>(editing?.console_session_agent ?? "claude");
  const [defaultAgent, setDefaultAgent] = useState<Agent>(editing?.default_agent ?? "claude");
  const [configDirs, setConfigDirs] = useState<Record<ConfigDirField, string>>({
    claude_config_dir: editing?.claude_config_dir ?? "",
    codex_config_dir: editing?.codex_config_dir ?? "",
    grok_config_dir: editing?.grok_config_dir ?? "",
  });
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

  // What was typed for each shown row, trimmed.
  const typedDirs = shownAgents.map((agent) => {
    const { field } = AGENT_CONFIG_DIR[agent];
    return [field, configDirs[field].trim()] as const;
  });

  // The avatar's own failure is not a field's, and stays at the foot of the dialog.
  const { shown, attempt } = useSubmitValidation();
  const nameError = name.trim() ? undefined : t("dialog.nameRequired");

  const submit = () => {
    if (decoding) return;
    if (!attempt(nameError)) return;
    void run(async () => {
      if (editing) {
        const changes: Partial<Record<ConfigDirField | "icon", string | null>> = {};
        for (const [field, typed] of typedDirs) {
          // Sent only when the input changed, so saving something else (a rename, say) never
          // re-validates a directory that has since vanished. A change to blank is an explicit
          // `null` (clear), never an omitted field: omitting it would leave the old directory in
          // place while the emptied input says otherwise.
          if (typed !== (editing[field] ?? "")) changes[field] = typed || null;
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
        const dirs: Partial<Record<ConfigDirField, string>> = {};
        for (const [field, typed] of typedDirs) {
          if (typed) dirs[field] = typed;
        }
        await request({
          type: "create_console",
          name,
          console_session_agent: consoleSessionAgent,
          default_agent: defaultAgent,
          ...(icon && { icon }),
          ...dirs,
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
          <ConsoleAvatar icon={icon} className="size-12" />
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
        const { field, placeholder } = AGENT_CONFIG_DIR[agent];
        return (
          <TextInput
            key={agent}
            label={t("dialog.console.configDir", { agent: AGENT_LABEL[agent] })}
            value={configDirs[field]}
            onChange={(value) => setConfigDirs({ ...configDirs, [field]: value })}
            placeholder={placeholder}
            dir="ltr"
          />
        );
      })}
      <DialogError message={error} />
    </Dialog>
  );
}
