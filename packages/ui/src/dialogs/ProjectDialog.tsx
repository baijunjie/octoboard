import { Button } from "@heroui/react";
import React, { useMemo, useState } from "react";

import { consoleAccount, resolveProjectAccount, settledProjectAccount } from "../accountChoices";
import { changedAgentFields, changedProjectAccounts, readAgentFields, readProjectAccounts } from "../agentFields";
import {
  AGENT_ACCOUNT_FIELD,
  AGENT_LABEL,
  AGENT_OPTIONS,
  AGENT_PROJECT_EFFORT_FIELD,
  AGENT_PROJECT_MODEL_FIELD,
} from "../agents";
import { AGENT_ICON_OPTIONS, AgentIcon } from "../components/AgentIcon";
import { InlineCode } from "../components/InlineCode";
import type { PlainMessageKey } from "../i18n/catalog";
import { Message, useT } from "../i18n/react";
import { tagVocabulary } from "../projectFiltering";
import type { Agent, Project, ProjectAccount, ProjectSource } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { AgentAccountField } from "./AgentAccountField";
import { Dialog, DialogError, useDialogAction, useSubmitValidation } from "./Dialog";
import { DirectoryPicker } from "./DirectoryPicker";
import { LaunchSettingsFields } from "./LaunchSettingsFields";
import { OptionSelect } from "./OptionSelect";
import { TagsInput } from "./TagsInput";
import { TextInput } from "./TextInput";
import { useDetectedAgent } from "./useDetectedAgent";

const SOURCE_OPTIONS: { value: ProjectSource; label: PlainMessageKey }[] = [
  { value: "local", label: "dialog.project.source.local" },
  { value: "parent", label: "dialog.project.source.parent" },
  { value: "git", label: "dialog.project.source.git" },
];

/** Every per-agent text field of a project: each agent's model and effort. The account is not one,
 * as it has three states (`ProjectAccount`) and is held apart. */
const FIELDS = AGENT_OPTIONS.flatMap(({ value: agent }) => [
  AGENT_PROJECT_MODEL_FIELD[agent],
  AGENT_PROJECT_EFFORT_FIELD[agent],
]);

/** The account picker's option values: `""` inherits the console's, which the picker opens on;
 * `"default"` and `"account:<id>"` cannot be taken for one another or for an id. */
const accountValue = (account: ProjectAccount): string =>
  account.kind === "account" ? `account:${account.id}` : account.kind === "default" ? "default" : "";
const accountOfValue = (value: string): ProjectAccount =>
  value.startsWith("account:") ? { kind: "account", id: value.slice("account:".length) } : value === "default" ? { kind: "default" } : { kind: "inherit" };

const INHERIT: ProjectAccount = { kind: "inherit" };

/** How long the path or URL must stay unchanged before the daemon is asked about it; a repository
 * is asked about later, since each ask reaches over the network. */
