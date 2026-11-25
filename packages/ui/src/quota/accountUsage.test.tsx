// @vitest-environment jsdom
import { renderToStaticMarkup } from "react-dom/server";
import { expect, it } from "vitest";

import type { AccountQuota, QuotaWindow } from "../protocol";
import { translatorFor } from "../i18n/catalog";
import { TEST_PROVIDERS } from "../test/testProviders";
import { accountUsage } from "./accountUsage";
import { PROVIDER_FILL_CLASS } from "./providerFill";

const NOW = Date.UTC(2026, 9, 18, 12, 0, 0);
const HOUR = 60 * 60_000;
const t = translatorFor("en");

const exhausted: QuotaWindow = { kind: "session", label: null, used_percent: 100, resets_at: NOW + HOUR, window_minutes: 300 };
const quota = (window: QuotaWindow): AccountQuota => ({ agent: "claude", account: null, fetched_at: NOW, plan: null, windows: [window], unavailable: null, email: null });
const bar = (usage: ReturnType<typeof accountUsage>) => {
  const host = document.createElement("div");
  host.innerHTML = renderToStaticMarkup(usage.icon);
  return host.firstElementChild!;
};

it("draws the usage bar of a used-up account grey, in place of the provider fill, and says why in words", () => {
  const usage = accountUsage(t, TEST_PROVIDERS, "claude", quota(exhausted), exhausted, NOW);
  expect(bar(usage).classList.contains("quota-meter--used-up")).toBe(true);
  expect(bar(usage).classList.contains(PROVIDER_FILL_CLASS)).toBe(false);
  expect(usage.detail).toBeTruthy();
  expect(usage.reason).toBeTruthy();
});

it("draws the usage bar of an account that is not used up in its provider's fill", () => {
  const window: QuotaWindow = { ...exhausted, used_percent: 30 };
  const usage = accountUsage(t, TEST_PROVIDERS, "claude", quota(window), undefined, NOW);
  expect(bar(usage).classList.contains(PROVIDER_FILL_CLASS)).toBe(true);
  expect(bar(usage).classList.contains("quota-meter--used-up")).toBe(false);
  expect(usage.reason).toBeUndefined();
});

it("draws an account with no reading in its provider's fill", () => {
  expect(bar(accountUsage(t, TEST_PROVIDERS, "claude", undefined, undefined, NOW)).classList.contains(PROVIDER_FILL_CLASS)).toBe(true);
});

it("draws no bar and says nothing in words for an account of a provider that declares no plan quota, whatever its reading holds", () => {
  const own = { ...quota(exhausted), agent: "test_agent" };
  for (const held of [undefined, own, { ...own, windows: [] }]) {
    expect(accountUsage(t, TEST_PROVIDERS, "test_agent", held, undefined, NOW)).toEqual({});
  }
  // Not even a window that would read as used up draws one: there is no quota to be used up.
  expect(accountUsage(t, TEST_PROVIDERS, "test_agent", own, exhausted, NOW)).toEqual({});
});
