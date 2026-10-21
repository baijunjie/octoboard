import { describe, expect, it } from "vitest";

import { isolate } from "../i18n/isolate";
import { format, type Translate } from "../i18n/catalog";
import type { QuotaWindow } from "../protocol";
import type { AccountQuota } from "../protocol";
import { exhaustedQuotaWindow, orderedQuotaWindows, primaryQuotaWindow, planLabel, quotaHeadline, quotaResetsIn, quotaUsedUpAlert, quotaUsedUpDetail, quotaUsedUpSentence, quotaUsedUpShort, quotaWindowLabel, quotaWindowPhrase, remainingPercentAt, twoLayerWindows, usageBarSummary, usageBarWindows } from "./quotaFormat";

const t: Translate = (key, ...args) => format("en", key, args[0]);
const MINUTE = 60_000;
const NOW = 1_000_000_000;

const window = (patch: Partial<QuotaWindow> = {}): QuotaWindow => ({
  kind: "session",
  label: null,
  used_percent: 18,
  resets_at: null,
  window_minutes: null,
  ...patch,
});

it.each([
  [18, 82],
  [17.6, 82],
  [0, 100],
  [100, 0],
  [130, 0],
  [-5, 100],
])("shows %s%% used as %s%% remaining", (used, remaining) => {
  expect(remainingPercentAt(window({ used_percent: used, resets_at: null }), NOW)).toBe(remaining);
});

it("draws a window whose reset has passed as full until a new reading arrives", () => {
  const used = window({ used_percent: 100, resets_at: NOW - 1 });
  expect(remainingPercentAt(used, NOW)).toBe(100);
  expect(remainingPercentAt(used, NOW - 2)).toBe(0);
  expect(remainingPercentAt(window({ used_percent: 40, resets_at: NOW + MINUTE }), NOW)).toBe(60);
  expect(remainingPercentAt(window({ used_percent: 40, resets_at: null }), NOW)).toBe(60);
  expect(quotaHeadline(t, used, NOW)).toBe("5-hour 100% remaining");
});

it.each([
  [{ kind: "session" }, "5-hour"],
  [{ kind: "weekly" }, "Weekly"],
  [{ kind: "weekly_model", label: "Fable" }, `${isolate("Fable")} weekly`],
  [{ kind: "weekly_model", label: null }, "Weekly"],
  [{ kind: "credit_period", window_minutes: 7 * 24 * 60 }, "Weekly"],
  [{ kind: "credit_period", window_minutes: 30 * 24 * 60 }, "Monthly"],
  [{ kind: "credit_period", window_minutes: null }, "Credits"],
] as const)("names %j as %s", (patch, label) => {
  expect(quotaWindowLabel(t, window(patch))).toBe(label);
});

it("words the headline from the label and the remaining percent", () => {
  expect(quotaHeadline(t, window({ kind: "weekly_model", label: "Fable", used_percent: 0 }), NOW)).toBe(`${isolate("Fable")} weekly 100% remaining`);
});

it.each([
  [30_000, "resets in 1m"],
  [12 * MINUTE, "resets in 12m"],
  [12 * MINUTE + 1, "resets in 13m"],
  [117 * MINUTE, "resets in 1h 57m"],
  [60 * MINUTE, "resets in 1h 0m"],
  [(2 * 24 * 60 + 3 * 60 + 5) * MINUTE, "resets in 2d 3h"],
  [0, "resets now"],
  [-MINUTE, "resets now"],
])("counts down a reset %i ms ahead as %s", (ahead, text) => {
  expect(quotaResetsIn(t, window({ resets_at: NOW + ahead }), NOW)).toBe(text);
});

it("has no countdown for a window that gives no reset time", () => {
  expect(quotaResetsIn(t, window(), NOW)).toBeNull();
});

it.each([
  ["max", "Max"],
  ["plus", "Plus"],
  ["SuperGrok", "SuperGrok"],
  ["pro_lite", "Pro Lite"],
  ["team-plan", "Team Plan"],
])("displays the plan %s as %s", (plan, shown) => {
  expect(planLabel(plan)).toBe(shown);
});

