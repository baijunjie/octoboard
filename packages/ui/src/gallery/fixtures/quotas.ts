import type { Account, AccountQuota, Agent, QuotaUnavailable, QuotaWindow, QuotaWindowKind } from "../../protocol";
import { accountOf, minutesAgo, minutesFromNow } from "./builders";

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
  return { agent, account, fetched_at: minutesAgo(1), plan, email: null, windows, unavailable: null, ...patch };
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
    accounts: MANY_CLAUDE_ACCOUNT_NAMES.map((name) => accountOf(`a-${name.toLowerCase()}`, "claude", name, `/Users/dev/.claude-${name.toLowerCase()}`)),
    quotas: MANY_CLAUDE_ACCOUNT_NAMES.map((name, index) =>
      quotaOf("claude", `a-${name.toLowerCase()}`, index % 3 === 0 ? "max" : "pro", [
        quotaWindow("session", (index * 23 + 11) % 100, 30 + index * 31),
        quotaWindow("weekly", (index * 37 + 8) % 100, (index + 1) * 24 * 60 + 90),
      ], { fetched_at: minutesAgo(1 + index * 2) })),
  };
}

/** Claude Code's accounts in every state a person meets: the default account usable (72% of its session and 75% of
 * its week left), Work on the Team plan used up by its session window with its latest read failed (the windows it
 * kept still apply), Second used up by its weekly window while its session window is untouched, Third and Fourth
 * used up by their session windows with 15% and 86% of the week left. The default account has no entry in
 * `accounts`; its reading is the first of `quotas`. */
export const USED_UP_MIX: { accounts: Account[]; quotas: AccountQuota[] } = {
  accounts: [
    accountOf("a-work", "claude", "Work", "/Users/dev/.claude-work"),
    accountOf("a-second", "claude", "Second", "/Users/dev/.claude-second"),
    accountOf("a-third", "claude", "Third", "/Users/dev/.claude-third"),
    accountOf("a-fourth", "claude", "Fourth", "/Users/dev/.claude-fourth"),
  ],
  quotas: [
    quotaOf("claude", null, "max", [quotaWindow("session", 28, 117), quotaWindow("weekly", 25, 3 * 24 * 60 + 4 * 60)]),
    quotaOf("claude", "a-work", "team", [quotaWindow("session", 100, 42), quotaWindow("weekly", 63, 2 * 24 * 60 + 5 * 60)], {
      fetched_at: minutesAgo(38),
      unavailable: quotaFailure("quota_timed_out", { agent: "Claude Code" }, "Claude Code took too long to report its usage."),
    }),
    quotaOf("claude", "a-second", "pro", [quotaWindow("session", 0, 280), quotaWindow("weekly", 100, 4 * 24 * 60 + 2 * 60)], { fetched_at: minutesAgo(4) }),
    quotaOf("claude", "a-third", "pro", [quotaWindow("session", 100, 95), quotaWindow("weekly", 85, 5 * 24 * 60 + 7 * 60)], { fetched_at: minutesAgo(2) }),
    quotaOf("claude", "a-fourth", "pro", [quotaWindow("session", 100, 160), quotaWindow("weekly", 14, 6 * 24 * 60 + 3 * 60)], { fetched_at: minutesAgo(6) }),
  ],
};
