import React, { useState } from "react";

import { AGENT_CONFIG_DIR, AGENT_LABEL, AGENT_OPTIONS } from "../agents";
import { useT } from "../i18n/react";
import type { Agent, ConfigDirField, Console } from "../protocol";
import { useDaemon } from "../store";
import { Dialog, DialogError, useDialogAction } from "./Dialog";
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
  const [name, setName] = useState(editing?.name ?? "");
  const [hubAgent, setHubAgent] = useState<Agent>(editing?.hub_agent ?? "claude");
  const [defaultAgent, setDefaultAgent] = useState<Agent>(editing?.default_agent ?? "claude");
  const [configDirs, setConfigDirs] = useState<Record<ConfigDirField, string>>({
    claude_config_dir: editing?.claude_config_dir ?? "",
    codex_config_dir: editing?.codex_config_dir ?? "",
    grok_config_dir: editing?.grok_config_dir ?? "",
  });
  const { error, setError, busy, run } = useDialogAction();

  // One row per agent the dialog currently selects, in a fixed order. An agent that is not selected
  // keeps whatever is stored for it: its row is neither shown nor sent.
  const shownAgents = AGENT_OPTIONS.map((option) => option.value).filter(
    (agent) => agent === hubAgent || agent === defaultAgent,
  );

  // What was typed for each shown row, trimmed.
  const typedDirs = shownAgents.map((agent) => {
    const { field } = AGENT_CONFIG_DIR[agent];
    return [field, configDirs[field].trim()] as const;
  });

  const submit = () => {
    if (!name.trim()) {
      setError(t("dialog.nameRequired"));
      return;
    }
    void run(async () => {
      if (editing) {
        const changes: Partial<Record<ConfigDirField, string | null>> = {};
        for (const [field, typed] of typedDirs) {
          // Sent only when the input changed, so saving something else (a rename, say) never
          // re-validates a directory that has since vanished. A change to blank is an explicit
          // `null` (clear), never an omitted field: omitting it would leave the old directory in
          // place while the emptied input says otherwise.
          if (typed !== (editing[field] ?? "")) changes[field] = typed || null;
        }
        await request({
          type: "update_console",
          console: editing.id,
          name,
          hub_agent: hubAgent,
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
          hub_agent: hubAgent,
          default_agent: defaultAgent,
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
      busy={busy}
      onSubmit={submit}
    >
      <TextInput label={t("common.name")} value={name} onChange={setName} autoFocus />
      <OptionSelect label={t("dialog.console.hubAgent")} options={AGENT_OPTIONS} value={hubAgent} onChange={setHubAgent} />
      <OptionSelect label={t("dialog.console.defaultAgent")} options={AGENT_OPTIONS} value={defaultAgent} onChange={setDefaultAgent} />
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
