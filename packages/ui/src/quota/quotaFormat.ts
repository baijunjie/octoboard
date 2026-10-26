import type { Reason } from "../components/ReasonText";
import { isolate } from "../i18n/isolate";
import type { Translate } from "../i18n/catalog";
import type { AccountQuota, Agent, QuotaUnavailable, QuotaWindow } from "../protocol";

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

/** Whether the reading is unavailable because the account is not logged in yet: a state, not a failure. */
export function isNotLoggedIn(unavailable: QuotaUnavailable | null): unavailable is QuotaUnavailable {
  return unavailable !== null && ACCOUNT_STATE_CODES.has(unavailable.code);
}

/** Whether the account is logged in as far as its reading says: `"out"` when the reading says it is not,
 * `"in"` when a reading was taken (the last good windows of a failed one count), and `undefined` while that is not
 * known: a reading still to come, or one that failed for another reason with nothing read before. */
export function loginStateOf(quota: AccountQuota | "loading" | undefined): "in" | "out" | undefined {
  if (typeof quota !== "object") return undefined;
  if (isNotLoggedIn(quota.unavailable)) return "out";
  return quota.unavailable === null || quota.windows.length > 0 ? "in" : undefined;
}

/** How much of `window` is left, as a whole percent from 0 to 100. The daemon passes a used
 * percentage above 100 on as the agent gives it, and one below 0 is as unlikely, so both are
 * clamped rather than shown as a negative or an excess. */
function remainingPercent(window: QuotaWindow): number {
  return Math.min(100, Math.max(0, Math.round(100 - window.used_percent)));
}

/** `remainingPercent` as of `now`: a window whose reset time has passed is drawn full (100) until the next
 * reading lands, treating the passed reset as `exhaustedQuotaWindow` does. */
export function remainingPercentAt(window: QuotaWindow, now: number): number {
  return window.resets_at != null && window.resets_at <= now ? 100 : remainingPercent(window);
}

/** What names a window besides its own kind: the agent, whose own interface the names follow, and the
 * account's other windows. Any `AccountQuota` serves. */
export interface QuotaNaming {
  agent: Agent;
  windows: readonly QuotaWindow[];
}

/** Whether `window` is Claude's account-wide weekly window shown beside a model's own weekly one, which
 * Claude's interface then names "Weekly · all models". Alone, it is just "Weekly". */
function isWeeklyAllModels(naming: QuotaNaming, window: QuotaWindow): boolean {
  return window.kind === "weekly" && naming.agent === "claude" && naming.windows.some((other) => other.kind === "weekly_model");
}

/** The window's name: "Session" or "5h" (as the agent's own interface calls its short window), "Weekly",
 * "Weekly · all models" (Claude's account-wide one beside a model's own), "<Model> weekly". A credit pool is named for the length of its current period, which the agent bills over
 * (weekly on the measured account). */
export function quotaWindowLabel(t: Translate, naming: QuotaNaming, window: QuotaWindow): string {
  switch (window.kind) {
    case "session":
      return t(naming.agent === "claude" ? "quota.window.session" : "quota.window.fiveHour");
    case "weekly":
      return t(isWeeklyAllModels(naming, window) ? "quota.window.weeklyAll" : "quota.window.weekly");
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

/** The window's headline: "5h 82% remaining". */
export function quotaHeadline(t: Translate, naming: QuotaNaming, window: QuotaWindow, now: number): string {
  return t("quota.remaining", { window: quotaWindowLabel(t, naming, window), percent: remainingPercentAt(window, now) });
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

/** The window as a noun phrase inside a sentence, in lower case: "session limit", "5h limit", "weekly limit",
 * "credit limit". Unlike `quotaWindowLabel` it follows "its", so a credit pool is "credit limit" rather than
 * "Credits limit". */
export function quotaWindowPhrase(t: Translate, naming: QuotaNaming, window: QuotaWindow): string {
  switch (window.kind) {
    case "session":
      return t(naming.agent === "claude" ? "quota.windowPhrase.session" : "quota.windowPhrase.fiveHour");
    case "weekly":
      return t(isWeeklyAllModels(naming, window) ? "quota.windowPhrase.weeklyAll" : "quota.windowPhrase.weekly");
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

/** What a picker shows beside an account that cannot be chosen: "Limit reached" over "resets in 2h", without the
 * countdown when the agent gives no reset time. */
export function quotaUsedUpShort(t: Translate, window: QuotaWindow, now: number): Reason {
  const detail = quotaResetsIn(t, window, now);
  return detail ? { state: t("quota.usedUp"), detail } : { state: t("quota.usedUp") };
}

/** The same in full, for assistive technology: "Reached 5h limit · resets in 2h". */
export function quotaUsedUpDetail(t: Translate, naming: QuotaNaming, window: QuotaWindow, now: number): string {
  const resets = quotaResetsIn(t, window, now);
  const quota = quotaWindowPhrase(t, naming, window);
  return resets ? t("quota.usedUpDetailResets", { quota, resets }) : t("quota.usedUpDetail", { quota });
}

/** Why `account` cannot be used, as a sentence: "Work cannot be used for now because it has reached its 5h
 * limit. It resets in 2h." */
export function quotaUsedUpSentence(t: Translate, naming: QuotaNaming, account: string, window: QuotaWindow, now: number): string {
  const resets = quotaResetsIn(t, window, now);
  const params = { account: isolate(account), quota: quotaWindowPhrase(t, naming, window) };
  return resets ? t("quota.accountUsedUp.resets", { ...params, resets }) : t("quota.accountUsedUp", params);
}

/** The two layers of a combined meter: the 5-hour window and the account-wide weekly one beneath it. The weekly
 * layer is absent when the agent reports none. `undefined` for an account with no 5-hour window (a single
 * credit pool, say), which keeps one bar. */
export function twoLayerWindows(windows: readonly QuotaWindow[]): { session: QuotaWindow; weekly: QuotaWindow | undefined } | undefined {
  const session = windows.find((window) => window.kind === "session");
  return session && { session, weekly: windows.find((window) => window.kind === "weekly") };
}

/** The windows a compact usage bar draws: the main one, and the thin one beneath it if any. `naming` is the
 * reading they come from, which names them. */
export interface QuotaBar {
  main: QuotaWindow;
  thin?: QuotaWindow;
  naming: QuotaNaming;
}

/** The windows a compact usage bar draws: the 5-hour window with the weekly one beneath it, else the
 * account's one primary window (a credit pool, or a weekly window with no 5-hour one). `undefined`
 * for a reading with nothing to draw. */
export function usageBarWindows(quota: QuotaNaming): QuotaBar | undefined {
  const layers = twoLayerWindows(quota.windows);
  if (layers) return { main: layers.session, thin: layers.weekly, naming: quota };
  const main = primaryQuotaWindow([...quota.windows]);
  return main && { main, naming: quota };
}

/** What a compact usage bar shows, in words: "5h 82% remaining, Weekly 40% remaining". */
export function usageBarSummary(t: Translate, bar: QuotaBar, now: number): string {
  const main = quotaHeadline(t, bar.naming, bar.main, now);
  return bar.thin ? t("quota.usageSummary", { main, thin: quotaHeadline(t, bar.naming, bar.thin, now) }) : main;
}
