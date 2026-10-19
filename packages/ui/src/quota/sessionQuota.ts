import { AGENT_LABEL, defaultAccountName } from "../agents";
import type { Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import type { Account, AccountQuota, Agent, AgentAvailability, QuotaWindow, Session, Settings } from "../protocol";
import { accountQuotaOf } from "./accountQuotaOf";
import { primaryQuotaWindow, quotaHeadline } from "./quotaFormat";

export interface SessionQuotaSource {
  accounts: Account[];
  agentAvailability: Map<Agent, AgentAvailability>;
  accountQuotas: AccountQuota[];
}

/** Sessions retain their launch directory when an account is edited or removed. Its current
 * reading then belongs to another sign-in and cannot represent that session's quota. */
export function sessionQuotaWindowOf(session: Session | undefined, source: SessionQuotaSource): QuotaWindow | undefined {
  if (!session || source.agentAvailability.get(session.agent)?.availability !== "available") return undefined;
  if (session.account_id) {
    const account = source.accounts.find((account) => account.id === session.account_id);
    if (account?.agent !== session.agent || (session.config_dir && session.config_dir !== account.config_dir)) return undefined;
  } else if (session.config_dir) {
    return undefined;
  }
  const quota = accountQuotaOf(source.accountQuotas, session.agent, session.account_id ?? null);
  return quota && !quota.unavailable ? primaryQuotaWindow(quota.windows) : undefined;
}

/** The same account and window identification for quota indicators on the rail and native menus. With no
 * `accountName` it is the agent's default account, under the name the user gave it in `defaultNames`, if any. */
export function accountQuotaLabel(
  t: Translate,
  agent: Agent,
  accountName: string | undefined,
  window: QuotaWindow,
  defaultNames: Settings["default_account_names"],
): string {
  return t("rail.quota.selected", {
    agent: AGENT_LABEL[agent],
    account: isolate(accountName ?? defaultAccountName(t, defaultNames, agent)),
    quota: quotaHeadline(t, window),
  });
}
