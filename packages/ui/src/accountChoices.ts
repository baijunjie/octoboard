import { accountsByAgent, AGENT_ACCOUNT_FIELD, isAgentUnavailable, selectableAgent } from "./agents";
import type { Translate } from "./i18n/catalog";
import type { Account, Agent, AgentAvailability, Console, Project, Session } from "./protocol";
import { sessionAccountName } from "./sessionLabel";

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

/** One account a session can be moved to or is on, as the Switch account submenu lists it. */
export interface SwitchEntry {
  account: string | null;
  name: string;
  current: boolean;
  /** `name` is the directory of an account that has been removed, not a name the user typed. */
  isPath: boolean;
}

/** The accounts of the session's own agent, the default account first, with the one the session is
 * on marked current. Another agent's accounts never appear: a switch stays within one agent. A
 * session on an account that has since been removed keeps it as its current entry, named as
 * everywhere else a session's account is (`sessionAccountName`: the directory the session
 * recorded), so the menu still says where the session is; with no directory recorded there is
 * nothing to name it by and it is left out. The submenu is offered only when there is somewhere to
 * go, that is with more than one entry. */
export function switchEntries(session: Session, accounts: Account[], t: Translate): SwitchEntry[] {
  const owned = accountsByAgent(accounts).find((group) => group.agent === session.agent)?.accounts ?? [];
  const entries = [
    { account: null, name: t("settings.accounts.defaultName"), isPath: false },
    ...owned.map((account) => ({ account: account.id, name: account.name, isPath: false })),
  ];
  const removed = session.account_id && !owned.some((account) => account.id === session.account_id);
  const removedName = removed ? sessionAccountName(t, session, accounts) : undefined;
  if (session.account_id && removedName !== undefined) {
    entries.push({ account: session.account_id, name: removedName, isPath: true });
  }
  return entries.map((entry) => ({ ...entry, current: entry.account === (session.account_id ?? null) }));
}