it.each([
  [["weekly", "session", "credit_period"], "session"],
  [["weekly_model", "weekly", "credit_period"], "weekly"],
  [["credit_period"], "credit_period"],
  [["weekly_model"], undefined],
  [[], undefined],
] as const)("chooses one account-wide quota window from %j", (kinds, expected) => {
  expect(primaryQuotaWindow(kinds.map((kind) => window({ kind })))?.kind).toBe(expected);
});

it("orders account-wide windows before models without mutating or reordering model limits", () => {
  const fable = window({ kind: "weekly_model", label: "Fable" });
  const sonnet = window({ kind: "weekly_model", label: "Sonnet" });
  const credit = window({ kind: "credit_period" });
  const weekly = window({ kind: "weekly" });
  const session = window();
  const source = Object.freeze([fable, credit, sonnet, weekly, session]);
  expect(orderedQuotaWindows(source)).toEqual([session, weekly, credit, fable, sonnet]);
  expect(source).toEqual([fable, credit, sonnet, weekly, session]);
});

const reading = (windows: QuotaWindow[], patch: Partial<AccountQuota> = {}): AccountQuota => ({
  agent: "claude", account: null, fetched_at: NOW, plan: null, windows, unavailable: null, ...patch,
});

describe("exhaustedQuotaWindow", () => {
  it.each([
    ["a session window used up and resetting later", [window({ used_percent: 100, resets_at: NOW + MINUTE })], true],
    ["a session window used up with no reset time", [window({ used_percent: 100 })], true],
    ["a window used beyond 100%", [window({ used_percent: 130, resets_at: NOW + MINUTE })], true],
    ["a weekly window used up", [window({ kind: "weekly", used_percent: 100 })], true],
    ["a credit pool used up", [window({ kind: "credit_period", used_percent: 100 })], true],
    ["a used-up window whose reset time has passed", [window({ used_percent: 100, resets_at: NOW - 1 })], false],
    ["a used-up window that resets exactly now", [window({ used_percent: 100, resets_at: NOW })], false],
    ["a window with room left", [window({ used_percent: 99.9, resets_at: NOW + MINUTE })], false],
    ["only a model-specific weekly window used up", [window({ kind: "weekly_model", label: "Fable", used_percent: 100 })], false],
    ["one window used up among several", [window({ used_percent: 20 }), window({ kind: "weekly", used_percent: 100, resets_at: NOW + MINUTE })], true],
    ["no windows", [], false],
  ])("is %s: %s", (_name, windows, expected) => {
    expect(exhaustedQuotaWindow(reading(windows), NOW) !== undefined).toBe(expected);
  });

  it("fails open without a reading", () => {
    expect(exhaustedQuotaWindow(undefined, NOW)).toBeUndefined();
    expect(exhaustedQuotaWindow("loading", NOW)).toBeUndefined();
  });

  it("still applies the windows an unavailable reading kept", () => {
    const unavailable = { code: "quota_timed_out", params: {}, message: "" };
    expect(exhaustedQuotaWindow(reading([window({ used_percent: 100 })], { unavailable }), NOW)).toBeDefined();
  });

  it("names the used-up window that resets last, one without a reset time after the rest", () => {
    const early = window({ used_percent: 100, resets_at: NOW + MINUTE });
    const late = window({ kind: "weekly", used_percent: 100, resets_at: NOW + 60 * MINUTE });
    const open = window({ kind: "credit_period", used_percent: 100 });
    expect(exhaustedQuotaWindow(reading([early, late]), NOW)).toBe(late);
    expect(exhaustedQuotaWindow(reading([late, early]), NOW)).toBe(late);
    expect(exhaustedQuotaWindow(reading([early, open, late]), NOW)).toBe(open);
  });

  it("words the reason in short for a picker, with the countdown when the reset time is known", () => {
    expect(quotaUsedUpShort(t, window({ used_percent: 100, resets_at: NOW + 117 * MINUTE }), NOW)).toEqual({ state: "Used up", detail: "resets in 1h 57m" });
    expect(quotaUsedUpShort(t, window({ kind: "weekly", used_percent: 100 }), NOW)).toEqual({ state: "Used up" });
  });

  it("words the reason in full for assistive technology", () => {
    expect(quotaUsedUpDetail(t, window({ used_percent: 100, resets_at: NOW + 117 * MINUTE }), NOW)).toBe("Used up: 5-hour quota · resets in 1h 57m");
    expect(quotaUsedUpDetail(t, window({ kind: "weekly", used_percent: 100 }), NOW)).toBe("Used up: weekly quota");
  });

  it("words the reason as a sentence, naming a credit pool as credits and a window in lower case", () => {
    expect(quotaUsedUpSentence(t, "Work", window({ kind: "weekly", used_percent: 100, resets_at: NOW + 117 * MINUTE }), NOW)).toContain(`${isolate("Work")} cannot be used for now because it has used up its weekly quota. It resets in 1h 57m.`);
    expect(quotaUsedUpSentence(t, "Work", window({ kind: "credit_period", used_percent: 100 }), NOW)).toContain("has used up its credits.");
  });

  it.each([
    [null, "It has used up its weekly quota."],
    [NOW + 117 * MINUTE, "It has used up its weekly quota. It resets in 1h 57m."],
  ])("separates the quota Alert title from its complete reason (%s)", (resets_at, description) => {
    expect(quotaUsedUpAlert(t, "Work", window({ kind: "weekly", used_percent: 100, resets_at }), NOW)).toEqual({
      title: `${isolate("Work")} cannot be used for now`,
      description,
    });
  });

  it("names a window inside a sentence in lower case, a credit pool by its period", () => {
    expect(quotaWindowPhrase(t, window({ kind: "credit_period", window_minutes: 7 * 24 * 60 }))).toBe("weekly quota");
    expect(quotaWindowPhrase(t, window({ kind: "credit_period", window_minutes: 30 * 24 * 60 }))).toBe("monthly quota");
    expect(quotaWindowPhrase(t, window({ kind: "credit_period" }))).toBe("credits");
  });
});

