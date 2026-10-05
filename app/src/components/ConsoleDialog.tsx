import React, { useState } from "react";

import { AGENT_CONFIG_DIR, AGENT_LABEL, AGENT_OPTIONS } from "../agents";
import type { Agent, ConfigDirField, Console } from "../protocol";
import { useDaemon } from "../store";
import { Dropdown } from "./Dropdown";
import { Modal } from "./Modal";

interface ConsoleDialogProps {
  /** Editing an existing console when set, creating a new one otherwise. */
  console?: Console;
  onClose: () => void;
}

export function ConsoleDialog({ console: editing, onClose }: ConsoleDialogProps): React.ReactElement {
  const { request } = useDaemon();
  const [name, setName] = useState(editing?.name ?? "");
  const [hubAgent, setHubAgent] = useState<Agent>(editing?.hub_agent ?? "claude");
  const [defaultAgent, setDefaultAgent] = useState<Agent>(editing?.default_agent ?? "claude");
  const [configDirs, setConfigDirs] = useState<Record<ConfigDirField, string>>({
    claude_config_dir: editing?.claude_config_dir ?? "",
    codex_config_dir: editing?.codex_config_dir ?? "",
    grok_config_dir: editing?.grok_config_dir ?? "",
  });
  const [error, setError] = useState<string>();
  const [saving, setSaving] = useState(false);

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

  const submit = async () => {
    if (!name.trim()) {
      setError("Name is required.");
      return;
    }
    setSaving(true);
    setError(undefined);
    try {
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
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setSaving(false);
    }
  };

  const footer = (
    <>
      <button type="button" onClick={onClose} disabled={saving}>
        Cancel
      </button>
      <button type="submit" disabled={saving}>
        {editing ? "Save" : "Create"}
      </button>
    </>
  );

  return (
    <Modal title={editing ? "Edit console" : "New console"} onClose={onClose} footer={footer} onSubmit={submit}>
      <label className="field">
        <span>Name</span>
        <input type="text" value={name} onChange={(e) => setName(e.target.value)} autoFocus />
      </label>
      <label className="field">
        <span>Hub agent</span>
        <Dropdown options={AGENT_OPTIONS} value={hubAgent} onChange={setHubAgent} aria-label="Hub agent" />
      </label>
      <label className="field">
        <span>Default agent</span>
        <Dropdown options={AGENT_OPTIONS} value={defaultAgent} onChange={setDefaultAgent} aria-label="Default agent" />
      </label>
      {shownAgents.map((agent) => {
        const { field, placeholder } = AGENT_CONFIG_DIR[agent];
        return (
          <label className="field" key={agent}>
            <span>{AGENT_LABEL[agent]} config directory (optional)</span>
            <input
              type="text"
              value={configDirs[field]}
              onChange={(e) => setConfigDirs({ ...configDirs, [field]: e.target.value })}
              placeholder={placeholder}
            />
          </label>
        );
      })}
      {error && <p className="error-text">{error}</p>}
    </Modal>
  );
}
