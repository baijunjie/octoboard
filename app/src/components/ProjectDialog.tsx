import React, { useState } from "react";

import { AGENT_OPTIONS } from "../agents";
import type { Agent, Project, ProjectSource } from "../protocol";
import { useDaemon } from "../store";
import { DirectoryPicker } from "./DirectoryPicker";
import { Dropdown } from "./Dropdown";
import { Modal } from "./Modal";

const SOURCE_OPTIONS: { value: ProjectSource; label: string }[] = [
  { value: "local", label: "A single directory" },
  { value: "parent", label: "A parent directory (discover git repos beneath it)" },
  { value: "github", label: "A GitHub URL (clone it)" },
];

/** `""` stands for "unset" — falls back to the console's default agent per the "Which agent gets
 * used" priority in docs/mvp.md. */
const DEFAULT_AGENT_OPTIONS: { value: Agent | ""; label: string }[] = [
  { value: "", label: "Use console default" },
  ...AGENT_OPTIONS,
];

interface ProjectDialogProps {
  consoleId: string;
  /** Editing an existing project when set — only name and default agent can change (see
   * `daemon/PROTOCOL.md`'s `update_project`); association details are immutable once added. */
  project?: Project;
  onClose: () => void;
}

export function ProjectDialog({ consoleId, project: editing, onClose }: ProjectDialogProps): React.ReactElement {
  const { request } = useDaemon();
  const [source, setSource] = useState<ProjectSource>(editing?.source ?? "local");
  const [path, setPath] = useState(editing?.path ?? "");
  const [remoteUrl, setRemoteUrl] = useState(editing?.remote_url ?? "");
  const [name, setName] = useState(editing?.name ?? "");
  const [defaultAgent, setDefaultAgent] = useState<Agent | "">(editing?.default_agent ?? "");
  const [pickingDirectory, setPickingDirectory] = useState(false);
  const [error, setError] = useState<string>();
  const [saving, setSaving] = useState(false);

  const submit = async () => {
    setError(undefined);
    if (!editing) {
      // `path` is required for every source — for `github` it is the parent directory the clone
      // lands in, not something the daemon can default — and `remote_url` additionally for
      // `github`. Checked here so an obviously incomplete request never reaches the daemon only to
      // come back as a raw "`path` is required" field-name error.
      if (!path.trim()) {
        setError("A directory is required.");
        return;
      }
      if (source === "github" && !remoteUrl.trim()) {
        setError("A repository URL is required.");
        return;
      }
    } else if (!name.trim()) {
      // `name: name || undefined` below means "blank leaves it alone" everywhere else this pattern
      // is used (the field is genuinely optional on creation), but here blanking it out and saving
      // would silently keep the old name instead of doing what the empty field visually suggests.
      setError("Name is required.");
      return;
    }
    setSaving(true);
    try {
      if (editing) {
        await request({
          type: "update_project",
          project: editing.id,
          name: name || undefined,
          // "" means "use console default", which on the wire is an explicit `null` (clear),
          // never an omitted field — omitting it means "leave whatever was there alone" instead.
          default_agent: defaultAgent === "" ? null : defaultAgent,
        });
      } else {
        await request({
          type: "add_project",
          console_id: consoleId,
          source,
          path: path || undefined,
          remote_url: source === "github" ? remoteUrl : undefined,
          name: name || undefined,
          default_agent: defaultAgent || undefined,
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
        {editing ? "Save" : "Add"}
      </button>
    </>
  );

  return (
    <>
      <Modal title={editing ? "Edit project" : "Add project"} onClose={onClose} footer={footer} onSubmit={submit}>
        {!editing && (
          <label className="field">
            <span>Source</span>
            <Dropdown options={SOURCE_OPTIONS} value={source} onChange={setSource} aria-label="Project source" />
          </label>
        )}
        {!editing && source === "github" && (
          <label className="field">
            <span>Repository URL</span>
            <input type="text" value={remoteUrl} onChange={(e) => setRemoteUrl(e.target.value)} placeholder="https://github.com/owner/repo" />
          </label>
        )}
        {!editing && (
          <label className="field">
            <span>{source === "github" ? "Clone into (parent directory)" : "Directory"}</span>
            <div className="field-with-button">
              <input type="text" value={path} onChange={(e) => setPath(e.target.value)} placeholder="~/code" />
              <button type="button" onClick={() => setPickingDirectory(true)}>
                Browse…
              </button>
            </div>
          </label>
        )}
        <label className="field">
          <span>Name {editing ? "" : "(optional)"}</span>
          <input type="text" value={name} onChange={(e) => setName(e.target.value)} placeholder={editing ? undefined : "derived from the directory"} />
        </label>
        <label className="field">
          <span>Default agent</span>
          <Dropdown options={DEFAULT_AGENT_OPTIONS} value={defaultAgent} onChange={setDefaultAgent} aria-label="Project default agent" />
        </label>
        {error && <p className="error-text">{error}</p>}
      </Modal>
      {pickingDirectory && (
        <DirectoryPicker
          title="Choose a directory"
          initialPath={path || "~"}
          onPick={(picked) => {
            setPath(picked);
            setPickingDirectory(false);
          }}
          onClose={() => setPickingDirectory(false)}
        />
      )}
    </>
  );
}
