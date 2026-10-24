import { useState } from "react";

import { accountOpenedOn, avoidUnavailableAccount, choiceChangingSharing, choiceDisabled, choiceExhaustion, choiceGroups, choiceKey, currentChoice, findEntry, type AccountChoice, type ChoiceGroup } from "../accountChoices";
import { AGENT_LABEL, agentsInListingOrder, defaultAccountName, inheritedSessionLaunch, launchWithOverrides, pickableAgents, projectSessionAgent, selectableAgent } from "../agents";
import { agentIconPickerOptions } from "../components/AgentIcon";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import type { Agent, Console, Project } from "../protocol";
import { quotaUsedUpCopy } from "../quota/quotaCopy";
import { useDaemonStore } from "../store";
import { useNow } from "../useNow";
import { accountOption } from "./accountOptions";

type Overrides = { model?: string | null; effort?: string | null };

/** Every agent's group of accounts as the pickers offer them, with the used-up quota, the running sharing change and the
 * disabled state of each account marked. */
export function useChoiceGroups(): ChoiceGroup[] {
  const t = useT();
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const addedAgents = useDaemonStore((s) => s.settings.added_agents);
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  const quotas = useDaemonStore((s) => s.accountQuotas);
  const sharing = useDaemonStore((s) => s.accountSharing);
  const disabledDefaults = useDaemonStore((s) => s.settings.disabled_default_accounts);
  const now = useNow();
  return choiceGroups(accounts, agentAvailability, addedAgents, (agent) => defaultAccountName(t, defaultNames, agent), { quotas, now }, sharing, disabledDefaults);
}

/**
 * What a dialog that starts a session of some agent asks for: the agent and its account, which
 * open on what a new session of the project (or, without one, the console) would use and which the
 * user can change, and the model and reasoning effort, which follow the project or console until
 * changed. `exclude` leaves one agent out of the choice altogether (a handoff never goes to the
 * session's own agent). `forced` is an agent and account the dialog settles itself, which then stand
 * in place of the pickers' (ignored while that pair may not be picked). `overrides` holds only what the
 * user changed, for the request.
 *
 * An account that is turned off is not listed; one whose quota is used up or whose sharing change runs is listed disabled
 * with why. The choice opens on another account of the agent when the one it would inherit is unavailable, and
 * `unavailable`, when even the account the choice ends on is (every account of the agent is, or it is the
 * agent's only one), says why it cannot be used, for the dialog to refuse its submit with.
 *
 * With automatic account selection on, the account a new session (a handoff's included) opens on is
 * the one whose weekly quota resets soonest rather than the one the project or console resolves to.
 * A forced choice that can still be picked wins over that. Switching to an agent whose remembered
 * account is no longer an entry falls back the same way.
 */
export function useLaunchChoice(parentConsole: Console, project: Project | undefined, exclude?: Agent, forced?: AccountChoice) {
  const t = useT();
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const addedAgents = useDaemonStore((s) => s.settings.added_agents);
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const autoSelect = useDaemonStore((s) => s.settings.auto_select_account) ?? false;
  const now = useNow();
  const [pickedAgent, setPickedAgent] = useState<Agent>();
  const [pickedAccounts, setPickedAccounts] = useState<Partial<Record<Agent, string | null>>>({});
  const [launchSettings, setLaunchSettings] = useState<Partial<Record<Agent, Overrides>>>({});

  const groups = useChoiceGroups().filter((group) => group.agent !== exclude);
  const openingAccount = (agent: Agent): string | null =>
    accountOpenedOn(parentConsole, project, agent, accounts, groups, autoSelect ? { now } : undefined);
  const pickedAccount = pickedAgent ? pickedAccounts[pickedAgent] : undefined;
  const pickedEntry = groups.find((group) => group.agent === pickedAgent)?.entries.find((entry) => entry.account === pickedAccount);
  const picked = pickedAgent
    ? pickedEntry
      ? { agent: pickedAgent, account: pickedEntry.account }
      : avoidUnavailableAccount({ agent: pickedAgent, account: openingAccount(pickedAgent) }, groups)
    : undefined;

  const selectable = pickableAgents(agentAvailability, addedAgents).filter((agent) => agent !== exclude);
  const wanted = project ? project.default_agent ?? projectSessionAgent(parentConsole) : parentConsole.default_agent;
  const initialAgent = exclude === undefined
    ? selectableAgent(agentAvailability, addedAgents, wanted)
    // With nothing to pick, the first other agent in listing order, so the picker shows a held,
    // marked value and never the excluded agent itself; submitting stays blocked.
    : selectable.includes(wanted) ? wanted : selectable[0] ?? agentsInListingOrder(addedAgents).find((other) => other !== exclude)!;
  const initial = avoidUnavailableAccount({ agent: initialAgent, account: openingAccount(initialAgent) }, groups);
  const forcedEntry = forced && findEntry(groups, choiceKey(forced));
  const choice = forced && forcedEntry?.selectable ? forced : currentChoice(picked, initial, groups);
  // A turned-off account is not offered; it stays listed, marked, only as the value the choice ends on (every account
  // of the agent is off, or it is the agent's only one), so the field is not blank while `unavailable` says why.
  const accountEntries = (groups.find((group) => group.agent === choice.agent)?.entries ?? []).filter((entry) => !entry.disabled || entry.key === choiceKey(choice));
  const overrides = launchSettings[choice.agent];
  const { model, effort } = launchWithOverrides(inheritedSessionLaunch(parentConsole, project, choice.agent), overrides);
  const blocked = selectable.length === 0;
  const exhaustedBy = choiceExhaustion(choice, groups);
  const chosenEntry = findEntry(groups, choiceKey(choice));
  const changingSharing = choiceChangingSharing(choice, groups);
  const disabled = choiceDisabled(choice, groups);

  const changeLaunchSetting = (field: "model" | "effort") => (value: string | null) =>
    setLaunchSettings((current) => ({ ...current, [choice.agent]: { ...current[choice.agent], [field]: value } }));

  return {
    choice,
    /** No agent that may be picked. */
    blocked,
    /** Why the chosen account cannot run a session, when it is disabled, its quota is used up or its sharing change runs. */
    unavailable: disabled && chosenEntry
      ? { title: t("accountChoice.disabled.title"), description: t("accountChoice.disabled.description", { name: isolate(chosenEntry.name) }) }
      : exhaustedBy && chosenEntry?.quota
        ? quotaUsedUpCopy(t, chosenEntry.quota, chosenEntry.name, exhaustedBy, now)
        : changingSharing && chosenEntry
          ? { title: t("accountChoice.sharingChange.title"), description: t("accountChoice.sharingChange.description", { name: isolate(chosenEntry.name) }) }
          : undefined,
    overrides,
    agentOptions: agentIconPickerOptions(t, agentAvailability, addedAgents, choice.agent).filter((option) => option.value !== exclude),
    setPickedAgent,
    /** The account half of the joined agent and account field, shown even for one account: every agent
     * has its default account. */
    account: {
      label: t("dialog.agentSettings.account", { agent: AGENT_LABEL[choice.agent] }),
      options: accountEntries.map((entry) => accountOption(t, entry, now, blocked)),
      value: choiceKey(choice),
      onChange: (key: string) => {
        const entry = findEntry(groups, key);
        if (entry) {
          setPickedAgent(entry.agent);
          setPickedAccounts((current) => ({ ...current, [entry.agent]: entry.account }));
        }
      },
    },
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
