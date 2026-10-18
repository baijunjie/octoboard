import type { AccountQuota, Agent } from "../protocol";

/** The quota entry of an account (`null` for an agent's default account), if the daemon has
 * reported on it. */
export function accountQuotaOf(quotas: AccountQuota[], agent: Agent, account: string | null): AccountQuota | undefined {
  return quotas.find((quota) => quota.agent === agent && quota.account === account);
}
