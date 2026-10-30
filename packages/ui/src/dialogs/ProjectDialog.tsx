import React, { useId, useMemo, useState } from "react";

import { orderedAccounts } from "../accountOrder";
import { Button } from "../components/Button";
import { accountDisabled, consoleAccount, resolveProjectAccount, settledProjectAccount } from "../accountChoices";
import { changedLaunch, changedProjectAccounts, readLaunch, readProjectAccounts } from "../agentFields";
import {
  AGENT_LABEL,
  consoleProjectLaunch,
  defaultAccountName,
  projectSessionAgent,
} from "../agents";
import { agentIconPickerOptions, AgentIcon } from "../components/AgentIcon";
import { DaemonErrorMessage } from "../components/DaemonErrorMessage";
import { FailureAlert } from "../components/FailureAlert";
import { HelpTip } from "../components/HelpTip";
import { InlineReference } from "../components/InlineReference";
import { changedGitWorkflowItems, FOLLOWING_GIT_WORKFLOW } from "../gitWorkflow";
import type { PlainMessageKey } from "../i18n/catalog";
import { Message, useT } from "../i18n/react";
import { tagVocabulary } from "../projectFiltering";
import type { Agent, Project, ProjectAccount, ProjectSource } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { markDisabledPickable } from "./accountOptions";
import { AdvancedSection, AdvancedTabs } from "./AdvancedSection";
import { AgentAccountField } from "./AgentAccountField";
import { Dialog, useDialogAction, useSubmitValidation } from "./Dialog";
import { DirectoryPicker } from "./DirectoryPicker";
import { GitWorkflowField } from "./GitWorkflowField";
import { LaunchSettingsFields } from "./LaunchSettingsFields";
import { OptionSelect, type Option } from "./OptionSelect";
import { TagsInput } from "./TagsInput";
import { TextAreaInput } from "./TextAreaInput";
import { TextInput } from "./TextInput";
import { useDetectedAgent } from "./useDetectedAgent";
import { projectNameDefault } from "./projectNameDefault";
import { exceedsTextLimit, PROJECT_DESCRIPTION_LIMIT, PROJECT_NAME_LIMIT } from "./textLength";

const SOURCE_OPTIONS: { value: ProjectSource; label: PlainMessageKey }[] = [
  { value: "local", label: "dialog.project.source.local" },
  { value: "parent", label: "dialog.project.source.parent" },
  { value: "git", label: "dialog.project.source.git" },
];

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

/** The refusals of a directory's check that say the path entered is wrong, where any other says the check could not run. */
const LOCATION_PROBLEMS: ReadonlySet<string> = new Set(["path_not_found", "path_not_a_directory"]);

