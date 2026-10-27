import { accountDisabled } from "../accountChoices";
import { AGENT_LABEL, defaultAccountName } from "../agents";
import type { Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { isLive, type Account, type AccountQuota, type Agent, type AgentAvailability, type Session, type Settings } from "../protocol";
import { accountQuotaOf } from "./accountQuotaOf";
import { usageBarSummary, usageBarWindows, type QuotaBar } from "./quotaFormat";

export interface SessionQuotaSource {
  accounts: Account[];
  /** The agents whose default account is turned off. */
  disabledDefaultAccounts?: Settings["disabled_default_accounts"];
  agentAvailability: Map<Agent, AgentAvailability>;
  accountQuotas: AccountQuota[];
}

/** Sessions retain their launch directory when an account is edited or removed. Its current
 * reading then belongs to another sign-in and cannot represent that session's quota. The result is
 * what a compact bar draws: the 5-hour window with the weekly one beneath it, or the account's one
 * primary window (`usageBarWindows`). An account the user turned off has a bar only while the session is running
 * on it, as the quota panel lists it only then. */
export function sessionQuotaBarOf(session: Session | undefined, source: SessionQuotaSource): QuotaBar | undefined {
  if (!session || source.agentAvailability.get(session.agent)?.availability !== "available") return undefined;
  if (session.account_id) {
    const account = source.accounts.find((account) => account.id === session.account_id);
    if (account?.agent !== session.agent || (session.config_dir && session.config_dir !== account.config_dir)) return undefined;
  } else if (session.config_dir) {
    return undefined;
  }
  if (!isLive(session.status) && accountDisabled(source.accounts, source.disabledDefaultAccounts, session.agent, session.account_id ?? null)) return undefined;
  const quota = accountQuotaOf(source.accountQuotas, session.agent, session.account_id ?? null);
  return quota && !quota.unavailable ? usageBarWindows(quota) : undefined;
}

/** The account a turned-off account's exception is for: the one `session` runs on while it is running, which the rail's
 * panel and the menu bar's usage submenus list although it is turned off; `undefined` when the session is not running. */
export function accountInUse(session: Session | undefined): { agent: Agent; account: string | null } | undefined {
  return session && isLive(session.status) ? { agent: session.agent, account: session.account_id ?? null } : undefined;
}

/** The same account and window identification for quota indicators on the rail and native menus, naming
 * every window the indicator draws. With no `accountName` it is the agent's default account, under the
 * name the user gave it in `defaultNames`, if any. */
export function accountQuotaLabel(
  t: Translate,
  agent: Agent,
  accountName: string | undefined,
  bar: QuotaBar,
  defaultNames: Settings["default_account_names"],
  now: number,
): string {
  return t("rail.quota.selected", {
    agent: AGENT_LABEL[agent],
    account: isolate(accountName ?? defaultAccountName(t, defaultNames, agent)),
    quota: usageBarSummary(t, bar, now),
  });
}
