import { accountsByAgent, AGENT_ACCOUNT_FIELD, defaultAccountName, isAgentPickable, projectSessionAgent, selectableAgent } from "./agents";
import type { Translate } from "./i18n/catalog";
import type { Account, AccountQuota, AccountSharing, Agent, AgentAvailability, Console, Project, ProjectAccount, QuotaWindow, Session, Settings } from "./protocol";
import { accountQuotaOf } from "./quota/accountQuotaOf";
import { exhaustedQuotaWindow } from "./quota/quotaFormat";
import { sessionAccountName } from "./sessionLabel";
import type { Reason } from "./components/ReasonText";

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
  /** The user turned the account off for new sessions; the daemon refuses one on it (`account_disabled`). Pickers
   * leave it out, unless it is the value they hold. */
  disabled: boolean;
}

/** One agent's entries, the default account first. `selectable` is false only once the agent is
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
  return accounts.find((item) => item.id === account && item.share_default_config_incomplete === true);
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

export function choiceKey({ agent, account }: AccountChoice): string {
  return `${agent}:${account ?? ""}`;
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
  return account === null ? disabledDefaults?.includes(agent) === true : accounts.some((item) => item.id === account && item.disabled === true);
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
  defaultName: (agent: Agent) => string,
  readings?: QuotaReadings,
  sharing?: Map<string, AccountSharing>,
  disabledDefaults?: readonly Agent[],
): ChoiceGroup[] {
  return accountsByAgent(accounts).map(({ agent, accounts: owned }) => ({
    agent,
    selectable: isAgentPickable(agentAvailability, added, agent),
    entries: [
      { account: null, name: defaultName(agent) },
      ...owned.map((account) => ({ account: account.id, name: account.name })),
    ].map((entry) => {
      const quota = readings && accountQuotaOf(readings.quotas, agent, entry.account);
      return { ...entry, agent, key: choiceKey({ agent, account: entry.account }), quota, exhaustedBy: readings && exhaustedQuotaWindow(quota, readings.now), changingSharing: sharingChangeRuns(sharing, entry.account), disabled: accountDisabled(accounts, disabledDefaults, agent, entry.account) };
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

/** Whether the account `choice` names is disabled, which keeps a session from opening on it. */
export function choiceDisabled(choice: AccountChoice, groups: ChoiceGroup[]): boolean {
  return findEntry(groups, choiceKey(choice))?.disabled === true;
}

/** Whether the account `choice` names has a sharing change running, which keeps a session from opening on it. */
export function choiceChangingSharing(choice: AccountChoice, groups: ChoiceGroup[]): boolean {
  return findEntry(groups, choiceKey(choice))?.changingSharing === true;
}

const DAY_MINUTES = 24 * 60;
/** A credit period this long is the account's weekly quota. A monthly period (28 to 31 days) is not. */
const WEEKLY_CREDIT_MIN_MINUTES = 6 * DAY_MINUTES;
const WEEKLY_CREDIT_MAX_MINUTES = 8 * DAY_MINUTES;

/** Whether `window` is the account-wide weekly quota. A model-specific weekly window and a session
 * window are not: the former is one model's limit, the latter resets within hours. */
function isWeeklyQuota(window: QuotaWindow): boolean {
  if (window.kind === "weekly") return true;
  return window.kind === "credit_period"
    && window.window_minutes !== null
    && window.window_minutes >= WEEKLY_CREDIT_MIN_MINUTES
    && window.window_minutes <= WEEKLY_CREDIT_MAX_MINUTES;
}

/** The weekly window of `quota` that resets soonest and still has quota left. Of several, the soonest
 * reset wins, then the larger remainder. Nothing rankable returns undefined. */
export function expiringWeekly(quota: AccountQuota | undefined, now: number): { resetsAt: number; remaining: number } | undefined {
  if (!quota) return undefined;
  let best: { resetsAt: number; remaining: number } | undefined;
  for (const window of quota.windows) {
    if (!isWeeklyQuota(window) || !Number.isFinite(window.used_percent) || window.used_percent >= 100) continue;
    if (typeof window.resets_at !== "number" || !(window.resets_at > now)) continue;
    const candidate = { resetsAt: window.resets_at, remaining: Math.max(0, 100 - window.used_percent) };
    if (!best || candidate.resetsAt < best.resetsAt || (candidate.resetsAt === best.resetsAt && candidate.remaining > best.remaining)) {
      best = candidate;
    }
  }
  return best;
}