const DETECT_DELAY_MS = { local: 400, git: 800 };

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
  const owner = useDaemonStore((s) => s.consoles.get(consoleId));
  const consoleAgent = owner?.default_agent;
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const [source, setSource] = useState<ProjectSource>(editing?.source ?? "local");
  const defaultCloneDir = useDaemonStore((s) => s.settings.default_clone_dir);
  // The clone directory is kept apart from the other sources' directory, so switching the source
  // back and forth loses neither; it starts at the default clone directory, which the user may change.
  const [directory, setDirectory] = useState(editing?.path ?? "");
  const [cloneDir, setCloneDir] = useState(defaultCloneDir);
  const path = source === "git" ? cloneDir : directory;
  const setPath = source === "git" ? setCloneDir : setDirectory;
  const [remoteUrl, setRemoteUrl] = useState(editing?.remote_url ?? "");
  const [name, setName] = useState(editing?.name ?? "");
  const [defaultAgent, setDefaultAgent] = useState<Agent | "" | "detect">(editing?.default_agent ?? "");
  // `"detect"` is the choice only a parent directory offers: each repository found is detected on
  // its own. It is selected on arriving at that source and given up on leaving it.
  const changeSource = (next: ProjectSource) => {
    setSource(next);
    if (next === "parent") setDefaultAgent("detect");
    else if (defaultAgent === "detect") setDefaultAgent("");
  };
  // What the project holds for each agent when the dialog opens, and the edits since: the model and
  // effort, `null` being unset (the agent's own), and the account, which may be inheriting the
  // console's. Kept per agent as the names are the agent's own, so a value set for one says nothing
  // about another.
  const [initial] = useState(() => readAgentFields(FIELDS, editing ?? {}));
  const [fields, setFields] = useState(initial);
  const [initialAccounts] = useState(() => readProjectAccounts(editing ?? {}));
  const [accountFields, setAccountFields] = useState(initialAccounts);
  const [tags, setTags] = useState<string[]>(editing?.tags ?? []);
  // The tags to offer are the ones this console's projects carry now; there is no registry of them.
  const projects = useDaemonStore((s) => s.projects);
  const knownTags = useMemo(
    () => tagVocabulary([...projects.values()].filter((p) => p.console_id === consoleId)),
    [projects, consoleId],
  );
  const [pickingDirectory, setPickingDirectory] = useState(false);
  const { error, busy, run } = useDialogAction();

  // A single directory's agent is read as its path is entered, a git remote's by probing it, which
  // also checks the remote can be read. A parent directory has no pre-read: each repository found
  // is detected on adding.
  const detection = useDetectedAgent(
    request,
    editing
      ? undefined
      : source === "local" && path
        ? { type: "detect_directory_agent", path }
        : source === "git" && remoteUrl
          ? { type: "probe_git_remote", remote_url: remoteUrl }
          : undefined,
    source === "git" ? DETECT_DELAY_MS.git : DETECT_DELAY_MS.local,
    setDefaultAgent,
  );
  // Whatever the form asks the daemon about locks the fields below it until the answer for what is
  // entered now has arrived: a failure keeps them locked, and a git URL is asked about from the
  // first character, so it stays locked while empty too.
  const locked = !editing && (source === "git" ? detection.status !== "ready" : detection.status === "checking" || detection.status === "failed");

  // `path` is required for the `local` and `parent` sources; `git` needs no check here, since its
  // `path` the daemon defaults when blank and a blank URL keeps the form locked. Checked here so an
  // obviously incomplete request never reaches the daemon only to come back as a raw "`path` is
  // required" field-name error.
  const pathError = !editing && source !== "git" && !path ? t("dialog.project.directoryRequired") : undefined;
  // `name: name || undefined` below means "blank leaves it alone" everywhere else this pattern is
  // used (the field is genuinely optional on creation), but here blanking it out and saving would
  // silently keep the old name instead of doing what the empty field visually suggests.
  const nameError = editing && !name ? t("dialog.nameRequired") : undefined;
  const { shown, attempt } = useSubmitValidation();

  // The agent the account and the launch settings belong to: the one picked, else the console's for
  // "Inherit from console", and none for "Detect from files", which stands for a different agent
  // per repository.
  const agent = defaultAgent === "detect" ? undefined : defaultAgent === "" ? consoleAgent : defaultAgent;
  const accountField = agent && AGENT_ACCOUNT_FIELD[agent];
  const modelField = agent && AGENT_PROJECT_MODEL_FIELD[agent];
  const effortField = agent && AGENT_PROJECT_EFFORT_FIELD[agent];
  // Offered only where there is something to choose between: an agent with no account of its own
  // has the console's reference to inherit and nothing else.
  const ownAccounts = agent ? accounts.filter((account) => account.agent === agent) : [];
  const consoleAccountId = owner && agent ? consoleAccount(owner, agent, accounts) : null;
  const picked = accountField && agent ? settledProjectAccount(accountFields[accountField], agent, accounts) : INHERIT;
  // The account the project's sessions of `agent` run under, whose models the launch settings offer.
  const launchAccount = resolveProjectAccount(picked, consoleAccountId);

  const submit = () => {
    if (locked || !attempt(pathError, nameError)) return;
    void run(async () => {
      // Under "Detect from files" the controls are out of sight, so what was set in them is not sent.
      const sent = defaultAgent === "detect" ? initial : fields;
      const sentAccounts = defaultAgent === "detect" ? initialAccounts : accountFields;
      if (editing) {
        await request({
          type: "update_project",
          project: editing.id,
          name: name || undefined,
          // "" ("Inherit from console") means following the console's default, which on the wire
          // is an explicit `null` (clear), never an omitted field — omitting it means "leave
          // whatever was there alone" instead.
          default_agent: defaultAgent === "" || defaultAgent === "detect" ? null : defaultAgent,
          ...changedAgentFields("update", sent, initial),
          ...changedProjectAccounts("update", sentAccounts, initialAccounts),
          // Always sent, the whole list: a present array replaces the tags, and the field shows them all.
          tags,
        });
      } else {
        await request({
          type: "add_project",
          console_id: consoleId,
          source,
          path: path || undefined,
          remote_url: source === "git" ? remoteUrl : undefined,
          name: name || undefined,
          default_agent: defaultAgent === "" || defaultAgent === "detect" ? undefined : defaultAgent,
          // What the form shows is what is stored: "Inherit from console" is not detected again,
          // whether it was preselected after a pre-read or chosen; only "Detect from files" is.
          detect_default_agent: defaultAgent === "detect" ? undefined : false,
          ...changedAgentFields("create", sent, initial),
          ...changedProjectAccounts("create", sentAccounts, initialAccounts),
          tags,
        });
      }
      onClose();
    });
  };

  // `""` is "Inherit from console": the project sets none and follows its console's default agent
  // (the "Which agent a session uses" section of docs/product/sessions.md); the option names that
  // agent, its icon faded, so it says what it resolves to.
  const defaultAgentOptions: { value: Agent | "" | "detect"; label: string; icon?: React.ReactNode }[] = [
    ...(!editing && source === "parent" ? [{ value: "detect" as const, label: t("dialog.project.agentDetect") }] : []),
    {
      value: "",
      label: consoleAgent ? t("dialog.project.agentInherit", { agent: AGENT_LABEL[consoleAgent] }) : t("dialog.project.agentInheritBare"),
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
        submitDisabled={locked}
        onSubmit={submit}
      >
        {!editing && <OptionSelect label={t("dialog.project.source")} options={SOURCE_OPTIONS.map((option) => ({ ...option, label: t(option.label) }))} value={source} onChange={changeSource} />}
        {!editing && source === "git" && (
          <TextInput
            label={t("dialog.project.repositoryUrl")}
            value={remoteUrl}
            onChange={setRemoteUrl}
            dir="ltr"
            placeholder={t("dialog.project.urlExample")}
            description={detection.status === "checking" ? t("dialog.project.urlChecking") : undefined}
            errorMessage={detection.status === "failed" ? detection.error : undefined}
          />
        )}
        {!editing && (
          <TextInput
            label={source === "git" ? t("dialog.project.cloneInto") : t("dialog.project.directory")}
            value={path}
            onChange={setPath}
            dir="ltr"
            placeholder={source === "git" ? defaultCloneDir : t("dialog.project.directoryExample")}
            description={source === "local" && detection.status === "checking" ? t("dialog.project.pathChecking") : undefined}
            errorMessage={shown(pathError) ?? (source === "local" && detection.status === "failed" ? detection.error : undefined)}
            isDisabled={source === "git" && locked}
            trailing={
              <Button type="button" variant="secondary" isDisabled={source === "git" && locked} onPress={() => setPickingDirectory(true)}>
                {t("common.browse")}
              </Button>
            }
          />
        )}
        <TextInput
          label={t("common.name")}
          value={name}
          onChange={setName}
          placeholder={editing ? undefined : t("dialog.project.nameExample")}
          description={editing ? undefined : t("dialog.project.nameHint")}
          errorMessage={shown(nameError)}
          isDisabled={locked}
        />
        <TagsInput
          label={t("dialog.project.tags")}
          value={tags}
          onChange={setTags}
          suggestions={knownTags}
          placeholder={t("dialog.project.tagsExample")}
          isDisabled={locked}
        />
        <AgentAccountField
          label={t("dialog.project.defaultAgent")}
          options={defaultAgentOptions}
          value={defaultAgent}
          onChange={setDefaultAgent}
          description={
            !editing && source === "parent" ? (
              <Message
                id="dialog.project.agentDetectHint"
                params={{
                  claudeFile: <InlineCode>CLAUDE.md</InlineCode>,
                  agentsFile: <InlineCode>AGENTS.md</InlineCode>,
                  claudeDir: <InlineCode>.claude</InlineCode>,
                  codexDir: <InlineCode>.codex</InlineCode>,
                }}
              />
            ) : undefined
          }
          isDisabled={locked}
          account={
            agent && accountField && ownAccounts.length > 0
              ? {
                label: t("dialog.agentSettings.account", { agent: AGENT_LABEL[agent] }),
                options: [
                  {
                    value: accountValue(INHERIT),
                    label: t("dialog.project.accountInherit", {
                      account: ownAccounts.find((account) => account.id === consoleAccountId)?.name ?? t("settings.accounts.defaultName"),
                    }),
                  },
                  { value: accountValue({ kind: "default" }), label: t("settings.accounts.defaultName") },
                  ...ownAccounts.map((account) => ({ value: accountValue({ kind: "account", id: account.id }), label: account.name })),
                ],
                value: accountValue(picked),
                onChange: (value) => setAccountFields({ ...accountFields, [accountField]: accountOfValue(value) }),
              }
              : undefined
          }
        />
        {agent && modelField && effortField && (
          <LaunchSettingsFields
            agent={agent}
            account={launchAccount}
            model={fields[modelField]}
            effort={fields[effortField]}
            onModelChange={(value) => setFields({ ...fields, [modelField]: value })}
            onEffortChange={(value) => setFields({ ...fields, [effortField]: value })}
            note={t("dialog.project.launchSettingsNote")}
            isDisabled={locked}
          />
        )}
        <DialogError message={error} />
      </Dialog>
      {pickingDirectory && (
        <DirectoryPicker
          title={t("dialog.chooseDirectory")}
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