export function ProjectDialog({
  consoleId,
  project: editing,
  onClose,
}: {
  consoleId: string;
  /** Editing an existing project when set; association details (source, directory, remote) are
   * immutable once added. */
  project?: Project;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const owner = useDaemonStore((s) => s.consoles.get(consoleId));
  // The agent "Same as console" resolves to: what a project session of the console is of when
  // its project names none.
  const consoleAgent = owner && projectSessionAgent(owner);
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const addedAgents = useDaemonStore((s) => s.settings.added_agents);
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const accountOrder = useDaemonStore((s) => s.settings.account_order);
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  const disabledDefaults = useDaemonStore((s) => s.settings.disabled_default_accounts);
  const [source, setSource] = useState<ProjectSource>(editing?.source ?? "local");
  const defaultCloneDir = useDaemonStore((s) => s.settings.default_clone_dir);
  const home = useDaemonStore((s) => s.homeDir);
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
  // console's. Kept per agent as the values are the agent's own, so a value set for one says nothing
  // about another.
  const [initialLaunch] = useState(() => readLaunch(editing?.launch));
  const [launch, setLaunch] = useState(initialLaunch);
  const [initialAccounts] = useState(() => readProjectAccounts(editing?.accounts));
  const [accountFields, setAccountFields] = useState(initialAccounts);
  // The project's own git workflow settings when the dialog opens, and the edits since; `null` follows the application-wide value.
  const [initialGitWorkflow] = useState(() => editing?.git_workflow ?? FOLLOWING_GIT_WORKFLOW);
  const [gitWorkflow, setGitWorkflow] = useState(initialGitWorkflow);
  // Whether the Advanced section is open, kept here so it stays open as the section with launch settings and the
  // one without them take each other's place.
  const [advancedExpanded, setAdvancedExpanded] = useState(false);
  const [description, setDescription] = useState(editing?.description ?? "");
  const [tags, setTags] = useState<string[]>(editing?.tags ?? []);
  // The tags to offer are the ones this console's projects carry now; there is no registry of them.
  const projects = useDaemonStore((s) => s.projects);
  const knownTags = useMemo(
    () => tagVocabulary([...projects.values()].filter((p) => p.console_id === consoleId)),
    [projects, consoleId],
  );
  const [pickingDirectory, setPickingDirectory] = useState(false);
  const { busy, run } = useDialogAction(undefined, t(editing ? "dialog.project.saveFailedTitle" : "dialog.project.addFailedTitle"));

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
  const detectionAlertId = useId();
  // The lookup found the location wrong (no such path, not a directory): that is the location field's own error.
  const locationProblem = source === "local" && detection.status === "failed" && typeof detection.error !== "string" && LOCATION_PROBLEMS.has(detection.error.code) ? detection.error : undefined;
  const detectionAlert = detection.status === "failed" && !locationProblem ? (
    // A lookup the form makes on its own that could not run, so its failure is an Alert directly under the field
    // it is about, which describes that field.
    <FailureAlert id={detectionAlertId} announce title={t("dialog.project.detectFailedTitle")} message={detection.error} />
  ) : undefined;
  const locked = !editing && (source === "git" ? detection.status !== "ready" : detection.status === "checking" || detection.status === "failed");

  // `path` is required for the `local` and `parent` sources; `git` needs no check here, since its
  // `path` the daemon defaults when blank and a blank URL keeps the form locked. Checked here so an
  // obviously incomplete request never reaches the daemon only to come back as a raw "`path` is
  // required" field-name error.
  // A parent directory adds a project per repository it finds, so the name and the description,
  // which each belong to one project, are neither shown nor sent.
  const addsParent = !editing && source === "parent";
  const pathError = !editing && source !== "git" && !path ? t("dialog.project.directoryRequired") : undefined;
  // `name: name || undefined` below means "blank leaves it alone" everywhere else this pattern is
  // used (the field is genuinely optional on creation), but here blanking it out and saving would
  // silently keep the old name instead of doing what the empty field visually suggests.
  const nameError = addsParent ? undefined : editing && !name.trim() ? t("dialog.nameRequired")
    : exceedsTextLimit(name, PROJECT_NAME_LIMIT, editing?.name) ? t("daemon.field_too_long.name", { limit: PROJECT_NAME_LIMIT }) : undefined;
  const descriptionError = !addsParent && exceedsTextLimit(description, PROJECT_DESCRIPTION_LIMIT, editing?.description ?? "")
    ? t("daemon.field_too_long.description", { limit: PROJECT_DESCRIPTION_LIMIT }) : undefined;
  const { shown, attempt } = useSubmitValidation();

  // The agent the account and the launch settings belong to: the one picked, else the console's
  // project session agent for "Same as console", and none for "Detect from files", which
  // stands for a different agent per repository.
  const agent = defaultAgent === "detect" ? undefined : defaultAgent === "" ? consoleAgent : defaultAgent;
  // Offered only where there is something to choose between: an agent with no account of its own
  // has the console's reference to inherit and nothing else.
  const ownAccounts = agent ? accounts.filter((account) => account.agent === agent) : [];
  const consoleAccountId = owner && agent ? consoleAccount(owner, agent, accounts) : null;
  const picked = agent ? settledProjectAccount(accountFields[agent], agent, accounts) : INHERIT;
  const saved = agent ? settledProjectAccount(initialAccounts[agent], agent, accounts) : INHERIT;
  // The account the project's sessions of `agent` run under, whose models the launch settings offer.
  const launchAccount = resolveProjectAccount(picked, consoleAccountId);

  const submit = () => {
    if (locked || !attempt(pathError, nameError, descriptionError)) return;
    void run(async () => {
      // Under "Detect from files" the controls are out of sight, so what was set in them is not sent.
      const sent = defaultAgent === "detect" ? initialLaunch : launch;
      const sentAccounts = defaultAgent === "detect" ? initialAccounts : accountFields;
      const trimmedDescription = addsParent ? "" : description.trim();
      // On add every item starts out following, so what changed is what the project holds a value of its own for.
      const gitWorkflowChanges = changedGitWorkflowItems(gitWorkflow, initialGitWorkflow);
      const gitWorkflowChanged = Object.keys(gitWorkflowChanges).length > 0;
      if (editing) {
        await request({
          type: "update_project",
          project: editing.id,
          name: name || undefined,
          // "" ("Same as console") means following the console's default, which on the wire
          // is an explicit `null` (clear), never an omitted field — omitting it means "leave
          // whatever was there alone" instead.
          default_agent: defaultAgent === "" || defaultAgent === "detect" ? null : defaultAgent,
          launch: changedLaunch("update", sent, initialLaunch),
          accounts: changedProjectAccounts("update", sentAccounts, initialAccounts),
          // Always sent, the whole list: a present array replaces the tags, and the field shows them all.
          tags,
          // Cleared with an explicit `null`, and sent only when it changed.
          ...(trimmedDescription !== (editing.description ?? "") && { description: trimmedDescription || null }),
          // Only the items that changed, `null` for one that goes back to following the application-wide value.
          ...(gitWorkflowChanged && { git_workflow: gitWorkflowChanges }),
        });
      } else {
        await request({
          type: "add_project",
          console_id: consoleId,
          source,
          path: path || undefined,
          remote_url: source === "git" ? remoteUrl : undefined,
          name: addsParent ? undefined : name || undefined,
          default_agent: defaultAgent === "" || defaultAgent === "detect" ? undefined : defaultAgent,
          // What the form shows is what is stored: "Same as console" is not detected again,
          // whether it was preselected after a pre-read or chosen; only "Detect from files" is.
          detect_default_agent: defaultAgent === "detect" ? undefined : false,
          launch: changedLaunch("create", sent, initialLaunch),
          accounts: changedProjectAccounts("create", sentAccounts, initialAccounts),
          tags,
          ...(trimmedDescription && { description: trimmedDescription }),
          // Only the items set to a value of their own; the rest follow the application-wide value.
          ...(gitWorkflowChanged && { git_workflow: gitWorkflowChanges }),
        });
      }
      onClose();
    });
  };

  // `""` is "Same as console": the project sets none and follows its console's project session
  // agent (the "Which agent a session uses" section of docs/product/opening-sessions.md); the option names
  // that agent in a badge, its icon faded, so it says what it resolves to.
  const defaultAgentOptions: Option<Agent | "" | "detect">[] = [
    ...(addsParent ? [{ value: "detect" as const, label: t("dialog.project.agentDetect") }] : []),
    {
      value: "",
      label: t("dialog.agentSettings.sameAsConsole"),
      badge: consoleAgent && AGENT_LABEL[consoleAgent],
      badgeInLabel: true,
      icon: consoleAgent && <AgentIcon agent={consoleAgent} className="size-4 opacity-50" />,
    },
    ...agentIconPickerOptions(t, agentAvailability, addedAgents, defaultAgent === "" || defaultAgent === "detect" ? undefined : defaultAgent),
  ];
  const gitWorkflowTab = {
    id: "gitWorkflow",
    label: t("dialog.project.gitWorkflow.title"),
    content: <GitWorkflowField value={gitWorkflow} onChange={setGitWorkflow} isDisabled={locked} />,
  };

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
          <div className="flex flex-col gap-2">
            <TextInput
              label={t("dialog.project.repositoryUrl")}
              value={remoteUrl}
              onChange={setRemoteUrl}
              dir="ltr"
              placeholder={t("dialog.project.urlExample")}
              description={detection.status === "checking" ? t("dialog.project.urlChecking") : undefined}
              describedBy={detectionAlert ? detectionAlertId : undefined}
            />
            {detectionAlert}
          </div>
        )}
        {!editing && (
          <div className="flex flex-col gap-2">
            <TextInput
              label={source === "git" ? t("dialog.project.cloneInto") : t("dialog.project.directory")}
              value={path}
              onChange={setPath}
              dir="ltr"
              // The first text field when adding; the source picker above it is not one, and the
              // repository URL only appears after a switch, which must not take focus off the picker.
              autoFocus
              placeholder={source === "git" ? defaultCloneDir : t("dialog.project.directoryExample")}
              description={source === "local" && detection.status === "checking" ? t("dialog.project.pathChecking") : undefined}
              errorMessage={shown(pathError) ?? (locationProblem && <DaemonErrorMessage error={locationProblem} part="summary" />)}
              describedBy={source === "local" && !shown(pathError) && detectionAlert ? detectionAlertId : undefined}
              isDisabled={source === "git" && locked}
              trailing={
                <Button type="button" variant="secondary" isDisabled={source === "git" && locked} onPress={() => setPickingDirectory(true)}>
                  {t("common.browse")}
                </Button>
              }
            />
            {source === "local" && !shown(pathError) && detectionAlert}
          </div>
        )}
        {!addsParent && (
          <TextInput
            label={t("common.name")}
            value={name}
            onChange={setName}
            autoFocus={!!editing}
            placeholder={editing ? t("dialog.project.nameExample") : projectNameDefault(source, path, remoteUrl, home) ?? t(source === "git" ? "dialog.project.repositoryNameDefault" : "dialog.project.directoryNameDefault")}
            description={editing ? undefined : t("dialog.project.nameHint")}
            errorMessage={shown(nameError)}
            characterLimit={PROJECT_NAME_LIMIT}
            isDisabled={locked}
          />
        )}
        {!addsParent && (
          <TextAreaInput
            label={t("dialog.project.description")}
            value={description}
            onChange={setDescription}
            placeholder={t("dialog.project.descriptionPlaceholder")}
            description={t("dialog.project.descriptionNote")}
            errorMessage={shown(descriptionError)}
            characterLimit={PROJECT_DESCRIPTION_LIMIT}
            rows={3}
            isDisabled={locked}
          />
        )}
        <TagsInput
          label={t("dialog.project.tags")}
          value={tags}
          onChange={setTags}
          suggestions={knownTags}
          placeholder={t(tags.length ? "dialog.project.noAdditionalTags" : "dialog.project.noTags")}
          isDisabled={locked}
        />
        <AgentAccountField
          label={t("dialog.project.defaultAgent")}
          options={defaultAgentOptions}
          value={defaultAgent}
          onChange={setDefaultAgent}
          help={
            addsParent ? (
              <HelpTip label={t("dialog.project.agentDetectHelp")}>
                <Message
                  id="dialog.project.agentDetectMechanism"
                  params={{
                    detect: <InlineReference dir="auto" wrap>{t("dialog.project.agentDetect")}</InlineReference>,
                    claudeFile: <InlineReference monospace wrap>CLAUDE.md</InlineReference>,
                    agentsFile: <InlineReference monospace wrap>AGENTS.md</InlineReference>,
                    claudeDir: <InlineReference monospace wrap>.claude</InlineReference>,
                    codexDir: <InlineReference monospace wrap>.codex</InlineReference>,
                  }}
                />
              </HelpTip>
            ) : undefined
          }
          description={addsParent ? t("dialog.project.agentDetectHint") : undefined}
          isDisabled={locked}
          account={
            agent && ownAccounts.length > 0
              ? {
                label: t("dialog.agentSettings.account", { agent: AGENT_LABEL[agent] }),
                options: [
                  {
                    value: accountValue(INHERIT),
                    label: t("dialog.agentSettings.sameAsConsole"),
                    badge: ownAccounts.find((account) => account.id === consoleAccountId)?.name ?? defaultAccountName(t, defaultNames, agent),
                    badgeInLabel: true,
                  },
                  // A turned-off account is hidden, but for the one the project pinned when the dialog opened (or
                  // pins now), which stays, marked.
                  ...orderedAccounts(agent, accounts, accountOrder).map((account) => ({
                    choice: account ? { kind: "account", id: account.id } as const : { kind: "default" } as const,
                    label: account?.name ?? defaultAccountName(t, defaultNames, agent),
                    off: accountDisabled(accounts, disabledDefaults, agent, account?.id ?? null),
                  }))
                    .filter(({ choice, off }) => !off || accountValue(choice) === accountValue(picked) || accountValue(choice) === accountValue(saved))
                    .map(({ choice, label, off }) => markDisabledPickable(t, { value: accountValue(choice), label }, off)),
                ],
                value: accountValue(picked),
                onChange: (value) => setAccountFields({ ...accountFields, [agent]: accountOfValue(value) }),
              }
              : undefined
          }
        />
        {agent ? (
          <LaunchSettingsFields
            agent={agent}
            account={launchAccount}
            model={launch[agent].model}
            effort={launch[agent].effort}
            onModelChange={(model) => setLaunch({ ...launch, [agent]: { ...launch[agent], model } })}
            onEffortChange={(effort) => setLaunch({ ...launch, [agent]: { ...launch[agent], effort } })}
            inherit={owner ? consoleProjectLaunch(owner, agent) : undefined}
            isDisabled={locked}
            expanded={advancedExpanded}
            onExpandedChange={setAdvancedExpanded}
            otherTab={gitWorkflowTab}
          />
        ) : (
          // "Detect from files" has no launch settings to offer, so the Advanced section holds the Git workflow
          // tab alone.
          <AdvancedSection expanded={advancedExpanded} onExpandedChange={setAdvancedExpanded}>
            <AdvancedTabs tabs={[gitWorkflowTab]} />
          </AdvancedSection>
        )}
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