/** The entry whose weekly quota resets soonest, among entries that are not used up.
 * Soonest reset wins over a larger remainder. Equal reset and remainder keep `entries` order
 * (the default account is first). No rankable window returns undefined. */
export function preferExpiringWeekly(entries: ChoiceEntry[], now: number): ChoiceEntry | undefined {
  let best: ChoiceEntry | undefined;
  let bestSignal: { resetsAt: number; remaining: number } | undefined;
  for (const entry of entries) {
    if (entry.exhaustedBy || entry.changingSharing || entry.disabled) continue;
    const signal = expiringWeekly(entry.quota, now);
    if (!signal) continue;
    if (!bestSignal || signal.resetsAt < bestSignal.resetsAt || (signal.resetsAt === bestSignal.resetsAt && signal.remaining > bestSignal.remaining)) {
      best = entry;
      bestSignal = signal;
    }
  }
  return best;
}

/** The account a new session of `agent` opens on. With `autoSelect`, the one whose weekly quota resets
 * soonest when one ranks; otherwise the account the project or console resolves to. */
export function accountOpenedOn(
  owner: Console,
  project: Project | undefined,
  agent: Agent,
  accounts: Account[],
  groups: ChoiceGroup[],
  autoSelect?: { now: number },
): string | null {
  if (autoSelect) {
    const preferred = preferExpiringWeekly(groups.find((candidate) => candidate.agent === agent)?.entries ?? [], autoSelect.now);
    if (preferred) return preferred.account;
  }
  return inheritedAccount(owner, project, agent, accounts);
}

/** `choice`, or when its account is disabled, used up or its sharing change runs, and its agent may be picked, the first
 * account of the same agent that is none of these, in the order of the agent's group (the default account first). With
 * every account of the agent unavailable `choice` stays, for the caller to refuse. */
export function avoidUnavailableAccount(choice: AccountChoice, groups: ChoiceGroup[]): AccountChoice {
  const group = groups.find((candidate) => candidate.agent === choice.agent);
  if (!group?.selectable || (!choiceExhaustion(choice, groups) && !choiceChangingSharing(choice, groups) && !choiceDisabled(choice, groups))) return choice;
  const usable = group.entries.find((entry) => !entry.exhaustedBy && !entry.changingSharing && !entry.disabled);
  return usable ? { agent: choice.agent, account: usable.account } : choice;
}

/** Where the session dialog's control opens: the agent the dialog already resolves — a project
 * session's: the project's default, else the console's project session default; a console
 * session's: the console's default — moved to one that may be picked when that one is known
 * to be missing or is not added, with the account that agent resolves to when none is chosen: the project's
 * setting for it, else the console `owner`'s, moved to another account of the agent when that one's quota is
 * used up or its sharing change runs (`avoidUnavailableAccount`). With `autoSelect`, an account of that agent whose weekly quota resets
 * soonest (`preferExpiringWeekly`) replaces the inherited one before that move, when the group has one;
 * omitted, the inherited account is used as before. The choice made in the dialog sits above this, as the
 * daemon's own chain puts a session's choice above both. `groups` are the agents' entries with their quotas
 * judged (`choiceGroups`). `exhaustedBy` is the window that still keeps the account from running a session
 * when every account of the agent is used up, and `changingSharing` and `disabled` the same for a running
 * sharing change and for a turned-off account. */
