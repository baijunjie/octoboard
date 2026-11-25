import type { AccountQuota, Agent } from "../protocol";
import { hasPlanQuota } from "../agents/providers";
import { useDaemonStore, useProviders } from "../store";
import { isLive } from "../sessions/sessionStatus";
import { isNotLoggedIn } from "../accounts/quotaFormat";
import { accountDisabled } from "../accounts/choiceEntries";
import { accountQuotaOf } from "../accounts/accountQuotaOf";

/** What is known of an account's quota: its entry, `"loading"` for an account whose reading has not
 * landed yet, or `undefined` for one that has nothing to wait for. */
export type QuotaReading = AccountQuota | "loading" | undefined;

/** The reading of one account (`null` for an agent's default account). The daemon adds an entry
 * only once a probe has finished, so an account with none is waiting on its first reading when its
 * agent is available, and has nothing to wait for when the agent is missing or not yet determined,
 * or when the user turned the account off and no session with a process runs on it: the daemon reads
 * no quota for such an account.
 *
 * Of a provider that declares no plan quota (`hasPlanQuota`) it is only what the account's sign-in reading says, which
 * is never waited for: the entry when the account reads as signed out or signed in with an email, and `undefined`
 * otherwise: an entry that says neither (the reading could not tell), or that failed for another reason (the shell
 * environment could not be read), knows nothing of the sign-in, and its failure is never worded as usage. */
export function useAccountQuota(agent: Agent, account: string | null): QuotaReading {
  const quota = useDaemonStore((state) => accountQuotaOf(state.accountQuotas, agent, account));
  const providers = useProviders();
  const available = useDaemonStore((state) => state.agentAvailability.get(agent)?.availability === "available");
  const disabled = useDaemonStore((state) => accountDisabled(state.settings.accounts, state.settings.disabled_default_accounts, agent, account));
  const inUse = useDaemonStore((state) => disabled && Array.from(state.sessions.values()).some((session) => session.agent === agent && (session.account_id ?? null) === account && isLive(session.status)));
  if (!hasPlanQuota(providers, agent)) return quota && (isNotLoggedIn(quota.unavailable) || (quota.unavailable === null && quota.email)) ? quota : undefined;
  if (quota) return quota;
  return available && (!disabled || inUse) ? "loading" : undefined;
}
