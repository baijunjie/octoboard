import React, { useState } from "react";

import { choiceGroups, choiceKey, currentChoice, findEntry, inheritedAccount, initialChoice } from "../accountChoices";
import { AGENT_CONSOLE_EFFORT_FIELD, AGENT_CONSOLE_MODEL_FIELD, AGENT_LABEL, AGENT_PROJECT_EFFORT_FIELD, AGENT_PROJECT_MODEL_FIELD, noAgentAvailable } from "../agents";
import { agentIconPickerOptions } from "../components/AgentIcon";
import { FadeOverflow } from "../components/FadeOverflow";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import type { Agent, Console, Project, Session } from "../protocol";
import { sessionDisplayTitle } from "../sessionLabel";
import { BindingBadge } from "../sidebar/BindingBadge";
import { liveConsoleSessions } from "../sidebar/order";
import { useDaemon, useDaemonStore } from "../store";
import { AgentAccountField } from "./AgentAccountField";
import { Dialog, DialogError, useDialogAction } from "./Dialog";
import type { NewSessionBinding } from "./dialogRequest";
import { LaunchSettingsFields } from "./LaunchSettingsFields";
import { OptionSelect } from "./OptionSelect";
import { TextInput } from "./TextInput";

/** `OptionSelect`'s value for "no owner"; a session id is never this. */
const NO_OWNER = "none";

/**
 * Opens a console session or a session manually under a project. Joined pickers settle the agent and its account
 * together. They open on the agent per the "Which agent a session uses" section of
 * docs/product/sessions.md (project default, then console default) with the account that agent
 * resolves to when none is chosen (the project's reference, else the console's), and the user can
 * pick another agent or one of its accounts for this session; the choice goes out as `open_session`'s `agent` and
 * `account` fields.
 *
 * `open_session` answers with `session_opened`, which names the session it started — the broadcast
 * that puts it in the tree carries no request id, so this reply is the only way to tell which of
 * the sessions appearing there is ours to select.
 *
 * Where the dialog was opened from settles the session's binding (`binding`). From the project
 * list it is a choice: the console's console sessions that are not archived, each beside its colour
 * (the binding badge's own dot), and "none", which is the default, since a session the user starts
 * stays outside the orchestration unless they say otherwise; with no console session to offer there
 * is no choice at all and the session goes out unbound. From a project's focus mode it is never
 * offered and the session is always unbound. From a console session's focus mode the session is
 * bound to it, shown as a fixed line rather than a field. The binding is immutable once the session
 * opens, so this is the only place it is ever set; it goes out as `open_session`'s `bound_to`.
 */
