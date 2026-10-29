import { accountDisabled } from "../accountChoices";
import type { Agent } from "../protocol";
import { useDaemonStore } from "../store";

/** Whether the user turned `account` of `agent` off (`null`, the agent's default account), as the daemon has recorded
 * it and as it is now, so a change from another window shows. */
export function useAccountDisabled(agent: Agent, account: string | null): boolean {
  return useDaemonStore((s) => accountDisabled(s.settings.accounts, s.settings.disabled_default_accounts, agent, account));
}
