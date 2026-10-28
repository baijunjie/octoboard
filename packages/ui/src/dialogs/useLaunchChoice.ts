import { useState } from "react";

import { accountOpenedOn, avoidUnavailableAccount, choiceChangingSharing, choiceDisabled, choiceExhaustion, choiceGroups, choiceSignedOut, choiceKey, currentChoice, findEntry, openingChoice, type AccountChoice, type ChoiceGroup } from "../accountChoices";
import { AGENT_LABEL, agentsInListingOrder, defaultAccountName, inheritedSessionLaunch, launchWithOverrides, pickableAgents, projectSessionAgent } from "../agents";
import { agentIconPickerOptions } from "../components/AgentIcon";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import type { Agent, Console, Project } from "../protocol";
import { quotaUsedUpMessage } from "../quota/quotaCopy";
import { useDaemonStore } from "../store";
import { useNow } from "../useNow";
import { accountOption } from "./accountOptions";
import { accountRefusalMessage } from "./accountRefusalMessage";

type Overrides = { model?: string | null; effort?: string | null };

/** Every agent's group of accounts as the pickers offer them, with the used-up quota, the running sharing change, the
 * not-logged-in state and the disabled state of each account marked. */
export function useChoiceGroups(): ChoiceGroup[] {
  const t = useT();
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const addedAgents = useDaemonStore((s) => s.settings.added_agents);
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const defaultNames = useDaemonStore((s) => s.settings.default_account_names);
  const quotas = useDaemonStore((s) => s.accountQuotas);
  const sharing = useDaemonStore((s) => s.accountSharing);
  const disabledDefaults = useDaemonStore((s) => s.settings.disabled_default_accounts);
  const accountOrder = useDaemonStore((s) => s.settings.account_order);
  const now = useNow();
  return choiceGroups(accounts, agentAvailability, addedAgents, (agent) => defaultAccountName(t, defaultNames, agent), { quotas, now }, sharing, disabledDefaults, accountOrder);
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
 * Agents whose accounts are all turned off and accounts that are turned off are not listed. An account whose
 * quota is used up, whose sharing change runs or that is not logged in
 * is listed disabled with why. The choice opens on another account of the agent when the one it would inherit is unavailable, and
 * `unavailable`, when even the account the choice ends on is (every account of the agent is, or it is the
 * agent's only one), says why it cannot be used, for the dialog to refuse its submit with.
 *
 * With automatic account selection on, the account a new session (a handoff's included) opens on is
 * the one whose weekly quota resets soonest rather than the one the project or console resolves to.
 * With no usable account of that agent, the initial choice tries other agents in added order.
 * A forced choice that can still be picked wins over that. Switching to an agent whose remembered
 * account is no longer an entry falls back the same way.
 *
 * `preset` is where the pickers start: an agent and account, and the model and effort, each absent
 * to follow the parent. `overrides` then holds what the user changed, starting from the preset's
 * values. `presetGone` is whether the preset's agent can no longer be picked, or its account is no
 * longer among that agent's accounts, and nothing has been picked since: the dialog then shows the
 * pickers empty rather than a fallback the user never chose.
 */
export function useLaunchChoice(parentConsole: Console, project: Project | undefined, exclude?: Agent, forced?: AccountChoice, preset?: { choice: AccountChoice } & Overrides) {
  const t = useT();
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const addedAgents = useDaemonStore((s) => s.settings.added_agents);
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const autoSelect = useDaemonStore((s) => s.settings.auto_select_account) ?? false;
  const now = useNow();
  const [pickedAgent, setPickedAgentValue] = useState<Agent | undefined>(preset?.choice.agent);
  const [touched, setTouched] = useState(false);
  const [pickedAccounts, setPickedAccounts] = useState<Partial<Record<Agent, string | null>>>(preset ? { [preset.choice.agent]: preset.choice.account } : {});
  const [launchSettings, setLaunchSettings] = useState<Partial<Record<Agent, Overrides>>>(
    preset && (preset.model !== undefined || preset.effort !== undefined)
      ? { [preset.choice.agent]: { ...(preset.model !== undefined && { model: preset.model }), ...(preset.effort !== undefined && { effort: preset.effort }) } }
      : {},
  );

  const allGroups = useChoiceGroups().filter((group) => group.agent !== exclude);
  const groups = allGroups.map((group) => ({
    ...group,
    selectable: group.selectable && group.entries.some((entry) => !entry.disabled),
  }));
  const presetGroup = preset && groups.find((group) => group.agent === preset.choice.agent);
  const presetGone = preset !== undefined && !touched
    && (!presetGroup?.selectable || !presetGroup.entries.some((entry) => entry.account === preset.choice.account));
  const setPickedAgent = (agent: Agent) => {
    setTouched(true);
    setPickedAgentValue(agent);
  };
  const openingAccount = (agent: Agent): string | null =>
    accountOpenedOn(parentConsole, project, agent, accounts, groups, autoSelect ? { now } : undefined);
  const pickedAccount = pickedAgent ? pickedAccounts[pickedAgent] : undefined;
  const pickedEntry = groups.find((group) => group.agent === pickedAgent)?.entries.find((entry) => entry.account === pickedAccount);
  const picked = pickedAgent
    ? pickedEntry
      ? { agent: pickedAgent, account: pickedEntry.account }
      : avoidUnavailableAccount({ agent: pickedAgent, account: openingAccount(pickedAgent) }, groups)
    : undefined;

  const selectable = pickableAgents(agentAvailability, addedAgents).filter((agent) => groups.some((group) => group.agent === agent && group.selectable));
  const wanted = project ? project.default_agent ?? projectSessionAgent(parentConsole) : parentConsole.default_agent;
  // Keep a value for the refusal when no agent can be chosen, without returning the excluded agent.
  const initialAgent = selectable.includes(wanted) ? wanted : selectable[0]
    ?? agentsInListingOrder(addedAgents).find((agent) => agent !== exclude)!;
  const initial = openingChoice(parentConsole, project, initialAgent, accounts, groups, addedAgents, autoSelect ? { now } : undefined);
  const forcedEntry = forced && findEntry(allGroups, choiceKey(forced));
  const choice = forced && forcedEntry?.selectable ? forced : currentChoice(picked, initial, groups);
  // A fixed conversation can still name a turned-off account; keep its value so the refusal names it.
  const accountEntries = (groups.find((group) => group.agent === choice.agent)?.entries ?? []).filter((entry) => !entry.disabled || entry.key === choiceKey(choice));
  const overrides = launchSettings[choice.agent];
  const { model, effort } = launchWithOverrides(inheritedSessionLaunch(parentConsole, project, choice.agent), overrides);
  const blocked = selectable.length === 0;
  const exhaustedBy = choiceExhaustion(choice, groups);
  const chosenEntry = findEntry(groups, choiceKey(choice));
  const changingSharing = choiceChangingSharing(choice, groups);
  const signedOut = choiceSignedOut(choice, groups);
  const disabled = choiceDisabled(choice, groups);

  const changeLaunchSetting = (field: "model" | "effort") => (value: string | null) =>
    setLaunchSettings((current) => ({ ...current, [choice.agent]: { ...current[choice.agent], [field]: value } }));

  return {
    choice,
    /** No agent that may be picked. */
    blocked,
    /** The preset's agent or account can no longer be picked, and the user has not picked since. */
    presetGone,
    /** Why the chosen account cannot run a session, when it is disabled, not logged in, its quota is used up or its
     * sharing change runs: the error text of the account field. */
    unavailable: disabled && chosenEntry
      ? accountRefusalMessage("accountChoice.disabled.description", chosenEntry.name)
      : signedOut && chosenEntry
        ? accountRefusalMessage("accountChoice.signedOut.description", chosenEntry.name)
        : exhaustedBy && chosenEntry?.quota
          ? quotaUsedUpMessage(t, chosenEntry.quota, chosenEntry.name, exhaustedBy, now)
          : changingSharing && chosenEntry
            ? t("accountChoice.sharingChange.description", { name: isolate(chosenEntry.name) })
            : undefined,
    overrides,
    agentOptions: agentIconPickerOptions(t, agentAvailability, addedAgents, choice.agent).filter((option) => option.value !== exclude && groups.some((group) => group.agent === option.value && group.entries.some((entry) => !entry.disabled))),
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
