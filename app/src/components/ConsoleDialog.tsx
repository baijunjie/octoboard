import React, { useState } from "react";

import { AGENT_OPTIONS } from "../agents";
import type { Agent, Console } from "../protocol";
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
  const [error, setError] = useState<string>();
  const [saving, setSaving] = useState(false);

  const submit = async () => {
    if (!name.trim()) {
      setError("Name is required.");
      return;
    }
    setSaving(true);
    setError(undefined);
    try {
      if (editing) {
        await request({
          type: "update_console",
          console: editing.id,
          name,
          hub_agent: hubAgent,
          default_agent: defaultAgent,
        });
      } else {
        await request({ type: "create_console", name, hub_agent: hubAgent, default_agent: defaultAgent });
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
      {error && <p className="error-text">{error}</p>}
    </Modal>
  );
}
