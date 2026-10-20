import { accountsByAgent, AGENT_ACCOUNT_FIELD, defaultAccountName, isAgentPickable, projectSessionAgent, selectableAgent } from "./agents";
import type { Translate } from "./i18n/catalog";
import type { Account, AccountQuota, Agent, AgentAvailability, Console, Project, ProjectAccount, QuotaWindow, Session, Settings } from "./protocol";
import { accountQuotaOf } from "./quota/accountQuotaOf";
import { exhaustedQuotaWindow } from "./quota/quotaFormat";
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
  agent: Agent;
  account: string | null;
  name: string;
  /** What the daemon last read of the account's quota, if it has. */
  quota: AccountQuota | undefined;
  /** The window that keeps the account from running a session right now (see `exhaustedQuotaWindow`);
   * the entry may still be shown, but not chosen. */
  exhaustedBy: QuotaWindow | undefined;
}

/** One agent's entries, the default account first. `selectable` is false only once the agent is
 * determined to be missing or is not added; before that nothing is known to be wrong with any of its entries. */
export interface ChoiceGroup {
  agent: Agent;
  selectable: boolean;
  entries: ChoiceEntry[];
}

export function choiceKey({ agent, account }: AccountChoice): string {
  return `${agent}:${account ?? ""}`;
}

/** What the accounts' quotas are judged by: the daemon's readings and the time they are judged at. Passed
 * as one value so that the readings are never judged against a time that was left out. */
export interface QuotaReadings {
  quotas: AccountQuota[];
  now: number;
}

/** Every agent's group, built from the stored accounts (`added` is the user's added agents, `null` while
 * the daemon has not decided them). With none stored an agent's group holds its
 * default account alone, which needs no discovery to exist. `defaultName` is the default account's
 * name in the current language. Without `readings` no entry is used up. */
export function choiceGroups(
  accounts: Account[],
  agentAvailability: Map<Agent, AgentAvailability>,
  added: Agent[] | null,
  defaultName: (agent: Agent) => string,
  readings?: QuotaReadings,
): ChoiceGroup[] {
  return accountsByAgent(accounts).map(({ agent, accounts: owned }) => ({
    agent,
    selectable: isAgentPickable(agentAvailability, added, agent),
    entries: [
      { account: null, name: defaultName(agent) },
      ...owned.map((account) => ({ account: account.id, name: account.name })),
    ].map((entry) => {
      const quota = readings && accountQuotaOf(readings.quotas, agent, entry.account);
      return { ...entry, agent, key: choiceKey({ agent, account: entry.account }), quota, exhaustedBy: readings && exhaustedQuotaWindow(quota, readings.now) };
    }),
  }));
}

/** The entry `key` names, with its agent, or `undefined` when there is none. */
export function findEntry(groups: ChoiceGroup[], key: string): (ChoiceEntry & { selectable: boolean }) | undefined {
  for (const group of groups) {
    const entry = group.entries.find((candidate) => candidate.key === key);
    if (entry) return { ...entry, selectable: group.selectable };
  }
  return undefined;
}

/** The account `id` names for `agent` — `null`, the default account, when it names none or one
 * that is no longer stored. */
export function storedAccount(id: string | null | undefined, agent: Agent, accounts: Account[]): string | null {
  return id && accounts.some((account) => account.id === id && account.agent === agent) ? id : null;
}

/** The account the console refers to for `agent`, `null` (the default account) when none. */
export function consoleAccount(owner: Console, agent: Agent, accounts: Account[]): string | null {
  return storedAccount(owner[AGENT_ACCOUNT_FIELD[agent]], agent, accounts);
}

/** `reference`, a project's setting for `agent`'s account, as the daemon reads it: one to an account
 * that is no longer stored counts as inheriting. */
export function settledProjectAccount(reference: ProjectAccount, agent: Agent, accounts: Account[]): ProjectAccount {
  return reference.kind === "account" && !storedAccount(reference.id, agent, accounts) ? { kind: "inherit" } : reference;
}

/** What the project says about `agent`'s account. */
export function projectAccount(project: Project, agent: Agent, accounts: Account[]): ProjectAccount {
  return settledProjectAccount(project[AGENT_ACCOUNT_FIELD[agent]], agent, accounts);
}

/** The account a project session of `agent` runs under when none is chosen for it, as the daemon
 * resolves it: the project's `own` setting, which may be the default account and then stops the
 * chain, else the console's (`consoleAccountId`, `null` for none), else `null`, the default
 * account. */
export function resolveProjectAccount(own: ProjectAccount, consoleAccountId: string | null): string | null {
  if (own.kind === "default") return null;
  return own.kind === "account" ? own.id : consoleAccountId;
}

