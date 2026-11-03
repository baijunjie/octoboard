// Shared by the account-choice tests (opening choice, choice entries, switch entries, ranking, inheritance): the
// accounts, availability, quota readings and translator they build on.

import { choiceGroups, type QuotaReadings } from "./choiceEntries";
import { settledInitialChoice } from "./openingChoice";
import { accountOf } from "../fixtures/builders";
import { format } from "../i18n/catalog";
import type { Translate } from "../i18n/catalog";
import { type Account, type AccountQuota, type Agent, type AgentAvailability, type Console, type Project, type QuotaWindow } from "../protocol";
import { TEST_AGENTS, TEST_PROVIDERS } from "../test/testProviders";

export const availability = (entries: Partial<Record<Agent, AgentAvailability["availability"]>>): Map<Agent, AgentAvailability> =>
  new Map(
    TEST_AGENTS.map((agent) => [
      agent,
      { agent, availability: entries[agent] ?? "not_determined", default_account_dir: null },
    ]),
  );

export const account = (id: string, agent: Agent, name: string): Account => accountOf(id, agent, name, `/home/${id}`);

export const accounts = [account("w", "claude", "Work"), account("c", "codex", "Codex work")];

/** The state `settledInitialChoice` is given for `accounts`, `agents` and `added` (quotas judged by `readings`). */
export const settle = (owner: Console, project: Project | undefined, accounts: Account[], agents: ReturnType<typeof availability>, added: Agent[] | null, readings?: QuotaReadings) =>
  settledInitialChoice(owner, project, accounts, agents, added, TEST_PROVIDERS, choiceGroups(accounts, agents, added, TEST_PROVIDERS, () => "", readings));

/** The agent and account the dialog opens on. */
export const initialChoice = (...args: Parameters<typeof settle>) => settle(...args).choice;

export const NOW = 1_000_000;

export const HOUR = 3_600_000;

export const DAY = 24 * HOUR;

export const windowOf = (patch: Partial<QuotaWindow> = {}): QuotaWindow => ({
  kind: "weekly", label: null, used_percent: 0, resets_at: NOW + DAY, window_minutes: 7 * 24 * 60, ...patch,
});

export const reading = (accountId: string | null, windows: QuotaWindow[], agent: Agent = "claude"): AccountQuota => ({
  agent, account: accountId, fetched_at: NOW, plan: null, unavailable: null, windows, email: null,
});

export const usedUp = (agent: Agent, account: string | null): AccountQuota => ({
  agent, account, fetched_at: NOW, plan: null, unavailable: null,
  windows: [{ kind: "session", label: null, used_percent: 100, resets_at: NOW + 60_000, window_minutes: 300 }], email: null,
});

export const signedOut = (agent: Agent, account: string | null, code = "quota_not_signed_in"): AccountQuota => ({
  agent, account, fetched_at: NOW, plan: null, windows: [], unavailable: { code, params: { agent }, message: "x" }, email: null,
});

export const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;
