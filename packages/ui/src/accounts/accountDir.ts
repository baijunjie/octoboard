import { defaultAccountConfigDir } from "../agents/providers";
import type { Account, Agent, AgentAvailability, ProviderInfo } from "../protocol";

/** The configuration directory of an account (`null` for an agent's default account), as the window names it: a stored
 * account's own, the default account's as the daemon found it (`default_account_dir`), else the provider's usual
 * one under the home directory. Empty for an account that is not stored and has no directory, which no surface asks for. */
export function accountConfigDir(
  accounts: readonly Account[],
  agentAvailability: ReadonlyMap<Agent, AgentAvailability>,
  providers: readonly ProviderInfo[],
  agent: Agent,
  account: string | null,
): string {
  if (account !== null) return accounts.find((stored) => stored.id === account)?.config_dir ?? "";
  return defaultAccountConfigDir(agentAvailability, providers, agent);
}