export function SessionDialog({
  console: parentConsole,
  project,
  binding = { kind: "unbound" },
  sessions,
  onClose,
  onOpened,
}: {
  console: Console;
  project?: Project;
  binding?: NewSessionBinding;
  sessions: Session[];
  onClose: () => void;
  onOpened: (sessionId: string) => void;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const blocked = noAgentAvailable(agentAvailability);
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const [pickedAgent, setPickedAgent] = useState<Agent>();
  const [pickedAccounts, setPickedAccounts] = useState<Partial<Record<Agent, string | null>>>({});
  const [title, setTitle] = useState("");
  const [launchSettings, setLaunchSettings] = useState<Partial<Record<Agent, { model?: string | null; effort?: string | null }>>>({});
  const [pickedOwner, setPickedOwner] = useState(NO_OWNER);
  const { error, busy, run } = useDialogAction();

  const groups = choiceGroups(accounts, agentAvailability, t("settings.accounts.defaultName"));
  const pickedAccount = pickedAgent ? pickedAccounts[pickedAgent] : undefined;
  const pickedEntry = groups.find((group) => group.agent === pickedAgent)?.entries.find((entry) => entry.account === pickedAccount);
  const picked = pickedAgent ? {
    agent: pickedAgent,
    account: pickedEntry ? pickedEntry.account : inheritedAccount(parentConsole, project, pickedAgent, accounts),
  } : undefined;
  const choice = currentChoice(picked, initialChoice(parentConsole, project, accounts, agentAvailability), groups);
  const accountEntries = groups.find((group) => group.agent === choice.agent)?.entries ?? [];
  const overrides = launchSettings[choice.agent];
  const inheritedModel = project ? project[AGENT_PROJECT_MODEL_FIELD[choice.agent]] : parentConsole[AGENT_CONSOLE_MODEL_FIELD[choice.agent]];
  const inheritedEffort = project ? project[AGENT_PROJECT_EFFORT_FIELD[choice.agent]] : parentConsole[AGENT_CONSOLE_EFFORT_FIELD[choice.agent]];
  const model = overrides?.model === undefined ? inheritedModel ?? null : overrides.model;
  const effort = overrides?.effort === undefined ? inheritedEffort ?? null : overrides.effort;

  // The select is shown from the moment there is a console session to offer and stays mounted
  // for the rest of the dialog's life, even if the list later empties, leaving "none" alone: it is
  // a focusable control, and unmounting it under the user would drop focus to the body and take
  // Escape and Tab containment with it. Hiding the field when the dialog opens with nothing to
  // offer is the rule for that state; a list that empties under the user is not that state, and a
  // field vanishing would also hide that their choice went with it.
  //
  // `owner` is only ever one still offered, never the picked id as it stands: `open_session` does
  // not check the target's status, so sending an archived console session's id would create a live
  // session bound to an archived one, whose reports would be refused. An archived pick therefore
  // falls back to none, visibly in the select. `pickedOwner` keeps the stale id, so reopening that
  // console session while the dialog is still open restores the choice, which is harmless. A fixed
  // owner that is no longer live cannot fall back to anything, so it stops the dialog instead.
  const sessionBinding: NewSessionBinding = project ? binding : { kind: "unbound" };
  const owners = liveConsoleSessions(sessions, parentConsole.id);
  const storedFixedOwner = useDaemonStore((s) => sessionBinding.kind === "bound" ? s.sessions.get(sessionBinding.to.id) : undefined);
  const fixedOwner = sessionBinding.kind === "bound"
    ? storedFixedOwner ?? sessions.find((s) => s.id === sessionBinding.to.id) ?? sessionBinding.to
    : undefined;
  const fixedOwnerGone = fixedOwner !== undefined && !owners.some((s) => s.id === fixedOwner.id);
  const owner = fixedOwner ?? (sessionBinding.kind === "choose" ? owners.find((s) => s.id === pickedOwner) : undefined);
  const [ownerOffered, setOwnerOffered] = useState(owners.length > 0);
  if (owners.length > 0 && !ownerOffered) setOwnerOffered(true);

  const submit = () =>
    void run(async () => {
      const reply = await request({
        type: "open_session",
        console_id: parentConsole.id,
        ...(project ? { project_id: project.id, bound_to: owner?.id } : {}),
        agent: choice.agent,
        account: choice.account,
        title: title || undefined,
        ...overrides,
      });
      if (reply.type === "session_opened") onOpened(reply.session.id);
      onClose();
    });

  return (
    <Dialog
      title={project
        ? t("dialog.session.title", { project: isolate(project.name) })
        : t("dialog.consoleSession.title", { console: isolate(parentConsole.name) })}
      onClose={onClose}
      submitLabel={t("dialog.session.open")}
      busy={busy}
      submitDisabled={blocked || fixedOwnerGone}
      onSubmit={submit}
    >
      {blocked && <p className="text-sm text-app-danger">{t("agents.installPrompt")}</p>}
      <TextInput
        label={t("common.name")}
        value={title}
        onChange={setTitle}
        placeholder={project?.name ?? parentConsole.name}
        description={t(project ? "dialog.session.titleHint" : "dialog.consoleSession.consoleNameHint")}
      />
      <AgentAccountField
        label={t("dialog.session.agent")}
        options={agentIconPickerOptions(t, agentAvailability)}
        value={choice.agent}
        onChange={setPickedAgent}
        account={accountEntries.length > 1 ? {
          label: t("dialog.agentSettings.account", { agent: AGENT_LABEL[choice.agent] }),
          options: accountEntries.map((entry) => ({ value: entry.key, label: entry.name, isDisabled: blocked })),
          value: choiceKey(choice),
          onChange: (key) => {
            const entry = findEntry(groups, key);
            if (entry) {
              setPickedAgent(entry.agent);
              setPickedAccounts((current) => ({ ...current, [entry.agent]: entry.account }));
            }
          },
        } : undefined}
      />
      {fixedOwner && (
        <div data-marquee-scope className="flex items-center gap-2 text-sm">
          <BindingBadge owner={fixedOwner} decorative />
          <FadeOverflow dir="auto" className="min-w-0 flex-1" titleWhenClipped={t("dialog.session.ownerFixed", { name: isolate(sessionDisplayTitle(t, fixedOwner)) })}>
            {t("dialog.session.ownerFixed", { name: isolate(sessionDisplayTitle(t, fixedOwner)) })}
          </FadeOverflow>
        </div>
      )}
      {fixedOwner && fixedOwnerGone && <p className="text-sm text-app-danger">{t("dialog.session.ownerGone", { name: isolate(sessionDisplayTitle(t, fixedOwner)) })}</p>}
      {sessionBinding.kind === "choose" && ownerOffered && (
        <OptionSelect
          label={t("dialog.session.owner")}
          value={owner?.id ?? NO_OWNER}
          onChange={setPickedOwner}
          options={[
            { value: NO_OWNER, label: t("dialog.session.ownerNone") },
            ...owners.map((s) => ({ value: s.id, label: sessionDisplayTitle(t, s), icon: <BindingBadge owner={s} decorative /> })),
          ]}
        />
      )}
      <LaunchSettingsFields
        agent={choice.agent}
        account={choice.account}
        model={model}
        effort={effort}
        onModelChange={(value) => setLaunchSettings((current) => ({ ...current, [choice.agent]: { ...current[choice.agent], model: value } }))}
        onEffortChange={(value) => setLaunchSettings((current) => ({ ...current, [choice.agent]: { ...current[choice.agent], effort: value } }))}
        note={t(project ? "dialog.session.launchSettingsNote" : "dialog.consoleSession.launchSettingsNote")}
        isDisabled={blocked}
      />
      <DialogError message={error} />
    </Dialog>
  );
}
