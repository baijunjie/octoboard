import { describe, expect, it } from "vitest";

import { choiceGroups } from "./choiceEntries";
import { expiringWeekly, preferExpiringWeekly } from "./accountRanking";
import type { AccountQuota, QuotaWindow } from "../protocol";
import { availability, account, NOW, HOUR, DAY, windowOf, reading } from "./accountChoicesTestSupport";

describe("preferExpiringWeekly", () => {
  const stored = [account("w", "claude", "Work"), account("soon", "claude", "Soon")];
  const entries = (quotas: AccountQuota[]) =>
    choiceGroups(stored, availability({}), null, () => "Default", { quotas, now: NOW }).find((group) => group.agent === "claude")!.entries;

  it("prefers the soonest reset over a larger remainder", () => {
    const ranked = preferExpiringWeekly(entries([
      reading("w", [windowOf({ used_percent: 10, resets_at: NOW + 3 * DAY })]),
      reading("soon", [windowOf({ used_percent: 99, resets_at: NOW + HOUR })]),
    ]), NOW);
    expect(ranked?.account).toBe("soon");
  });

  it("prefers more remaining when the reset is the same", () => {
    const ranked = preferExpiringWeekly(entries([
      reading("w", [windowOf({ used_percent: 80, resets_at: NOW + DAY })]),
      reading("soon", [windowOf({ used_percent: 10, resets_at: NOW + DAY })]),
    ]), NOW);
    expect(ranked?.account).toBe("soon");
    expect(expiringWeekly(reading("w", [
      windowOf({ used_percent: 80, resets_at: NOW + DAY }),
      windowOf({ used_percent: 10, resets_at: NOW + DAY }),
    ]), NOW)).toEqual({ resetsAt: NOW + DAY, remaining: 90 });
  });

  it("keeps the earlier entry when the reset and the remainder are equal, the default account first", () => {
    const ranked = preferExpiringWeekly(entries([
      reading(null, [windowOf({ used_percent: 40, resets_at: NOW + DAY })]),
      reading("w", [windowOf({ used_percent: 40, resets_at: NOW + DAY })]),
    ]), NOW);
    expect(ranked?.account).toBeNull();
  });

  it.each([
    ["a model-specific weekly window", windowOf({ kind: "weekly_model", label: "Fable" })],
    ["a monthly credit period", windowOf({ kind: "credit_period", window_minutes: 30 * 24 * 60 })],
    ["a session window", windowOf({ kind: "session", window_minutes: 300 })],
    ["a window that is used up", windowOf({ used_percent: 100 })],
    ["a window whose reset is not still ahead", windowOf({ resets_at: NOW })],
    ["a window whose reset is in the past", windowOf({ resets_at: NOW - 1 })],
    ["a window with no reset", windowOf({ resets_at: null })],
    ["a window whose used percent is not a number", windowOf({ used_percent: Number.NaN })],
  ] as const)("ignores %s", (_, window) => {
    expect(preferExpiringWeekly(entries([reading("w", [window])]), NOW)).toBeUndefined();
    expect(expiringWeekly(reading("w", [window]), NOW)).toBeUndefined();
  });

  it("ranks a credit period of 6 to 8 days and ignores one a minute outside that", () => {
    const week = (minutes: number) => reading("w", [windowOf({ kind: "credit_period", window_minutes: minutes, used_percent: 25, resets_at: NOW + HOUR })]);
    expect(expiringWeekly(week(6 * 24 * 60), NOW)).toEqual({ resetsAt: NOW + HOUR, remaining: 75 });
    expect(expiringWeekly(week(8 * 24 * 60), NOW)).toEqual({ resetsAt: NOW + HOUR, remaining: 75 });
    expect(expiringWeekly(week(6 * 24 * 60 - 1), NOW)).toBeUndefined();
    expect(expiringWeekly(week(8 * 24 * 60 + 1), NOW)).toBeUndefined();
    expect(expiringWeekly(week(28 * 24 * 60), NOW)).toBeUndefined();
  });

  it("takes the soonest weekly window of one account, ignoring a session window that resets sooner", () => {
    expect(expiringWeekly(reading("w", [
      windowOf({ kind: "session", window_minutes: 300, used_percent: 1, resets_at: NOW + HOUR }),
      windowOf({ used_percent: 40, resets_at: NOW + 2 * DAY }),
      windowOf({ used_percent: 80, resets_at: NOW + DAY }),
    ]), NOW)).toEqual({ resetsAt: NOW + DAY, remaining: 20 });
  });

  it("skips an entry a shorter window has used up, even when its weekly quota resets sooner", () => {
    const spent = reading("soon", [
      windowOf({ kind: "session", used_percent: 100, resets_at: NOW + HOUR, window_minutes: 300 }),
      windowOf({ used_percent: 1, resets_at: NOW + HOUR }),
    ]);
    const later = reading("w", [windowOf({ used_percent: 50, resets_at: NOW + 3 * DAY })]);
    expect(preferExpiringWeekly(entries([spent, later]), NOW)?.account).toBe("w");
  });

  it("returns nothing when no window ranks", () => {
    expect(preferExpiringWeekly(entries([]), NOW)).toBeUndefined();
    expect(expiringWeekly(undefined, NOW)).toBeUndefined();
  });
});

