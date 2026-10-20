import type { Reason } from "../components/ReasonText";
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
export function countdown(t: Translate, resetsAt: number, now: number): string {
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

/** The window that keeps `quota` from being used right now, mirroring the daemon's refusal of a session on an
 * account whose quota is used up: a session, weekly or credit window used to 100% or more whose reset time is
 * not in the past (or is not given). A model-specific weekly limit never counts. A reading that has not
 * landed, or has no windows, blocks nothing, and a failed re-read still applies the windows it kept. Of
 * several used-up windows, the one that resets last is returned, since that is when the account is usable
 * again; one with no reset time comes after every other. */
export function exhaustedQuotaWindow(quota: AccountQuota | "loading" | undefined, now: number): QuotaWindow | undefined {
  if (typeof quota !== "object") return undefined;
  let blocking: QuotaWindow | undefined;
  for (const window of quota.windows) {
    if (window.kind === "weekly_model" || window.used_percent < 100) continue;
    if (window.resets_at != null && window.resets_at <= now) continue;
    if (!blocking || (blocking.resets_at != null && (window.resets_at == null || window.resets_at > blocking.resets_at))) blocking = window;
  }
  return blocking;
}

/** The window as a noun phrase inside a sentence, in lower case: "5-hour quota", "weekly quota", "credits". Unlike
 * `quotaWindowLabel` it follows "its", so a credit pool is "credits" rather than "Credits quota". */
export function quotaWindowPhrase(t: Translate, window: QuotaWindow): string {
  switch (window.kind) {
    case "session":
      return t("quota.windowPhrase.session");
    case "weekly":
      return t("quota.windowPhrase.weekly");
    case "weekly_model":
      return window.label ? t("quota.windowPhrase.weeklyModel", { model: isolate(window.label) }) : t("quota.windowPhrase.weekly");
    case "credit_period": {
      const days = (window.window_minutes ?? 0) / DAY_MINUTES;
      if (days >= 6 && days <= 8) return t("quota.windowPhrase.weekly");
      if (days >= 28 && days <= 31) return t("quota.windowPhrase.monthly");
      return t("quota.windowPhrase.credits");
    }
  }
}

/** What a picker shows beside an account that cannot be chosen: "Used up" over "resets in 2h", without the
 * countdown when the agent gives no reset time. */
export function quotaUsedUpShort(t: Translate, window: QuotaWindow, now: number): Reason {
  const detail = quotaResetsIn(t, window, now);
  return detail ? { state: t("quota.usedUp"), detail } : { state: t("quota.usedUp") };
}

/** The same in full, for assistive technology: "Used up: 5-hour quota · resets in 2h". */
export function quotaUsedUpDetail(t: Translate, window: QuotaWindow, now: number): string {
  const resets = quotaResetsIn(t, window, now);
  const quota = quotaWindowPhrase(t, window);
  return resets ? t("quota.usedUpDetailResets", { quota, resets }) : t("quota.usedUpDetail", { quota });
}

/** Why `account` cannot be used, as a sentence: "Work cannot be used for now because it has used up its 5-hour
 * quota. It resets in 2h." */
export function quotaUsedUpSentence(t: Translate, account: string, window: QuotaWindow, now: number): string {
  const resets = quotaResetsIn(t, window, now);
  const params = { account: isolate(account), quota: quotaWindowPhrase(t, window) };
  return resets ? t("quota.accountUsedUp.resets", { ...params, resets }) : t("quota.accountUsedUp", params);
}

/** The two layers of a combined meter: the 5-hour window and the account-wide weekly one beneath it. The weekly
 * layer is absent when the agent reports none. `undefined` for an account with no 5-hour window (a single
 * credit pool, say), which keeps one bar. */
export function twoLayerWindows(windows: readonly QuotaWindow[]): { session: QuotaWindow; weekly: QuotaWindow | undefined } | undefined {
  const session = windows.find((window) => window.kind === "session");
  return session && { session, weekly: windows.find((window) => window.kind === "weekly") };
}

/** The windows a compact usage bar draws: the main one, and the thin one beneath it if any. */
export interface QuotaBar {
  main: QuotaWindow;
  thin?: QuotaWindow;
}

/** The windows a compact usage bar draws: the 5-hour window with the weekly one beneath it, else the
 * account's one primary window (a credit pool, or a weekly window with no 5-hour one). `undefined`
 * for a reading with nothing to draw. */
export function usageBarWindows(windows: readonly QuotaWindow[]): QuotaBar | undefined {
  const layers = twoLayerWindows(windows);
  if (layers) return { main: layers.session, thin: layers.weekly };
  const main = primaryQuotaWindow([...windows]);
  return main && { main };
}

/** What a compact usage bar shows, in words: "5-hour 82% remaining, Weekly 40% remaining". */
export function usageBarSummary(t: Translate, bar: QuotaBar): string {
  const main = quotaHeadline(t, bar.main);
  return bar.thin ? t("quota.usageSummary", { main, thin: quotaHeadline(t, bar.thin) }) : main;
}
