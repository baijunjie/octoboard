import { Button } from "@heroui/react";
import React, { useState } from "react";

import { AGENT_OPTIONS } from "../agents";
import type { Agent, Project, ProjectSource } from "../protocol";
import { useDaemon } from "../store";
import { Dialog, DialogError, useDialogAction } from "./Dialog";
import { DirectoryPicker } from "./DirectoryPicker";
import { OptionSelect } from "./OptionSelect";
import { TextInput } from "./TextInput";

const SOURCE_OPTIONS: { value: ProjectSource; label: string }[] = [
  { value: "local", label: "A single directory" },
  { value: "parent", label: "A parent directory (discover git repos beneath it)" },
  { value: "github", label: "A GitHub URL (clone it)" },
];

/** `""` stands for "unset" — falls back to the console's default agent per the "Which agent a
 * session uses" section of docs/product/sessions.md. */
const DEFAULT_AGENT_OPTIONS: { value: Agent | ""; label: string }[] = [
  { value: "", label: "Use console default" },
  ...AGENT_OPTIONS,
];

export function ProjectDialog({
  consoleId,
  project: editing,
  onClose,
}: {
  consoleId: string;
  /** Editing an existing project when set — only name and default agent can change (see
   * `daemon/PROTOCOL.md`'s `update_project`); association details are immutable once added. */
  project?: Project;
  onClose: () => void;
}): React.ReactElement {
  const { request } = useDaemon();
  const [source, setSource] = useState<ProjectSource>(editing?.source ?? "local");
  const [path, setPath] = useState(editing?.path ?? "");
  const [remoteUrl, setRemoteUrl] = useState(editing?.remote_url ?? "");
  const [name, setName] = useState(editing?.name ?? "");
  const [defaultAgent, setDefaultAgent] = useState<Agent | "">(editing?.default_agent ?? "");
  const [pickingDirectory, setPickingDirectory] = useState(false);
  const { error, setError, busy, run } = useDialogAction();

  const submit = () => {
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
    void run(async () => {
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
    });
  };

  return (
    <>
      <Dialog
        title={editing ? "Edit project" : "Add project"}
        onClose={onClose}
        submitLabel={editing ? "Save" : "Add"}
        busy={busy}
        onSubmit={submit}
      >
        {!editing && <OptionSelect label="Source" options={SOURCE_OPTIONS} value={source} onChange={setSource} />}
        {!editing && source === "github" && (
          <TextInput
            label="Repository URL"
            value={remoteUrl}
            onChange={setRemoteUrl}
            placeholder="https://github.com/owner/repo"
          />
        )}
        {!editing && (
          <div className="flex items-end gap-2">
            <div className="min-w-0 flex-1">
              <TextInput
                label={source === "github" ? "Clone into (parent directory)" : "Directory"}
                value={path}
                onChange={setPath}
                placeholder="~/code"
              />
            </div>
            <Button type="button" variant="secondary" onPress={() => setPickingDirectory(true)}>
              Browse…
            </Button>
          </div>
        )}
        <TextInput
          label={editing ? "Name" : "Name (optional)"}
          value={name}
          onChange={setName}
          placeholder={editing ? undefined : "derived from the directory"}
        />
        <OptionSelect
          label="Default agent"
          options={DEFAULT_AGENT_OPTIONS}
          value={defaultAgent}
          onChange={setDefaultAgent}
        />
        <DialogError message={error} />
      </Dialog>
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
