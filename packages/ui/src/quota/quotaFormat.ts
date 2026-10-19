import { isolate } from "../i18n/isolate";
import type { Translate } from "../i18n/catalog";
import type { AccountQuota, QuotaUnavailable, QuotaWindow } from "../protocol";

const MINUTE = 60_000;
const DAY_MINUTES = 24 * 60;

const WINDOW_ORDER: Record<QuotaWindow["kind"], number> = {
  session: 0,
  weekly: 1,
  credit_period: 2,
  weekly_model: 3,
};

/** Account-wide limits precede model-specific limits regardless of the agent's response order.
 * Copy before the stable sort so store readings and the order among model limits stay intact. */
export function orderedQuotaWindows(windows: readonly QuotaWindow[]): QuotaWindow[] {
  return [...windows].sort((a, b) => WINDOW_ORDER[a.kind] - WINDOW_ORDER[b.kind]);
}

/** The quota codes that say the account is not logged in yet, rather than that reading it failed. */
const ACCOUNT_STATE_CODES = new Set(["quota_not_signed_in", "quota_config_dir_missing"]);

/** Whether the reading shows the account is logged in: a current reading with quota windows. A reading that
 * failed for another reason, a re-read that failed after a good one, and one not yet taken say nothing either way. */
export function isLoggedIn(quota: AccountQuota | "loading" | undefined): boolean {
  return typeof quota === "object" && quota.unavailable === null && quota.windows.length > 0;
}

/** Whether the reading is unavailable because the account is not logged in yet: a state, not a failure. */
export function isNotLoggedIn(unavailable: QuotaUnavailable | null): unavailable is QuotaUnavailable {
  return unavailable !== null && ACCOUNT_STATE_CODES.has(unavailable.code);
}

/** How much of `window` is left, as a whole percent from 0 to 100. The daemon passes a used
 * percentage above 100 on as the agent gives it, and one below 0 is as unlikely, so both are
 * clamped rather than shown as a negative or an excess. */
export function remainingPercent(window: QuotaWindow): number {
  return Math.min(100, Math.max(0, Math.round(100 - window.used_percent)));
}

/** The window's name: "5-hour", "Weekly", "<Model> weekly". A credit pool is named for the length
 * of its current period, which the agent bills over (weekly on the measured account). */
export function quotaWindowLabel(t: Translate, window: QuotaWindow): string {
  switch (window.kind) {
    case "session":
      return t("quota.window.session");
    case "weekly":
      return t("quota.window.weekly");
    case "weekly_model":
      return window.label ? t("quota.window.weeklyModel", { model: isolate(window.label) }) : t("quota.window.weekly");
    case "credit_period": {
      const days = (window.window_minutes ?? 0) / DAY_MINUTES;
      if (days >= 6 && days <= 8) return t("quota.window.weekly");
      if (days >= 28 && days <= 31) return t("quota.window.monthly");
      return t("quota.window.credits");
    }
  }
}

/** The window's headline: "5-hour 82% remaining". */
export function quotaHeadline(t: Translate, window: QuotaWindow): string {
  return t("quota.remaining", { window: quotaWindowLabel(t, window), percent: remainingPercent(window) });
}

/** The time left until `resetsAt` as "2d 3h", "1h 57m" or "12m", rounded up to the minute so a
 * countdown never reads zero before the reset. */
function countdown(t: Translate, resetsAt: number, now: number): string {
  const minutes = Math.max(1, Math.ceil((resetsAt - now) / MINUTE));
  if (minutes >= DAY_MINUTES) {
    return t("quota.duration.days", { days: Math.floor(minutes / DAY_MINUTES), hours: Math.floor((minutes % DAY_MINUTES) / 60) });
  }
  if (minutes >= 60) return t("quota.duration.hours", { hours: Math.floor(minutes / 60), minutes: minutes % 60 });
  return t("quota.duration.minutes", { minutes });
}

/** "resets in 1h 57m" for a window with a reset time; "resets now" once that time has passed
 * (the next reading has not landed yet); null when the agent gives none. `now` comes from the
 * caller, as `formatRelativeTime`'s does, so that a component passes the value it renders on. */
export function quotaResetsIn(t: Translate, window: QuotaWindow, now: number): string | null {
  if (window.resets_at === null) return null;
  if (window.resets_at <= now) return t("quota.resetsNow");
  return t("quota.resetsIn", { time: countdown(t, window.resets_at, now) });
}

/** The plan as the agent names it (`max`, `plus`, `SuperGrok`) in display case: a name that
 * already has a capital after its first letter keeps its own casing, any other has each word
 * capitalised. */
export function planLabel(plan: string): string {
  if (/.[A-Z]/.test(plan)) return plan;
  return plan
    .split(/[\s_-]+/)
    .filter(Boolean)
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");
}

/** The account-wide short window first, then the weekly limit or the agent's credit pool.
 * Model-specific weekly limits do not represent the whole account. */
export function primaryQuotaWindow(windows: QuotaWindow[]): QuotaWindow | undefined {
  return windows.find((window) => window.kind === "session")
    ?? windows.find((window) => window.kind === "weekly")
    ?? windows.find((window) => window.kind === "credit_period");
}
