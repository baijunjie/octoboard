import type { AccountQuota, Agent, QuotaWindow } from "../protocol";
import { isUsable, type ChoiceEntry, type ChoiceGroup } from "./choiceEntries";
import { creditPeriodLength } from "./quotaFormat";

/** Whether `window` is the account-wide weekly quota. A model-specific weekly window and a session
 * window are not: the former is one model's limit, the latter resets within hours. A credit period is weekly
 * by its length; a monthly one is not. */
function isWeeklyQuota(window: QuotaWindow): boolean {
  return window.kind === "weekly" || (window.kind === "credit_period" && creditPeriodLength(window) === "weekly");
}

/** The weekly window an account is ranked by: when it resets and how much of it is left. */
type WeeklySignal = { resetsAt: number; remaining: number };

/** How long a week is, for a weekly window that does not give its own length. */
const WEEK_MS = 7 * 24 * 60 * 60 * 1000;

/** The weekly window of `quota` that resets soonest and still has quota left. A window with no reset has not
 * started, so its whole length is still ahead: it ranks as resetting one window length (a week when not given)
 * from `now`. Of several, the soonest reset wins, then the larger remainder. Nothing rankable returns undefined. */
export function expiringWeekly(quota: AccountQuota | undefined, now: number): WeeklySignal | undefined {
  if (!quota) return undefined;
  let best: WeeklySignal | undefined;
  for (const window of quota.windows) {
    if (!isWeeklyQuota(window) || !Number.isFinite(window.used_percent) || window.used_percent >= 100) continue;
    const resetsAt =
      typeof window.resets_at === "number"
        ? window.resets_at
        : now + (window.window_minutes != null ? window.window_minutes * 60_000 : WEEK_MS);
    if (!(resetsAt > now)) continue;
    const candidate = { resetsAt, remaining: Math.max(0, 100 - window.used_percent) };
    if (!best || candidate.resetsAt < best.resetsAt || (candidate.resetsAt === best.resetsAt && candidate.remaining > best.remaining)) {
      best = candidate;
    }
  }
  return best;
}

/** Weekly resets less than this far apart count as the same deadline: one session window. The quota of a
 * session window that is already running is lost at its reset, so among the accounts whose weekly quota
 * expires within this of the soonest one, the running session window is spent first and the weekly reset
 * decides only between equals. */
const SESSION_WINDOW_TOLERANCE_MS = 5 * 60 * 60 * 1000;

/** When the session window of `quota` that is running resets: one with quota left and a reset still ahead.
 * A reading holds a reset only for a window that is counting, so a window with none is not. Of several,
 * the soonest. */
function runningSessionReset(quota: AccountQuota | undefined, now: number): number | undefined {
  let soonest: number | undefined;
  for (const window of quota?.windows ?? []) {
    if (window.kind !== "session" || !Number.isFinite(window.used_percent) || window.used_percent >= 100) continue;
    if (typeof window.resets_at !== "number" || !(window.resets_at > now)) continue;
    if (soonest === undefined || window.resets_at < soonest) soonest = window.resets_at;
  }
  return soonest;
}

/** What ranks an account for automatic selection: its weekly window and when its running session window resets. */
interface AccountSignal {
  weekly: WeeklySignal;
  sessionResetsAt: number | undefined;
}

/** Whether the tied signal `a` ranks ahead of `b`: a running session window before none, the sooner session
 * reset first, then the sooner weekly reset, then the larger remainder. An equal pair does not. */
function ranksAheadWithinTie(a: AccountSignal, b: AccountSignal): boolean {
  const session = (signal: AccountSignal) => signal.sessionResetsAt ?? Infinity;
  if (session(a) !== session(b)) return session(a) < session(b);
  if (a.weekly.resetsAt !== b.weekly.resetsAt) return a.weekly.resetsAt < b.weekly.resetsAt;
  return a.weekly.remaining > b.weekly.remaining;
}

/** The entry to use first among entries that are not used up. Those whose weekly reset is less than a session
 * window after the soonest one are tied: of them a running session window wins, the sooner session reset
 * first, then the sooner weekly reset, then the larger remainder. A weekly reset a session window or more
 * after the soonest does not take part. Equal signals keep `entries` order supplied by the caller. No
 * rankable window returns undefined. */
export function preferExpiringWeekly(entries: ChoiceEntry[], now: number): ChoiceEntry | undefined {
  const ranked: (AccountSignal & { entry: ChoiceEntry })[] = [];
  for (const entry of entries) {
    if (!isUsable(entry)) continue;
    const weekly = expiringWeekly(entry.quota, now);
    if (weekly) ranked.push({ entry, weekly, sessionResetsAt: runningSessionReset(entry.quota, now) });
  }
  const soonest = Math.min(...ranked.map((signal) => signal.weekly.resetsAt));
  let best: (typeof ranked)[number] | undefined;
  for (const signal of ranked) {
    if (signal.weekly.resetsAt - soonest >= SESSION_WINDOW_TOLERANCE_MS) continue;
    if (!best || ranksAheadWithinTie(signal, best)) best = signal;
  }
  return best?.entry;
}

/** The entries of `agent` in the order automatic selection takes them, which breaks a tie as the daemon does:
 * the group's, the user's saved account order. */
export function selectionEntries(groups: ChoiceGroup[], agent: Agent): ChoiceEntry[] {
  return groups.find((candidate) => candidate.agent === agent)?.entries ?? [];
}

/** The entry that becomes usable soonest, among those kept off only by a used-up quota (not turned off, not
 * logged out, no sharing change running): it is usable when the window that blocks it (`exhaustedBy`, the one
 * that resets last) resets, and a blocking window with no reset time counts as never. Equal times keep `entries`
 * order. No such entry returns undefined. */
export function soonestAvailable(entries: ChoiceEntry[]): ChoiceEntry | undefined {
  let best: ChoiceEntry | undefined;
  let bestAt = Infinity;
  for (const entry of entries) {
    if (!entry.exhaustedBy || entry.changingSharing || entry.signedOut || entry.disabled) continue;
    const at = entry.exhaustedBy.resets_at ?? Infinity;
    if (!best || at < bestAt) {
      best = entry;
      bestAt = at;
    }
  }
  return best;
}
