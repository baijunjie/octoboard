import { accountsByAgent, AGENT_ACCOUNT_FIELD, isAgentUnavailable, selectableAgent } from "./agents";
import type { Account, Agent, AgentAvailability, Console, Project } from "./protocol";

/** An account of one agent as a picker settles it; `null` is the agent's default account, the
 * state of pinning nothing. */
export interface AccountChoice {
  agent: Agent;
  account: string | null;
}

export interface ChoiceEntry {
  /** Unique across agents, so one control can hold every agent's entries. */
  key: string;
  account: string | null;
  name: string;
}

/** One agent's entries, the default account first. `selectable` is false only once the agent is
 * determined to be missing; before that nothing is known to be wrong with any of its entries. */
export interface ChoiceGroup {
  agent: Agent;
  selectable: boolean;
  entries: ChoiceEntry[];
}

export function choiceKey({ agent, account }: AccountChoice): string {
  return `${agent}:${account ?? ""}`;
}

/** Every agent's group, built from the stored accounts. With none stored an agent's group holds its
 * default account alone, which needs no discovery to exist. `defaultName` is the default account's
 * name in the current language. */
export function choiceGroups(
  accounts: Account[],
  agentAvailability: Map<Agent, AgentAvailability>,
  defaultName: string,
): ChoiceGroup[] {
  return accountsByAgent(accounts).map(({ agent, accounts: owned }) => ({
    agent,
    selectable: !isAgentUnavailable(agentAvailability, agent),
    entries: [
      { account: null, name: defaultName },
      ...owned.map((account) => ({ account: account.id, name: account.name })),
    ].map((entry) => ({ ...entry, key: choiceKey({ agent, account: entry.account }) })),
  }));
}

/** The entry `key` names, with its agent, or `undefined` when there is none. */
export function findEntry(groups: ChoiceGroup[], key: string): (ChoiceEntry & { agent: Agent; selectable: boolean }) | undefined {
  for (const group of groups) {
    const entry = group.entries.find((candidate) => candidate.key === key);
    if (entry) return { ...entry, agent: group.agent, selectable: group.selectable };
  }
  return undefined;
}

/** The account `owner` refers to for `agent` — `null`, the default account, when it refers to
 * none or to one that is no longer stored. */
export function consoleAccount(owner: Console, agent: Agent, accounts: Account[]): string | null {
  const id = owner[AGENT_ACCOUNT_FIELD[agent]];
  return id && accounts.some((account) => account.id === id && account.agent === agent) ? id : null;
}

/** Where the session dialog's control opens: the agent the dialog already resolves — the
 * project's default, else the console's — moved to one that may be picked when that one is known
 * to be missing, with the account that agent resolves to for the console `owner`. The choice made in the
 * dialog sits above this, as the daemon's own chain puts a session's choice above the console's. */
export function initialChoice(
  owner: Console,
  project: Project,
  accounts: Account[],
  agentAvailability: Map<Agent, AgentAvailability>,
): AccountChoice {
  const agent = selectableAgent(agentAvailability, project.default_agent ?? owner.default_agent);
  return { agent, account: consoleAccount(owner, agent, accounts) };
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
