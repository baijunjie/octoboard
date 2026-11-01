import { accountDisabled } from "../accounts/accountChoices";
import { orderedAccounts } from "../accounts/accountOrder";
import type { Account, Agent, Settings } from "../protocol";

/** One row of an agent's quota list: an account of the agent (`null`, the default account) and whether the user turned it off. */
export interface QuotaRow {
  account: Account | null;
  disabled: boolean;
}

/** The accounts the quota lists (the rail's panel, the menu bar's submenus) show for each of `agents`, in that order:
 * the agent's accounts in the saved account order, not one the user turned off, unless it is `inUse`, the account the
 * selected running session is still on. An agent left with no row is dropped. Which agents are listed is the caller's
 * rule, as the two lists differ in it. */
export function listedQuotaAccounts(
  agents: Agent[],
  source: { accounts: Account[]; accountOrder?: Settings["account_order"]; disabledDefaultAccounts?: readonly Agent[] },
  inUse?: { agent: Agent; account: string | null },
): { agent: Agent; rows: QuotaRow[] }[] {
  return agents
    .map((agent) => ({
      agent,
      rows: orderedAccounts(agent, source.accounts, source.accountOrder)
        .map((account) => ({ account, disabled: accountDisabled(source.accounts, source.disabledDefaultAccounts, agent, account?.id ?? null) }))
        .filter(({ account, disabled }) => !disabled || (inUse?.agent === agent && inUse.account === (account?.id ?? null))),
    }))
    .filter(({ rows }) => rows.length > 0);
}
