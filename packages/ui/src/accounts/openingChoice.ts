import { agentsInListingOrder, projectSessionAgent, selectableAgent } from "../agents/agents";
import type { Account, AgentAvailability, Agent, Console, Project, ProviderInfo, QuotaWindow } from "../protocol";
import { inheritedAccount } from "./accountInheritance";
import { preferExpiringWeekly, selectionEntries, soonestAvailable } from "./accountRanking";
import {
  choiceChangingSharing,
  choiceDisabled,
  choiceExhaustion,
  choiceKey,
  choiceSignedOut,
  findEntry,
  isUsable,
  type AccountChoice,
  type ChoiceGroup,
} from "./choiceEntries";

/** The account a new session of `agent` opens on. With `autoSelect`, the one whose quota is to be spent first
 * (`preferExpiringWeekly`) when one ranks; otherwise the account the project or console resolves to. Equal
 * readings keep the daemon's selection order, the group's. */
export function accountOpenedOn(
  owner: Console,
  project: Project | undefined,
  agent: Agent,
  accounts: Account[],
  groups: ChoiceGroup[],
  autoSelect?: { now: number },
): string | null {
  if (autoSelect) {
    const preferred = preferExpiringWeekly(selectionEntries(groups, agent), autoSelect.now);
    if (preferred) return preferred.account;
  }
  return inheritedAccount(owner, project, agent, accounts);
}

/** `choice`, or when its account is disabled, used up, not logged in or its sharing change runs, and its agent may be picked, the first
 * account of the same agent that is none of these, in the saved order of the agent's group. With
 * every account of the agent unavailable `choice` stays, for the caller to refuse. */
export function avoidUnavailableAccount(choice: AccountChoice, groups: ChoiceGroup[]): AccountChoice {
  const group = groups.find((candidate) => candidate.agent === choice.agent);
  const held = findEntry(groups, choiceKey(choice));
  if (!group?.selectable || !held || isUsable(held)) return choice;
  const usable = group.entries.find(isUsable);
  return usable ? { agent: choice.agent, account: usable.account } : choice;
}

/** The account of `agent` a new session opens on (`accountOpenedOn`), moved to another account of the agent
 * when that one cannot be used (`avoidUnavailableAccount`). With `autoSelect` and no usable account of the
 * agent, the one that becomes usable soonest (`soonestAvailable`); otherwise, or when none is kept off by a
 * used-up quota alone, the inherited one stays. */
export function agentOpeningChoice(
  owner: Console,
  project: Project | undefined,
  agent: Agent,
  accounts: Account[],
  groups: ChoiceGroup[],
  autoSelect?: { now: number },
): AccountChoice {
  const choice = avoidUnavailableAccount({ agent, account: accountOpenedOn(owner, project, agent, accounts, groups, autoSelect) }, groups);
  const entry = findEntry(groups, choiceKey(choice));
  if (!autoSelect || !entry?.selectable || isUsable(entry)) return choice;
  const soonest = soonestAvailable(selectionEntries(groups, agent));
  return soonest ? { agent, account: soonest.account } : choice;
}

/** The initial choice of a new session. Automatic selection tries other added agents in listing order only
 * when the preferred agent has no usable account; when no agent has one, it is the preferred agent's
 * `agentOpeningChoice`, which falls back to the account that becomes usable soonest. Explicit picks do not
 * pass through the agent fallback. */
export function openingChoice(
  owner: Console,
  project: Project | undefined,
  agent: Agent,
  accounts: Account[],
  groups: ChoiceGroup[],
  added: Agent[] | null,
  autoSelect?: { now: number },
): AccountChoice {
  const initial = agentOpeningChoice(owner, project, agent, accounts, groups, autoSelect);
  if (!autoSelect) return initial;
  for (const candidate of [agent, ...agentsInListingOrder(added, groups.map((group) => group.agent)).filter((other) => other !== agent)]) {
    const choice = candidate === agent ? initial : avoidUnavailableAccount({ agent: candidate, account: accountOpenedOn(owner, project, candidate, accounts, groups, autoSelect) }, groups);
    const entry = findEntry(groups, choiceKey(choice));
    if (entry?.selectable && isUsable(entry)) return choice;
  }
  return initial;
}

/** Where the session dialog's control opens: the agent the dialog already resolves — a project
 * session's: the project's default, else the console's project session default; a console
 * session's: the console's default — moved to one that may be picked when that one is known
 * to be missing or is not added, with the account that agent resolves to when none is chosen: the project's
 * setting for it, else the console `owner`'s, moved to another account of the agent when that one's quota is
 * used up, its sharing change runs, it is not logged in or it is turned off (`avoidUnavailableAccount`). With `autoSelect`, the account of that agent whose quota is to be spent first
 * (`preferExpiringWeekly`) replaces the inherited one before that move, when the group has one;
 * when none of its accounts can be used, another selectable agent is tried in added order, and when no agent has a usable account
 * the preferred agent's account that becomes usable soonest is chosen (`soonestAvailable`). With it
 * omitted, the inherited account is used as before. The choice made in the dialog sits above this, as the
 * daemon's own chain puts a session's choice above both. `groups` are the agents' entries with their quotas
 * judged (`choiceGroups`). `exhaustedBy` is the window that still keeps the account from running a session
 * when every account of the agent is used up, and `changingSharing`, `signedOut` and `disabled` the same for a running
 * sharing change, an account found not logged in and a turned-off account. */
export function settledInitialChoice(
  owner: Console,
  project: Project | undefined,
  accounts: Account[],
  agentAvailability: Map<Agent, AgentAvailability>,
  added: Agent[] | null,
  providers: readonly ProviderInfo[],
  groups: ChoiceGroup[],
  autoSelect?: { now: number },
): { choice: AccountChoice; exhaustedBy: QuotaWindow | undefined; changingSharing: boolean; signedOut: boolean; disabled: boolean } {
  const agent = selectableAgent(agentAvailability, added, providers, project ? project.default_agent ?? projectSessionAgent(owner) : owner.default_agent);
  const choice = openingChoice(owner, project, agent, accounts, groups, added, autoSelect);
  return { choice, exhaustedBy: choiceExhaustion(choice, groups), changingSharing: choiceChangingSharing(choice, groups), signedOut: choiceSignedOut(choice, groups), disabled: choiceDisabled(choice, groups) };
}

/** What the control currently shows: the user's pick while it is still an entry that may be
 * chosen — availability or the account list can move under an open dialog — else `initial`. */
export function currentChoice(
  picked: AccountChoice | undefined,
  initial: AccountChoice,
  groups: ChoiceGroup[],
): AccountChoice {
  const entry = picked && findEntry(groups, choiceKey(picked));
  return picked && entry?.selectable ? picked : initial;
}
