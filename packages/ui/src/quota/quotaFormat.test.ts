import { expect, it } from "vitest";

import { isolate } from "../i18n/isolate";
import { format, type Translate } from "../i18n/catalog";
import type { QuotaWindow } from "../protocol";
import { orderedQuotaWindows, primaryQuotaWindow, planLabel, quotaHeadline, quotaResetsIn, quotaWindowLabel, remainingPercent } from "./quotaFormat";

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
  expect(remainingPercent(window({ used_percent: used }))).toBe(remaining);
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
  expect(quotaHeadline(t, window({ kind: "weekly_model", label: "Fable", used_percent: 0 }))).toBe(`${isolate("Fable")} weekly 100% remaining`);
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
