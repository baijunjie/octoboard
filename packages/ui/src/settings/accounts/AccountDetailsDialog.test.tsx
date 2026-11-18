// @vitest-environment jsdom
import { beforeEach, expect, it, vi } from "vitest";

import type { AccountQuota, AgentAvailability, QuotaWindow } from "../../protocol";
import { LanguageProvider } from "../../i18n/react";
import { fakeTimers } from "../../test/clock";
import { mountHost } from "../../test/mountHost";
import { testDaemon } from "../../test/testDaemon";
import { AccountDetailsDialog } from "./AccountDetailsDialog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const MINUTE = 60_000;
const NOW = Date.UTC(2026, 9, 18, 12, 0, 0);

beforeEach(() => {
  fakeTimers();
  vi.setSystemTime(NOW);
});

const known: QuotaWindow = { kind: "session", label: null, used_percent: 18, resets_at: null, window_minutes: 300 };
const future: QuotaWindow = { kind: "from_a_newer_daemon", label: "Future", used_percent: 100, resets_at: null, window_minutes: null };
const failure = { code: "quota_command_failed", params: { agent: "Claude Code", detail: "exit 1" }, message: "failed" };

/** The details of the default Claude Code account over a reading holding `windows`, failed or not. */
function mountDetails(windows: QuotaWindow[], unavailable: AccountQuota["unavailable"] = null): HTMLElement {
  const reading: AccountQuota = { agent: "claude", account: null, fetched_at: NOW - 5 * MINUTE, plan: "max", windows, unavailable, email: null };
  const daemon = testDaemon({
    state: { accountQuotas: [reading], agentAvailability: new Map([["claude", { agent: "claude", availability: "available", default_account_dir: "/home/dev/.claude" } as AgentAvailability]]) },
  });
  mountHost(<LanguageProvider><AccountDetailsDialog agent="claude" account={null} onClose={() => {}} /></LanguageProvider>, { daemon });
  return document.querySelector<HTMLElement>('[role="dialog"]')!;
}

const meters = (dialog: HTMLElement) => dialog.querySelectorAll('[data-slot="meter"]');

it("draws the windows of a reading it knows, with when they were read", () => {
  const dialog = mountDetails([future, known]);
  expect(meters(dialog)).toHaveLength(1);
  expect(dialog.textContent).toContain("Session");
  expect(dialog.textContent).toContain("Updated 5 minutes ago");
  expect(dialog.textContent).toContain("Logged in");
  expect(dialog.textContent).not.toContain("Future");
});

it("draws no window and says nothing of when it was read for a reading holding only windows of a kind it does not know, which reads as no usage", () => {
  const dialog = mountDetails([future]);
  expect(meters(dialog)).toHaveLength(0);
  expect(dialog.textContent).not.toContain("Future");
  expect(dialog.textContent).not.toContain("Updated");
  expect(dialog.textContent).toContain("No usage reading");
  expect(dialog.textContent).not.toContain("Limit reached");
});

it("marks a failed reading holding only windows of a kind it does not know as unavailable, not as out of date, and states no login", () => {
  const dialog = mountDetails([future], failure);
  expect(meters(dialog)).toHaveLength(0);
  expect(dialog.querySelector('button[aria-label="Usage unavailable"]')).not.toBeNull();
  expect(dialog.textContent).not.toContain("Out of date");
  expect(dialog.textContent).not.toContain("Updated");
  expect(dialog.textContent).not.toContain("Logged in");
  expect(dialog.textContent).not.toContain("Not logged in");
});
