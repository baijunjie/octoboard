import { orderedAccounts } from "./accountOrder";
import { isAgentPickable, servedAgents } from "../agents/agents";
import type { Translate } from "../i18n/catalog";
import type { Account, AccountQuota, AccountSharing, Agent, AgentAvailability, ProviderInfo, QuotaWindow, Settings } from "../protocol";
import { accountQuotaOf } from "./accountQuotaOf";
import { accountKey } from "./accountRecords";
import { exhaustedQuotaWindow, isNotLoggedIn } from "./quotaFormat";
import type { Reason } from "../format/reason";

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
  /** A change of the account's sharing of the default configuration is running; the daemon refuses a new
   * session on the account until it ends, so the entry may be shown, but not chosen. */
  changingSharing: boolean;
  /** The last reading found the account not logged in; the daemon refuses a session on it
   * (`account_not_signed_in`), so the entry may be shown, but not chosen. A reading that failed for another reason
   * does not count. */
  signedOut: boolean;
  /** The user turned the account off for new sessions; the daemon refuses one on it (`account_disabled`). Pickers
   * leave it out, unless it is the value they hold. */
  disabled: boolean;
}

/** One agent's entries in the saved account order. `selectable` is false only once the agent is
 * determined to be missing or is not added; before that nothing is known to be wrong with any of its entries. */
export interface ChoiceGroup {
  agent: Agent;
  selectable: boolean;
  entries: ChoiceEntry[];
}

/** Whether a change of sharing runs for `account` (`null`, the default account, has none). */
export function sharingChangeRuns(sharing: Map<string, AccountSharing> | undefined, account: string | null): boolean {
  return account !== null && sharing?.get(account)?.progress !== undefined;
}

/** The stored account `account` names when its last sharing change did not finish and none runs now, which a session
 * may be started on only after the user confirms it. A running change is not asked about: the daemon refuses the
 * launch. `null`, the default account, and an unknown id have none. */
export function incompleteAccount(accounts: Account[], sharing: Map<string, AccountSharing> | undefined, account: string | null | undefined): Account | undefined {
  if (account === null || account === undefined || sharingChangeRuns(sharing, account)) return undefined;
  return accounts.find((item) => item.id === account && item.share_default_config_incomplete);
}

/** Whether the reading of an account says the agent is not logged in there, which keeps a session off it. */
export function quotaSignedOut(quota: AccountQuota | undefined): boolean {
  return isNotLoggedIn(quota?.unavailable ?? null);
}

/** What an account that is not logged in shows in a list in place of its usage: why it cannot be chosen in short as
 * the `reason` under its name, and in full as the `detail` for assistive technology. It lasts until the user logs in,
 * so unlike a sharing change it is said. */
export function accountSignedOutUnavailable(t: Translate): { detail: string; reason: Reason } {
  return { detail: t("accountChoice.signedOut.detail"), reason: { state: t("accountChoice.signedOut.reason") } };
}

/** What an account the user turned off shows in a list in place of its usage: why it cannot be chosen in short as the
 * `reason` under its name, and in full as the `detail` for assistive technology. */
export function accountDisabledUnavailable(t: Translate): { detail: string; reason: Reason } {
  return { detail: t("accountChoice.disabled.detail"), reason: { state: t("accountChoice.disabled.reason") } };
}

/** What a turned-off account shows in a picker that holds it as the current value only, since the choice is a lasting
 * preference (a console's or a project's account) and the account is otherwise hidden: the same short reason as
 * `accountDisabledUnavailable`, with a description that says sessions cannot open on it. */
export function accountDisabledPickable(t: Translate): { detail: string; reason: Reason } {
  return { detail: t("accountChoice.disabled.pickableDetail"), reason: accountDisabledUnavailable(t).reason };
}

/** What names a choice among the entries of every agent's accounts (`accountKey`). */
export function choiceKey({ agent, account }: AccountChoice): string {
  return accountKey(agent, account);
}

/** What the accounts' quotas are judged by: the daemon's readings and the time they are judged at. Passed
 * as one value so that the readings are never judged against a time that was left out. */
export interface QuotaReadings {
  quotas: AccountQuota[];
  now: number;
}

/** Whether the user turned `account` of `agent` off for new sessions. `null`, the default account, is looked up in
 * `disabledDefaults`, the agents whose default account is. */
export function accountDisabled(accounts: Account[], disabledDefaults: readonly Agent[] | undefined, agent: Agent, account: string | null): boolean {
  return account === null ? disabledDefaults?.includes(agent) === true : accounts.some((item) => item.id === account && item.disabled);
}

/** Every agent's group, built from the stored accounts (`added` is the user's added agents, `null` while
 * the daemon has not decided them). With none stored an agent's group holds its
 * default account alone, which needs no discovery to exist. `defaultName` is the default account's
 * name in the current language. Without `readings` no entry is used up, without `sharing` no
 * entry has a sharing change running, and without `disabledDefaults` no default account is disabled. */
export function choiceGroups(
  accounts: Account[],
  agentAvailability: Map<Agent, AgentAvailability>,
  added: Agent[] | null,
  providers: readonly ProviderInfo[],
  defaultName: (agent: Agent) => string,
  readings?: QuotaReadings,
  sharing?: Map<string, AccountSharing>,
  disabledDefaults?: readonly Agent[],
  order?: Settings["account_order"],
): ChoiceGroup[] {
  return servedAgents(providers).map((agent) => ({
    agent,
    selectable: isAgentPickable(agentAvailability, added, agent),
    entries: orderedAccounts(agent, accounts, order).map((account) => ({ account: account?.id ?? null, name: account?.name ?? defaultName(agent) })).map((entry) => {
      const quota = readings && accountQuotaOf(readings.quotas, agent, entry.account);
      return { ...entry, agent, key: choiceKey({ agent, account: entry.account }), quota, exhaustedBy: readings && exhaustedQuotaWindow(quota, readings.now), changingSharing: sharingChangeRuns(sharing, entry.account), signedOut: quotaSignedOut(quota), disabled: accountDisabled(accounts, disabledDefaults, agent, entry.account) };
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

/** The window that keeps `choice`'s account from running a session right now, if it is used up. */
export function choiceExhaustion(choice: AccountChoice, groups: ChoiceGroup[]): QuotaWindow | undefined {
  return findEntry(groups, choiceKey(choice))?.exhaustedBy;
}

/** Whether the last reading of the account `choice` names found it not logged in, which keeps a session from opening on it. */
export function choiceSignedOut(choice: AccountChoice, groups: ChoiceGroup[]): boolean {
  return findEntry(groups, choiceKey(choice))?.signedOut === true;
}

/** Whether the account `choice` names is disabled, which keeps a session from opening on it. */
export function choiceDisabled(choice: AccountChoice, groups: ChoiceGroup[]): boolean {
  return findEntry(groups, choiceKey(choice))?.disabled === true;
}

/** Whether the account `choice` names has a sharing change running, which keeps a session from opening on it. */
export function choiceChangingSharing(choice: AccountChoice, groups: ChoiceGroup[]): boolean {
  return findEntry(groups, choiceKey(choice))?.changingSharing === true;
}

/** Whether nothing keeps a session off the entry's account: its quota is not used up, no sharing change runs, it is
 * logged in and it is not turned off. */
export function isUsable(entry: Pick<ChoiceEntry, "exhaustedBy" | "changingSharing" | "signedOut" | "disabled">): boolean {
  return !entry.exhaustedBy && !entry.changingSharing && !entry.signedOut && !entry.disabled;
}
