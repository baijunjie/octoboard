import React, { useEffect, useRef, useState } from "react";

import { settingText } from "../settings/accounts/providerSettingCopy";
import { ActionableError } from "../errors/actionableError";
import { changedAccounts, changedLaunch, readAccounts, readLaunch, withAgent, withLaunchField, restoreLaunchField, launchFieldValue } from "../agents/agentFields";
import { selectableAgent, servedAgents } from "../agents/agents";
import { defaultProviderAgent, providerLabel } from "../agents/providers";
import { imageToAvatar } from "./avatarImage";
import { focusOpenDialog } from "../components/focus/modalFocus";
import { AgentIcon, agentIconPickerOptions } from "../components/icons/AgentIcon";
import { useT } from "../i18n/react";
import type { Agent, Console } from "../protocol";
import { SettingsDialog } from "../settings/SettingsDialog";
import { useDaemon, useDaemonStore, useProviders } from "../store";
import { AvatarField } from "./AvatarField";
import { ConsoleAccountsField } from "./ConsoleAccountsField";
import { type ConsoleAgentSettings, ConsoleLaunchFields } from "./ConsoleLaunchFields";
import { Dialog } from "../components/dialog/Dialog";
import { useDialogAction, useSubmitValidation } from "../components/dialog/dialogHooks";
import { GroupRow, GroupRows } from "../components/form/GroupRows";
import { LabelledGroup } from "../components/form/LabelledGroup";
import { OptionSelect } from "../components/form/OptionSelect";
import { TextAreaInput } from "../components/form/TextAreaInput";
import { TextInput } from "../components/form/TextInput";
import { raiseError } from "../components/alerts/raiseError";
import { ChangedElsewhereAnnouncer } from "../components/announce/ChangedElsewhereAnnouncer";
import { useCheckedEdit } from "../components/hooks/useCheckedEdit";
import { agentFieldPaths, agentPaths } from "../errors/writeConflict";

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
  const addedAgents = useDaemonStore((s) => s.settings.added_agents);
  const providers = useProviders();
  const served = servedAgents(providers);
  // What the form read of the console, which an edit is judged against: only a field the user changed is sent, so a
  // change another client made to a field they left alone is not overwritten, nor conflicts.
  const [read, setRead] = useState(() => ({
    name: editing?.name ?? "",
    default_agent: editing?.default_agent,
    project_session_agent: editing?.project_session_agent ?? "",
    icon: editing?.icon ?? null,
    console_session_prompt: editing?.console_session_prompt ?? "",
  }));
  const [name, setName] = useState(read.name);
  // A new console opens on the first registered agent, or on the first agent that may be picked when that one is
  // known to be missing or not added, so the agent never starts on an entry the picker would refuse.
  const initialAgent = selectableAgent(agentAvailability, addedAgents, providers, defaultProviderAgent(providers));
  const [defaultAgent, setDefaultAgent] = useState<Agent>(read.default_agent ?? initialAgent);
  // `""` is "Same as console sessions": the console sets no agent of its own for its project
  // sessions and they follow the console sessions' agent, which on the wire is an explicit `null`
  // when saving.
  const [projectAgent, setProjectAgent] = useState<Agent | "">(read.project_session_agent);
  // What the console holds for each agent when the dialog opens, and the edits since: the account
  // (`null` is that agent's default account) and what its console session is launched with (`null`
  // being the agent's own default), kept per agent as the values are the agent's own, so a value set
  // for one says nothing about another.
  const [initial, setInitial] = useState<ConsoleAgentSettings>(() => ({
    accounts: readAccounts(editing?.accounts, served),
    launch: readLaunch(editing?.launch, served),
    project_session_launch: readLaunch(editing?.project_session_launch, served),
  }));
  const [settings, setSettings] = useState(initial);
  const [icon, setIcon] = useState<string | null>(read.icon);
  const [sessionPrompt, setSessionPrompt] = useState(read.console_session_prompt);
  const stored = useDaemonStore((s) => (editing ? s.consoles.get(editing.id) : undefined));
  // Whether the store holds `path` as the form read it: a version that moved over the same value (every one does after a
  // daemon restart) changes nothing of what the form shows.
  const sameAsRead = (path: string) => {
    if (!stored) return true;
    const [field, agent = "", key = "", setting = ""] = path.split(".");
    if (field === "name") return stored.name === read.name;
    if (field === "default_agent") return stored.default_agent === read.default_agent;
    if (field === "project_session_agent") return (stored.project_session_agent ?? "") === read.project_session_agent;
    if (field === "icon") return (stored.icon ?? null) === read.icon;
    if (field === "console_session_prompt") return (stored.console_session_prompt ?? "") === read.console_session_prompt;
    if (field === "accounts") return (stored.accounts?.[agent] ?? null) === (initial.accounts[agent] ?? null);
    if (field === "launch" || field === "project_session_launch") {
      return launchFieldValue(stored[field]?.[agent], key, setting) === launchFieldValue(initial[field][agent], key, setting);
    }
    return true;
  };
  // Whether the user changed `path`'s field from what the form read, which is what makes its replacement worth announcing.
  const editedFromRead = (path: string) => {
    const [field, agent = "", key = "", setting = ""] = path.split(".");
    if (field === "name") return name !== read.name;
    if (field === "default_agent") return defaultAgent !== read.default_agent;
    if (field === "project_session_agent") return projectAgent !== read.project_session_agent;
    if (field === "icon") return icon !== read.icon;
    if (field === "console_session_prompt") return sessionPrompt.trim() !== read.console_session_prompt;
    if (field === "accounts") return (settings.accounts[agent] ?? null) !== (initial.accounts[agent] ?? null);
    if (field === "launch" || field === "project_session_launch") {
      return launchFieldValue(settings[field][agent], key, setting) !== launchFieldValue(initial[field][agent], key, setting);
    }
    return false;
  };
  // The label of the field a path belongs to, as the dialog shows it.
  const fieldLabel = (path: string) => {
    const field = path.split(".")[0];
    if (field === "name") return t("common.name");
    if (field === "default_agent" || field === "project_session_agent") return t("dialog.console.defaultAgent");
    if (field === "icon") return t("dialog.console.avatar");
    if (field === "console_session_prompt") return t("dialog.console.sessionInstructions");
    if (field === "accounts") return t("dialog.console.accounts");
    // A launch setting is named as its picker is: the kind of session, then the setting.
    const [, agent = "", key = "", setting = ""] = path.split(".");
    if (key === "compatibility") {
      const label = settingText(t, providers, agent, setting, "heading") ?? setting;
      const kind = t(field === "launch" ? "dialog.console.launchConsole" : "dialog.console.launchProject");
      return `${kind} ${label}`;
    }
    const kind = t(field === "launch" ? "dialog.console.launchConsole" : "dialog.console.launchProject");
    return `${kind} ${t(key === "effort" ? "dialog.agentSettings.effort" : "dialog.agentSettings.model")}`;
  };
  const resetToStored = (paths: readonly string[]) => {
    if (!stored) return;
    let nextRead = read;
    let nextInitial = initial;
    let nextSettings = settings;
    for (const path of paths) {
      const [field, agent = "", key = "", setting = ""] = path.split(".");
      if (field === "name") {
        setName(stored.name);
        nextRead = { ...nextRead, name: stored.name };
      } else if (field === "default_agent") {
        setDefaultAgent(stored.default_agent);
        nextRead = { ...nextRead, default_agent: stored.default_agent };
      } else if (field === "project_session_agent") {
        setProjectAgent(stored.project_session_agent ?? "");
        nextRead = { ...nextRead, project_session_agent: stored.project_session_agent ?? "" };
      } else if (field === "icon") {
        setIcon(stored.icon ?? null);
        nextRead = { ...nextRead, icon: stored.icon ?? null };
      } else if (field === "console_session_prompt") {
        setSessionPrompt(stored.console_session_prompt ?? "");
        nextRead = { ...nextRead, console_session_prompt: stored.console_session_prompt ?? "" };
      } else if (field === "accounts") {
        const account = stored.accounts?.[agent] ?? null;
        nextInitial = { ...nextInitial, accounts: withAgent(nextInitial.accounts, agent, account) };
        nextSettings = { ...nextSettings, accounts: withAgent(nextSettings.accounts, agent, account) };
      } else if (field === "launch" || field === "project_session_launch") {
        nextInitial = { ...nextInitial, [field]: restoreLaunchField(nextInitial[field], agent, stored[field]?.[agent], key, setting) };
        nextSettings = { ...nextSettings, [field]: restoreLaunchField(nextSettings[field], agent, stored[field]?.[agent], key, setting) };
      }
    }
    setRead(nextRead);
    setInitial(nextInitial);
    setSettings(nextSettings);
  };
  const { busy, run } = useDialogAction(undefined, t(editing ? "dialog.console.saveFailedTitle" : "dialog.console.createFailedTitle"));
  const edit = useCheckedEdit(editing?.versions ?? {}, stored?.versions, { same: sameAsRead, edited: editedFromRead, reset: resetToStored, writing: busy });
  // Settings, opened over this dialog from the accounts table. It is rendered beside the dialog
  // rather than inside it, as its forms would otherwise submit this one.
  const [managingAccounts, setManagingAccounts] = useState(false);
  const accountsOpener = useRef<HTMLButtonElement | null>(null);
  useEffect(() => {
    if (managingAccounts || !accountsOpener.current) return;
    // A backdrop press can leave focus on `<body>` after React Aria's restore, most visibly when an
    // accessibility activation opened Settings. Only that focus is handed back to the opener. The second
    // frame is slack beyond the restore's own, which the packaged app was verified with.
    let frame = requestAnimationFrame(() => {
      frame = requestAnimationFrame(() => {
        if (document.activeElement === document.body) focusOpenDialog(accountsOpener.current);
      });
    });
    return () => cancelAnimationFrame(frame);
  }, [managingAccounts]);

  // While a picked image is being read, saving would send the old avatar.
  const [decoding, setDecoding] = useState(false);

  const chooseImage = (file: File) => {
    setDecoding(true);
    imageToAvatar(file)
      .then(setIcon, () => {
        // The avatar's own failure is not a field's: it opens the error alert dialog, as a failed submit does.
        raiseError(new ActionableError(t("dialog.console.avatarUnreadable"), "image"), { title: t("dialog.console.avatarFailedTitle") });
      })
      .finally(() => setDecoding(false));
  };

  const { shown, attempt } = useSubmitValidation();
  const nameError = name ? undefined : t("dialog.nameRequired");

  const submit = () => {
    if (decoding) return;
    if (!attempt(nameError)) return;
    const prompt = sessionPrompt.trim();
    void run(async () => {
      if (editing) {
        const changes = {
          ...(name !== read.name && { name }),
          ...(defaultAgent !== read.default_agent && { default_agent: defaultAgent }),
          // Back to Same as console sessions is an explicit `null`.
          ...(projectAgent !== read.project_session_agent && { project_session_agent: projectAgent || null }),
          // The avatar is cleared the same way, with an explicit `null`.
          ...(icon !== read.icon && { icon }),
          // Cleared the same way, with an explicit `null`.
          ...(prompt !== read.console_session_prompt && { console_session_prompt: prompt || null }),
        };
        const accounts = changedAccounts("update", settings.accounts, initial.accounts);
        const launch = changedLaunch("update", settings.launch, initial.launch);
        const projectLaunch = changedLaunch("update", settings.project_session_launch, initial.project_session_launch);
        // The paths of what is sent, each written as it is on the record, are what the write is based on.
        const paths = [
          ...Object.keys(changes),
          ...agentPaths("accounts", accounts),
          ...agentFieldPaths("launch", launch),
          ...agentFieldPaths("project_session_launch", projectLaunch),
        ];
        await request({
          type: "update_console",
          console: editing.id,
          ...changes,
          accounts,
          launch,
          project_session_launch: projectLaunch,
          base_versions: edit.base(paths),
        }).catch((error: unknown) => {
          edit.refused(error);
          throw error;
        });
      } else {
        await request({
          type: "create_console",
          name,
          default_agent: defaultAgent,
          ...(projectAgent && { project_session_agent: projectAgent }),
          ...(icon && { icon }),
          ...(prompt && { console_session_prompt: prompt }),
          accounts: changedAccounts("create", settings.accounts, initial.accounts),
          launch: changedLaunch("create", settings.launch, initial.launch),
          project_session_launch: changedLaunch("create", settings.project_session_launch, initial.project_session_launch),
        });
      }
      onClose();
    });
  };

  return (
    <>
      <Dialog
        title={editing ? t("dialog.console.edit") : t("dialog.console.new")}
        onClose={onClose}
        submitLabel={editing ? t("common.save") : t("common.create")}
        // Reading a picked image holds Save pending too: it is brief, and Save would otherwise look live while it
        // does nothing.
        busy={busy || decoding}
        onSubmit={submit}
        size="wide"
      >
        <ChangedElsewhereAnnouncer count={edit.replaced.count} labels={edit.replaced.paths.map(fieldLabel)} onSaid={edit.said} />
        {/* The avatar is centred on the name's label and input by a fixed offset rather than by
          alignment, so it stays put when the name's error appears under them. The two are 66px
          together, and 62px above the tablet boundary, where HeroUI's input drops to a smaller text size. */}
        <div className="flex items-start gap-3">
          <div className="mt-2.25 tablet:mt-1.75">
            <AvatarField
              icon={icon}
              onPick={chooseImage}
              onRemove={() => setIcon(null)}
              isPending={decoding}
            />
          </div>
          <div className="min-w-0 flex-1">
            <TextInput
              label={t("common.name")}
              value={name}
              onChange={setName}
              placeholder={t("dialog.console.nameExample")}
              errorMessage={shown(nameError)}
              autoFocus
            />
          </div>
        </div>
        <LabelledGroup label={t("dialog.console.defaultAgent")} description={t("dialog.console.defaultAgentNote")}>
          <GroupRows>
            <GroupRow label={t("dialog.console.defaultAgentConsole")}>
              <OptionSelect
                hideLabel
                label={t("dialog.console.defaultAgentConsole")}
                options={agentIconPickerOptions(t, agentAvailability, addedAgents, providers, defaultAgent)}
                value={defaultAgent}
                onChange={setDefaultAgent}
              />
            </GroupRow>
            <GroupRow label={t("dialog.console.defaultAgentProject")}>
              <OptionSelect<Agent | "">
                hideLabel
                label={t("dialog.console.defaultAgentProject")}
                options={[
                  {
                    value: "",
                    label: t("dialog.console.projectAgentSame"),
                    badge: providerLabel(t, providers, defaultAgent),
                    icon: <AgentIcon agent={defaultAgent} className="size-4 opacity-50" />,
                  },
                  ...agentIconPickerOptions(t, agentAvailability, addedAgents, providers, projectAgent || undefined),
                ]}
                value={projectAgent}
                onChange={setProjectAgent}
              />
            </GroupRow>
          </GroupRows>
        </LabelledGroup>
        <ConsoleAccountsField
          label={t("dialog.console.accounts")}
          description={t("dialog.console.accountsNote")}
          values={settings.accounts}
          onChange={(agent, account) => setSettings({ ...settings, accounts: { ...settings.accounts, [agent]: account } })}
          onManageAccounts={(opener) => {
            accountsOpener.current = opener;
            setManagingAccounts(true);
          }}
        />
        {/* What is inside is the console's own: its sessions' launch settings and its instructions. */}
        <ConsoleLaunchFields
          defaultAgent={defaultAgent}
          values={settings}
          onChange={(kind, agent, ...edit) =>
            setSettings({ ...settings, [kind]: withLaunchField(settings[kind], agent, ...edit) })}
        >
          <TextAreaInput
            label={t("dialog.console.sessionInstructions")}
            value={sessionPrompt}
            onChange={setSessionPrompt}
            placeholder={t("dialog.console.sessionInstructionsPlaceholder")}
            description={t("dialog.console.sessionInstructionsPrecedenceNote")}
          />
        </ConsoleLaunchFields>
      </Dialog>
      {managingAccounts && <SettingsDialog initialSection="accounts" onClose={() => setManagingAccounts(false)} />}
    </>
  );
}
