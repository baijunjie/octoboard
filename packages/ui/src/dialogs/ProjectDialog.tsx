import { Button } from "@heroui/react";
import React, { useMemo, useState } from "react";

import { AGENT_LABEL } from "../agents";
import { AGENT_ICON_OPTIONS, AgentIcon } from "../components/AgentIcon";
import type { PlainMessageKey } from "../i18n/catalog";
import { useT } from "../i18n/react";
import { tagVocabulary } from "../projectFiltering";
import type { Agent, Project, ProjectSource } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { Dialog, DialogError, useDialogAction } from "./Dialog";
import { DirectoryPicker } from "./DirectoryPicker";
import { OptionSelect } from "./OptionSelect";
import { TagsInput } from "./TagsInput";
import { TextInput } from "./TextInput";

const SOURCE_OPTIONS: { value: ProjectSource; label: PlainMessageKey }[] = [
  { value: "local", label: "dialog.project.source.local" },
  { value: "parent", label: "dialog.project.source.parent" },
  { value: "github", label: "dialog.project.source.github" },
];

export function ProjectDialog({
  consoleId,
  project: editing,
  onClose,
}: {
  consoleId: string;
  /** Editing an existing project when set — only name, default agent and tags can change (see
   * `apps/daemon/PROTOCOL.md`'s `update_project`); association details are immutable once added. */
  project?: Project;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const consoleAgent = useDaemonStore((s) => s.consoles.get(consoleId)?.default_agent);
  const [source, setSource] = useState<ProjectSource>(editing?.source ?? "local");
  const [path, setPath] = useState(editing?.path ?? "");
  const [remoteUrl, setRemoteUrl] = useState(editing?.remote_url ?? "");
  const [name, setName] = useState(editing?.name ?? "");
  const [defaultAgent, setDefaultAgent] = useState<Agent | "">(editing?.default_agent ?? "");
  const [tags, setTags] = useState<string[]>(editing?.tags ?? []);
  // The tags to offer are the ones this console's projects carry now; there is no registry of them.
  const projects = useDaemonStore((s) => s.projects);
  const knownTags = useMemo(
    () => tagVocabulary([...projects.values()].filter((p) => p.console_id === consoleId)),
    [projects, consoleId],
  );
  const [pickingDirectory, setPickingDirectory] = useState(false);
  const { error, setError, busy, run } = useDialogAction();

  const submit = () => {
    if (!editing) {
      // `path` is required for every source — for `github` it is the parent directory the clone
      // lands in, not something the daemon can default — and `remote_url` additionally for
      // `github`. Checked here so an obviously incomplete request never reaches the daemon only to
      // come back as a raw "`path` is required" field-name error.
      if (!path.trim()) {
        setError(t("dialog.project.directoryRequired"));
        return;
      }
      if (source === "github" && !remoteUrl.trim()) {
        setError(t("dialog.project.urlRequired"));
        return;
      }
    } else if (!name.trim()) {
      // `name: name || undefined` below means "blank leaves it alone" everywhere else this pattern
      // is used (the field is genuinely optional on creation), but here blanking it out and saving
      // would silently keep the old name instead of doing what the empty field visually suggests.
      setError(t("dialog.nameRequired"));
      return;
    }
    void run(async () => {
      if (editing) {
        await request({
          type: "update_project",
          project: editing.id,
          name: name || undefined,
          // "" ("Auto") means following the console's default, which on the wire is an explicit `null` (clear),
          // never an omitted field — omitting it means "leave whatever was there alone" instead.
          default_agent: defaultAgent === "" ? null : defaultAgent,
          // Always sent, the whole list: a present array replaces the tags, and the field shows them all.
          tags,
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
          tags,
        });
      }
      onClose();
    });
  };

  // `""` is "Auto": the project sets none and follows its console's default agent (the "Which agent
  // a session uses" section of docs/product/sessions.md); the option names that agent, its icon
  // faded, so it says what it resolves to.
  const defaultAgentOptions: { value: Agent | ""; label: string; icon?: React.ReactNode }[] = [
    {
      value: "",
      label: consoleAgent ? t("dialog.project.agentAuto", { agent: AGENT_LABEL[consoleAgent] }) : t("dialog.project.agentAutoBare"),
      icon: consoleAgent && <AgentIcon agent={consoleAgent} className="size-4 opacity-50" />,
    },
    ...AGENT_ICON_OPTIONS,
  ];

  return (
    <>
      <Dialog
        title={editing ? t("dialog.project.edit") : t("dialog.project.add")}
        onClose={onClose}
        submitLabel={editing ? t("common.save") : t("common.add")}
        busy={busy}
        onSubmit={submit}
      >
        {!editing && <OptionSelect label={t("dialog.project.source")} options={SOURCE_OPTIONS.map((option) => ({ ...option, label: t(option.label) }))} value={source} onChange={setSource} />}
        {!editing && source === "github" && (
          <TextInput
            label={t("dialog.project.repositoryUrl")}
            value={remoteUrl}
            onChange={setRemoteUrl}
            dir="ltr"
            placeholder={t("dialog.project.urlExample")}
          />
        )}
        {!editing && (
          <div className="flex items-end gap-2">
            <div className="min-w-0 flex-1">
              <TextInput
                label={source === "github" ? t("dialog.project.cloneInto") : t("dialog.project.directory")}
                value={path}
                onChange={setPath}
                dir="ltr"
                placeholder={t("dialog.project.directoryExample")}
              />
            </div>
            <Button type="button" variant="secondary" onPress={() => setPickingDirectory(true)}>
              {t("dialog.project.browse")}
            </Button>
          </div>
        )}
        <TextInput
          label={editing ? t("common.name") : t("common.nameOptional")}
          value={name}
          onChange={setName}
          placeholder={editing ? undefined : t("dialog.project.nameExample")}
          description={editing ? undefined : t("dialog.project.nameHint")}
        />
        <TagsInput
          label={t("dialog.project.tags")}
          value={tags}
          onChange={setTags}
          suggestions={knownTags}
          placeholder={t("dialog.project.tagsExample")}
          description={t("dialog.project.tagsHint")}
        />
        <OptionSelect
          label={t("dialog.project.defaultAgent")}
          options={defaultAgentOptions}
          value={defaultAgent}
          onChange={setDefaultAgent}
        />
        <DialogError message={error} />
      </Dialog>
      {pickingDirectory && (
        <DirectoryPicker
          title={t("dialog.project.chooseDirectory")}
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