/** The account a session of `agent` runs under when none is chosen, using the project settings
 * when opening a project session and the console settings otherwise. */
export function inheritedAccount(owner: Console, project: Project | undefined, agent: Agent, accounts: Account[]): string | null {
  const account = consoleAccount(owner, agent, accounts);
  return project ? resolveProjectAccount(projectAccount(project, agent, accounts), account) : account;
}

/** The window that keeps `choice`'s account from running a session right now, if it is used up. */
export function choiceExhaustion(choice: AccountChoice, groups: ChoiceGroup[]): QuotaWindow | undefined {
  return findEntry(groups, choiceKey(choice))?.exhaustedBy;
}

/** `choice`, or when its account is used up and its agent may be picked, the first account of the same
 * agent that is not, in the order of the agent's group (the default account first). With every account
 * of the agent used up `choice` stays, for the caller to refuse. */
export function avoidExhaustedAccount(choice: AccountChoice, groups: ChoiceGroup[]): AccountChoice {
  const group = groups.find((candidate) => candidate.agent === choice.agent);
  if (!group?.selectable || !choiceExhaustion(choice, groups)) return choice;
  const usable = group.entries.find((entry) => !entry.exhaustedBy);
  return usable ? { agent: choice.agent, account: usable.account } : choice;
}

/** Where the session dialog's control opens: the agent the dialog already resolves — a project
 * session's: the project's default, else the console's project session default; a console
 * session's: the console's default — moved to one that may be picked when that one is known
 * to be missing or is not added, with the account that agent resolves to when none is chosen: the project's
 * setting for it, else the console `owner`'s, moved to another account of the agent when that one's quota is
 * used up (`avoidExhaustedAccount`). The choice made in the dialog sits above this, as the daemon's own chain
 * puts a session's choice above both. `groups` are the agents' entries with their quotas judged (`choiceGroups`).
 * `exhaustedBy` is the window that still keeps the account from running a session when every account of the
 * agent is used up. */
export function settledInitialChoice(
  owner: Console,
  project: Project | undefined,
  accounts: Account[],
  agentAvailability: Map<Agent, AgentAvailability>,
  added: Agent[] | null,
  groups: ChoiceGroup[],
): { choice: AccountChoice; exhaustedBy: QuotaWindow | undefined } {
  const agent = selectableAgent(agentAvailability, added, project ? project.default_agent ?? projectSessionAgent(owner) : owner.default_agent);
  const choice = avoidExhaustedAccount({ agent, account: inheritedAccount(owner, project, agent, accounts) }, groups);
  return { choice, exhaustedBy: choiceExhaustion(choice, groups) };
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
  /** The window that keeps the account from taking the session over (see `exhaustedQuotaWindow`); the
   * entry is listed, but not offered. */
  exhaustedBy: QuotaWindow | undefined;
  /** What the daemon last read of the account's quota, if it has; none for a removed account's directory. */
  quota: AccountQuota | undefined;
}

/** The accounts of the session's own agent, the default account first, with the one the session is
 * on marked current. Another agent's accounts never appear: a switch stays within one agent. A
 * session on an account that has since been removed keeps it as its current entry, named as
 * everywhere else a session's account is (`sessionAccountName`: the directory the session
 * recorded), so the menu still says where the session is; with no directory recorded there is
 * nothing to name it by and it is left out. The submenu is offered only when there is somewhere to
 * go, that is with more than one entry. Without `readings` no entry is used up. */
export function switchEntries(
  session: Session,
  accounts: Account[],
  t: Translate,
  defaultNames: Settings["default_account_names"],
  readings?: QuotaReadings,
): SwitchEntry[] {
  const owned = accountsByAgent(accounts).find((group) => group.agent === session.agent)?.accounts ?? [];
  const entries = [
    { account: null, name: defaultAccountName(t, defaultNames, session.agent), isPath: false },
    ...owned.map((account) => ({ account: account.id, name: account.name, isPath: false })),
  ];
  const removed = session.account_id && !owned.some((account) => account.id === session.account_id);
  const removedName = removed ? sessionAccountName(t, session, accounts, defaultNames) : undefined;
  if (session.account_id && removedName !== undefined) {
    entries.push({ account: session.account_id, name: removedName, isPath: true });
  }
  return entries.map((entry) => {
    const quota = entry.isPath || !readings ? undefined : accountQuotaOf(readings.quotas, session.agent, entry.account);
    return {
      ...entry,
      current: entry.account === (session.account_id ?? null),
      exhaustedBy: readings ? exhaustedQuotaWindow(quota, readings.now) : undefined,
      quota,
    };
  });
}
