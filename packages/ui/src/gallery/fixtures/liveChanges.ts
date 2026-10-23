import type { Account, AccountQuota, Agent, AgentAvailability, ShellTerminal } from "../../protocol";
import { accountQuotaOf } from "../../quota/accountQuotaOf";
import type { LiveChange, Step } from "../scenario";
import { ALL_AGENTS } from "./builders";

// The changes a scenario's "Live changes" buttons make, written as the daemon's own events so the app takes them
// the way it takes another window's or the machine's. Each is meant to be paired with a change that undoes it (the
// data, not necessarily what was selected meanwhile), and does nothing when it is already in effect. Scenarios use
// these builders, not raw `daemonEvents` steps, so the events are written once.

/** The readings without the one for the account `account` of `agent` (`null` for its default account). */
const withoutQuota = (quotas: AccountQuota[], agent: Agent, account: string | null) => quotas.filter((held) => held !== accountQuotaOf(quotas, agent, account));

/** `account`, with its `quota` reading, appearing in the settings just before the account `before`. */
export function accountAdded(id: string, label: string, account: Account, quota: AccountQuota, before: string): LiveChange {
  const step: Step = (ui) =>
    ui.daemonEvents(
      // The reading lands first, so the new row is whole as it slides in.
      (state) => ({ type: "account_quotas_updated", account_quotas: [...withoutQuota(state.accountQuotas, quota.agent, quota.account), quota] }),
      (state) => {
        const without = state.settings.accounts.filter(({ id: held }) => held !== account.id);
        const index = without.findIndex(({ id: held }) => held === before);
        if (index < 0) throw new Error(`The settings hold no account ${before} to add ${account.id} before`);
        return { type: "settings_updated", settings: { ...state.settings, accounts: [...without.slice(0, index), account, ...without.slice(index)] } };
      },
    );
  return { id, label, steps: [step] };
}

/** The account `account` removed from the settings, its reading dropped with it. */
export function accountRemoved(id: string, label: string, account: Account): LiveChange {
  const step: Step = (ui) =>
    ui.daemonEvents(
      (state) => ({ type: "settings_updated", settings: { ...state.settings, accounts: state.settings.accounts.filter(({ id: held }) => held !== account.id) } }),
      (state) => ({ type: "account_quotas_updated", account_quotas: withoutQuota(state.accountQuotas, account.agent, account.id) }),
    );
  return { id, label, steps: [step] };
}

/** The agent `agent` found installed (`available`) or missing (`unavailable`), the other two as they were (undetermined
 * when the scenario held no entry for them). */
export function agentAvailability(id: string, label: string, agent: Agent, availability: AgentAvailability["availability"]): LiveChange {
  const step: Step = (ui) =>
    ui.daemonEvents((state) => ({
      type: "agent_availability_updated",
      agent_availability: ALL_AGENTS.map((each): AgentAvailability => {
        const held = state.agentAvailability.get(each) ?? { agent: each, availability: "not_determined" };
        return each === agent ? { ...held, availability } : held;
      }),
    }));
  return { id, label, steps: [step] };
}

/** A shell terminal started by another window: it joins the project's tabs without being selected. */
export function terminalOpened(id: string, label: string, terminal: ShellTerminal): LiveChange {
  return { id, label, steps: [(ui) => ui.daemonEvents({ type: "shell_terminal_upserted", shell_terminal: terminal })] };
}

/** The shell terminal `terminal` closed by another window. */
export function terminalClosed(id: string, label: string, terminal: string): LiveChange {
  return { id, label, steps: [(ui) => ui.daemonEvents({ type: "shell_terminal_deleted", shell_terminal: terminal })] };
}
