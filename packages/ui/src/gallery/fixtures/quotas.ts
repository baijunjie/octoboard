import type { Account, AccountQuota, Agent, QuotaUnavailable, QuotaWindow, QuotaWindowKind } from "../../protocol";
import { minutesAgo, minutesFromNow } from "./builders";

/** The length of each kind of window as the agents give it, in minutes. */
const WINDOW_MINUTES: Record<QuotaWindowKind, number> = {
  session: 300,
  weekly: 7 * 24 * 60,
  weekly_model: 7 * 24 * 60,
  credit_period: 7 * 24 * 60,
};

/** A quota window `used` percent used that resets `resetsIn` minutes after the page loaded (never
 * when `null`; a negative number is a reset already past, which the reading has not caught up with).
 * A whole number of minutes keeps the countdown on one reading for the first minute. */
export function quotaWindow(kind: QuotaWindowKind, used: number, resetsIn: number | null, label: string | null = null): QuotaWindow {
  return {
    kind,
    label,
    used_percent: used,
    resets_at: resetsIn === null ? null : minutesFromNow(resetsIn),
    window_minutes: WINDOW_MINUTES[kind],
  };
}

/** An account's reading, taken a minute before the page loaded unless `patch` says otherwise;
 * `account` is `null` for the agent's default account. */
export function quotaOf(agent: Agent, account: string | null, plan: string | null, windows: QuotaWindow[], patch: Partial<AccountQuota> = {}): AccountQuota {
  return { agent, account, fetched_at: minutesAgo(1), plan, windows, unavailable: null, ...patch };
}

/** A coded reason a reading failed; `message` is the daemon's own wording, which the app words
 * from its catalog instead. */
export function quotaFailure(code: string, params: Record<string, string>, message: string): QuotaUnavailable {
  return { code, params, message };
}

const MANY_CLAUDE_ACCOUNT_NAMES = ["Research", "Billing", "Support", "Docs", "Staging", "Mobile", "Data", "Infra"] as const;

/** Claude Code's owned accounts beyond a list's height: eight of them, each with a plan, two windows and a reading of
 * a different age, so every row is as tall as a real one. */
export function manyClaudeAccounts(): { accounts: Account[]; quotas: AccountQuota[] } {
  return {
    accounts: MANY_CLAUDE_ACCOUNT_NAMES.map((name) => ({ id: `a-${name.toLowerCase()}`, agent: "claude", name, config_dir: `/Users/dev/.claude-${name.toLowerCase()}` })),
    quotas: MANY_CLAUDE_ACCOUNT_NAMES.map((name, index) =>
      quotaOf("claude", `a-${name.toLowerCase()}`, index % 3 === 0 ? "max" : "pro", [
        quotaWindow("session", (index * 23 + 11) % 100, 30 + index * 31),
        quotaWindow("weekly", (index * 37 + 8) % 100, (index + 1) * 24 * 60 + 90),
      ], { fetched_at: minutesAgo(1 + index * 2) })),
  };
}
