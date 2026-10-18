import type { AccountQuota, Agent } from "../protocol";
import { accountQuotaOf, useDaemonStore } from "../store";

/** What is known of an account's quota: its entry, `"loading"` for an account whose reading has not
 * landed yet, or `undefined` for one that has nothing to wait for. */
export type QuotaReading = AccountQuota | "loading" | undefined;

/** The reading of one account (`null` for an agent's default account). The daemon adds an entry
 * only once a probe has finished, so an account with none is waiting on its first reading when its
 * agent is available, and has nothing to wait for when the agent is missing or not yet determined. */
export function useAccountQuota(agent: Agent, account: string | null): QuotaReading {
  const quota = useDaemonStore((state) => accountQuotaOf(state.accountQuotas, agent, account));
  const available = useDaemonStore((state) => state.agentAvailability.get(agent)?.availability === "available");
  if (quota) return quota;
  return available ? "loading" : undefined;
}