describe("preferExpiringWeekly and the session window", () => {
  const stored = [account("w", "claude", "Work"), account("soon", "claude", "Soon")];
  const session = (patch: Partial<QuotaWindow> = {}) => windowOf({ kind: "session", window_minutes: 300, used_percent: 10, resets_at: NOW + 3 * HOUR, ...patch });
  const pick = (quotas: AccountQuota[]) =>
    preferExpiringWeekly(choiceGroups(stored, availability({}), null, () => "Default", { quotas, now: NOW }).find((group) => group.agent === "claude")!.entries, NOW)?.account;

  it("prefers a running session window when the weekly resets are within a session window of each other", () => {
    expect(pick([
      reading("w", [windowOf({ resets_at: NOW + 24 * HOUR })]),
      reading("soon", [windowOf({ resets_at: NOW + 26 * HOUR }), session()]),
    ])).toBe("soon");
  });

  it("leaves a weekly reset that is far later out of the tie", () => {
    expect(pick([
      reading("w", [windowOf({ resets_at: NOW + 4 * HOUR })]),
      reading("soon", [windowOf({ resets_at: NOW + 6 * DAY }), session()]),
    ])).toBe("w");
  });

  it("ties a weekly reset less than a session window after the soonest, but not exactly one", () => {
    const soonest = reading("w", [windowOf({ resets_at: NOW + 24 * HOUR })]);
    expect(pick([soonest, reading("soon", [windowOf({ resets_at: NOW + 29 * HOUR - 1 }), session()])])).toBe("soon");
    expect(pick([soonest, reading("soon", [windowOf({ resets_at: NOW + 29 * HOUR }), session()])])).toBe("w");
  });

  it("measures the tie against the soonest reset rather than chaining", () => {
    const third = [...stored, account("far", "claude", "Far")];
    const entries = choiceGroups(third, availability({}), null, () => "Default", {
      quotas: [
        reading("w", [windowOf({ resets_at: NOW + 24 * HOUR })]),
        reading("soon", [windowOf({ resets_at: NOW + 27 * HOUR })]),
        reading("far", [windowOf({ resets_at: NOW + 31 * HOUR }), session()]),
      ],
      now: NOW,
    }).find((group) => group.agent === "claude")!.entries;
    expect(preferExpiringWeekly(entries, NOW)?.account).toBe("w");
  });

  it("ranks the sooner session reset first, then the sooner weekly reset", () => {
    expect(pick([
      reading("w", [windowOf({ resets_at: NOW + 24 * HOUR }), session({ resets_at: NOW + 3 * HOUR })]),
      reading("soon", [windowOf({ resets_at: NOW + 25 * HOUR }), session({ resets_at: NOW + 2 * HOUR })]),
    ])).toBe("soon");
    expect(pick([
      reading("w", [windowOf({ resets_at: NOW + 25 * HOUR })]),
      reading("soon", [windowOf({ resets_at: NOW + 24 * HOUR })]),
    ])).toBe("soon");
  });

  it.each([
    ["used up", session({ used_percent: 100 })],
    ["already reset", session({ resets_at: NOW })],
    ["without a reset", session({ resets_at: null })],
  ])("does not count a session window that is %s as running", (_, window) => {
    expect(pick([
      reading("w", [windowOf({ resets_at: NOW + 24 * HOUR })]),
      reading("soon", [windowOf({ resets_at: NOW + 25 * HOUR }), window]),
    ])).toBe("w");
  });
});
