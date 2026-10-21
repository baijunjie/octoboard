import type { AccountQuota, Agent } from "../protocol";

/** The quota entry of an account (`null` for an agent's default account), if the daemon has
 * reported on it. A broadcast reading names no directory, so this matches by agent and account
 * only; do not key the match by directory unless a reading starts carrying one. Pointing an
 * account at another directory discards the old reading, which is what keeps this on the reading
 * held for the directory the account points at now (see "Which accounts have a quota" in
 * docs/product/account-quotas.md). */
export function accountQuotaOf(quotas: AccountQuota[], agent: Agent, account: string | null): AccountQuota | undefined {
  return quotas.find((quota) => quota.agent === agent && quota.account === account);
}