describe("twoLayerWindows", () => {
  it("pairs the 5-hour window with the account-wide weekly one", () => {
    const session = window();
    const weekly = window({ kind: "weekly" });
    expect(twoLayerWindows([window({ kind: "weekly_model", label: "Fable" }), weekly, session])).toEqual({ session, weekly });
  });

  it("keeps the 5-hour window alone when there is no weekly one, and has no pair without it", () => {
    expect(twoLayerWindows([window()])).toEqual({ session: window(), weekly: undefined });
    expect(twoLayerWindows([window({ kind: "weekly" })])).toBeUndefined();
    expect(twoLayerWindows([window({ kind: "credit_period" })])).toBeUndefined();
  });
});

describe("the compact usage bar", () => {
  const session = window({ kind: "session", used_percent: 30 });
  const weekly = window({ kind: "weekly", used_percent: 60 });

  it("draws the 5-hour window with the weekly one beneath it", () => {
    expect(usageBarWindows([weekly, session])).toEqual({ main: session, thin: weekly });
  });

  it("draws a 5-hour window alone when there is no weekly one", () => {
    expect(usageBarWindows([session])).toEqual({ main: session, thin: undefined });
  });

  it("draws the one primary window of an account without a 5-hour one, never a model limit", () => {
    const credits = window({ kind: "credit_period" });
    expect(usageBarWindows([window({ kind: "weekly_model", label: "Fable" }), credits])).toEqual({ main: credits });
    expect(usageBarWindows([window({ kind: "weekly_model", label: "Fable" })])).toBeUndefined();
  });

  it("states both windows in words, or the one", () => {
    expect(usageBarSummary(t, { main: session, thin: weekly }, NOW)).toBe("5-hour 70% remaining, Weekly 40% remaining");
    expect(usageBarSummary(t, { main: weekly }, NOW)).toBe("Weekly 40% remaining");
  });
});