export function settledInitialChoice(
  owner: Console,
  project: Project | undefined,
  accounts: Account[],
  agentAvailability: Map<Agent, AgentAvailability>,
  added: Agent[] | null,
  groups: ChoiceGroup[],
  autoSelect?: { now: number },
): { choice: AccountChoice; exhaustedBy: QuotaWindow | undefined; changingSharing: boolean; disabled: boolean } {
  const agent = selectableAgent(agentAvailability, added, project ? project.default_agent ?? projectSessionAgent(owner) : owner.default_agent);
  const choice = avoidUnavailableAccount({ agent, account: accountOpenedOn(owner, project, agent, accounts, groups, autoSelect) }, groups);
  return { choice, exhaustedBy: choiceExhaustion(choice, groups), changingSharing: choiceChangingSharing(choice, groups), disabled: choiceDisabled(choice, groups) };
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
  /** A change of the account's sharing runs; the daemon refuses a switch onto it, so it is listed, but not offered. */
  changingSharing: boolean;
  /** The user turned the account off; the menu leaves it out unless it is the session's own. */
  disabled: boolean;
}

/** Why a Switch account entry cannot be chosen apart from its quota being used up, or `undefined`
 * when nothing else keeps it: an account switch holds the session (`switching`), the entry's own
 * sharing change runs, or the account the session is on is changing its sharing (`ownChanging`,
 * which keeps every other account from taking it). Each of these ends on its own, so the `detail` is
 * for a message where a form needs one; a menu or list disables the entry without a reason. The
 * current entry is never blocked, as it is never offered. The Switch account submenu and the form of
 * a switch request share these rules. */
export function switchEntryBlock(
  t: Translate,
  entry: Pick<SwitchEntry, "current" | "changingSharing">,
  session: { switching: boolean; ownChanging: boolean },
): { detail: string } | undefined {
  if (entry.current) return undefined;
  if (session.switching) return { detail: t("accountChoice.switching.detail") };
  if (entry.changingSharing) return { detail: t("accountChoice.sharingChange.detail") };
  if (session.ownChanging) return { detail: t("accountChoice.ownSharingChange.detail") };
  return undefined;
}

/** Why a session cannot be handed off, or `undefined` when nothing keeps it: it is locked, it is in the middle of a
 * turn, an account switch holds it, its own account is changing its sharing (`ownChanging`), or it is still starting,
 * which the daemon refuses. The lasting holds (locked, a turn that can run for long) are checked first and come with a
 * `reason` to draw under a menu item; the others end on their own, come with none, and lose to a lasting hold. The Hand
 * off menu item and the form of a handoff request share these rules. */
export function handoffBlock(
  t: Translate,
  session: Pick<Session, "status" | "locked" | "switching_account">,
  ownChanging: boolean,
): { detail: string; reason?: Reason } | undefined {
  if (session.locked) return { detail: t("accountChoice.handoff.locked"), reason: { state: t("accountChoice.sessionLocked.reason") } };
  if (session.status === "working") return { detail: t("accountChoice.handoff.working"), reason: { state: t("accountChoice.handoff.working.reason") } };
  if (session.switching_account) return { detail: t("accountChoice.switching.detail") };
  if (ownChanging) return { detail: t("accountChoice.ownSharingChange.detail") };
  if (session.status === "starting") return { detail: t("accountChoice.handoff.starting") };
  return undefined;
}

/** The accounts of the session's own agent, the default account first, with the one the session is
 * on marked current. Another agent's accounts never appear: a switch stays within one agent. A
 * session on an account that has since been removed keeps it as its current entry, named as
 * everywhere else a session's account is (`sessionAccountName`: the directory the session
 * recorded), so the menu still says where the session is; with no directory recorded there is
 * nothing to name it by and it is left out. The submenu is offered only when there is somewhere to
 * go, that is with more than one entry, turned-off ones counted: the menu lists only the others that are on, but with
 * every other account off it stays, so the user can go to Settings to turn one back on. Without `readings` no entry is
 * used up, without `sharing` none has a sharing change running, and without `disabledDefaults` no default account is
 * disabled. */
export function switchEntries(
  session: Session,
  accounts: Account[],
  t: Translate,
  defaultNames: Settings["default_account_names"],
  readings?: QuotaReadings,
  sharing?: Map<string, AccountSharing>,
  disabledDefaults?: readonly Agent[],
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
      changingSharing: !entry.isPath && sharingChangeRuns(sharing, entry.account),
      disabled: !entry.isPath && accountDisabled(accounts, disabledDefaults, session.agent, entry.account),
    };
  });
}
