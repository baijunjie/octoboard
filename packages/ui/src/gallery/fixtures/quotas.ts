import type { AccountQuota, Agent, QuotaUnavailable, QuotaWindow, QuotaWindowKind } from "../../protocol";
import { minutesAgo, minutesFromNow } from "./builders";

/** The length of each kind of window as the agents give it, in minutes. */
const WINDOW_MINUTES: Record<QuotaWindowKind, number> = {
  session: 300,
  weekly: 7 * 24 * 60,
  weekly_model: 7 * 24 * 60,
  credit_period: 7 * 24 * 60,
};

/** A quota window `used` percent used that resets `resetsIn` minutes after the page loaded (never
 * when `null`). A whole number of minutes keeps the countdown on one reading for the first minute. */
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
