import { useState } from "react";

import { choiceGroups, choiceKey, currentChoice, findEntry, inheritedAccount, initialChoice, type AccountChoice } from "../accountChoices";
import { AGENT_LABEL, AGENT_OPTIONS, inheritedSessionLaunch, isAgentUnavailable, launchWithOverrides, projectSessionAgent } from "../agents";
import { agentIconPickerOptions } from "../components/AgentIcon";
import { useT } from "../i18n/react";
import type { Agent, Console, Project } from "../protocol";
import { useDaemonStore } from "../store";
import type { Option } from "./OptionSelect";

type Overrides = { model?: string | null; effort?: string | null };

/**
 * What a dialog that starts a session of some agent asks for: the agent and its account, which
 * open on what a new session of the project (or, without one, the console) would use and which the
 * user can change, and the model and reasoning effort, which follow the project or console until
 * changed. `exclude` leaves one agent out of the choice altogether (a handoff never goes to the
 * session's own agent). `overrides` holds only what the user changed, for the request.
 */
export function useLaunchChoice(parentConsole: Console, project: Project | undefined, exclude?: Agent) {
  const t = useT();
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const [pickedAgent, setPickedAgent] = useState<Agent>();
  const [pickedAccounts, setPickedAccounts] = useState<Partial<Record<Agent, string | null>>>({});
  const [launchSettings, setLaunchSettings] = useState<Partial<Record<Agent, Overrides>>>({});

  const groups = choiceGroups(accounts, agentAvailability, t("settings.accounts.defaultName")).filter((group) => group.agent !== exclude);
  const pickedAccount = pickedAgent ? pickedAccounts[pickedAgent] : undefined;
  const pickedEntry = groups.find((group) => group.agent === pickedAgent)?.entries.find((entry) => entry.account === pickedAccount);
  const picked = pickedAgent ? {
    agent: pickedAgent,
    account: pickedEntry ? pickedEntry.account : inheritedAccount(parentConsole, project, pickedAgent, accounts),
  } : undefined;

  const candidates = AGENT_OPTIONS.map((option) => option.value).filter((agent) => agent !== exclude);
  const selectable = candidates.filter((agent) => !isAgentUnavailable(agentAvailability, agent));
  let initial: AccountChoice;
  if (exclude === undefined) {
    initial = initialChoice(parentConsole, project, accounts, agentAvailability);
  } else {
    const wanted = project ? project.default_agent ?? projectSessionAgent(parentConsole) : parentConsole.default_agent;
    const agent = selectable.includes(wanted) ? wanted : selectable[0] ?? candidates[0];
    initial = { agent, account: inheritedAccount(parentConsole, project, agent, accounts) };
  }
  const choice = currentChoice(picked, initial, groups);
  const accountEntries = groups.find((group) => group.agent === choice.agent)?.entries ?? [];
  const overrides = launchSettings[choice.agent];
  const { model, effort } = launchWithOverrides(inheritedSessionLaunch(parentConsole, project, choice.agent), overrides);
  const blocked = selectable.length === 0;

  const changeLaunchSetting = (field: "model" | "effort") => (value: string | null) =>
    setLaunchSettings((current) => ({ ...current, [choice.agent]: { ...current[choice.agent], [field]: value } }));

  return {
    choice,
    /** No agent that may be picked. */
    blocked,
    overrides,
    agentOptions: agentIconPickerOptions(t, agentAvailability, choice.agent).filter((option) => option.value !== exclude),
    setPickedAgent,
    /** The account half of the joined agent and account field; absent when the agent has one account. */
    account: accountEntries.length > 1 ? {
      label: t("dialog.agentSettings.account", { agent: AGENT_LABEL[choice.agent] }),
      options: accountEntries.map((entry): Option<string> => ({ value: entry.key, label: entry.name, isDisabled: blocked })),
      value: choiceKey(choice),
      onChange: (key: string) => {
        const entry = findEntry(groups, key);
        if (entry) {
          setPickedAgent(entry.agent);
          setPickedAccounts((current) => ({ ...current, [entry.agent]: entry.account }));
        }
      },
    } : undefined,
    /** The props `LaunchSettingsFields` takes from the choice. */
    launch: {
      agent: choice.agent,
      account: choice.account,
      model,
      effort,
      onModelChange: changeLaunchSetting("model"),
      onEffortChange: changeLaunchSetting("effort"),
    },
  };
}
