import type { Account, Agent, Settings } from "./protocol";

/** The agent's accounts in the saved display order, including its default account (`null`). Unknown and repeated
 * references are ignored; accounts added since the order was saved follow in their stored order. */
export function orderedAccounts(agent: Agent, accounts: Account[], order: Settings["account_order"]): (Account | null)[] {
  const owned = accounts.filter((account) => account.agent === agent);
  const byId = new Map<string | null, Account | null>([[null, null], ...owned.map((account): [string, Account] => [account.id, account])]);
  const result: (Account | null)[] = [];
  for (const id of [...(order?.[agent] ?? []), null, ...owned.map((account) => account.id)]) {
    if (!byId.has(id)) continue;
    result.push(byId.get(id)!);
    byId.delete(id);
  }
  return result;
}
