// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { isolate } from "../i18n/isolate";
import { currentLanguage, setLanguage } from "../i18n/language";
import type { AccountQuota, QuotaWindow } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { AccountQuotaSummary, QuotaPlan } from "./AccountQuotaSummary";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const MINUTE = 60_000;
const NOW = Date.UTC(2026, 9, 18, 12, 0, 0);

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(NOW);
});
afterEach(() => {
  vi.useRealTimers();
});

const windows: QuotaWindow[] = [
  { kind: "session", label: null, used_percent: 18, resets_at: NOW + 117 * MINUTE, window_minutes: 300 },
  { kind: "weekly", label: null, used_percent: 5, resets_at: NOW + 3 * 24 * 60 * MINUTE, window_minutes: 10_080 },
  { kind: "weekly_model", label: "Fable", used_percent: 0, resets_at: null, window_minutes: 10_080 },
];
const quota = (patch: Partial<AccountQuota> = {}): AccountQuota => ({
  agent: "claude",
  account: null,
  fetched_at: NOW - 5 * MINUTE,
  plan: "max",
  windows,
  unavailable: null,
  ...patch,
});

function mount(element: React.ReactNode): HTMLElement {
  const previousLanguage = currentLanguage();
  act(() => setLanguage("en"));
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    act(() => setLanguage(previousLanguage));
  });
  const daemon = { store: createStateStore({}) } as Daemon;
  act(() => root.render(<DaemonProvider value={daemon}>{element}</DaemonProvider>));
  return container;
}

/** The text with the bidi isolates that wrap the values put into a sentence taken out. */
const plain = (container: HTMLElement) => container.textContent?.replace(/[\u2068\u2069]/g, "");

const headlines = (container: HTMLElement) =>
  Array.from(container.querySelectorAll('[aria-hidden="true"]'), (row) => row.textContent);

it("shows each window's headline, countdown and a bar of what remains", () => {
  const container = mount(<AccountQuotaSummary quota={quota()} />);
  expect(headlines(container)).toEqual([
    "5-hour 82% remainingresets in 1h 57m",
    "Weekly 95% remainingresets in 3d 0h",
    `${isolate("Fable")} weekly 100% remaining`,
  ]);
  const meters = Array.from(container.querySelectorAll('[data-slot="meter"]'));
  expect(meters.map((meter) => meter.getAttribute("aria-valuenow"))).toEqual(["82", "95", "100"]);
  expect(meters.map((meter) => meter.getAttribute("aria-label"))).toEqual(["5-hour", "Weekly", `${isolate("Fable")} weekly`]);
  expect(meters[0]!.getAttribute("aria-valuetext")).toBe("82% remaining, resets in 1h 57m");
  expect(meters[2]!.getAttribute("aria-valuetext")).toBe("100% remaining");
  expect(meters.every((meter) => meter.classList.contains("quota-meter--claude"))).toBe(true);
  expect(container.querySelector('[data-slot="meter-fill"]')?.getAttribute("style")).toContain("width: 82%");
});

it("counts the reset down as time passes", () => {
  const container = mount(<AccountQuotaSummary quota={quota()} />);
  act(() => vi.advanceTimersByTime(60 * MINUTE));
  expect(headlines(container)[0]).toBe("5-hour 82% remainingresets in 57m");
});

it("waits for the first reading of an account that has no entry yet", () => {
  const container = mount(<><AccountQuotaSummary quota="loading" /><QuotaPlan quota="loading" /></>);
  expect(container.textContent).toBe("Reading quota…");
  expect(container.querySelector('[data-slot="meter"]')).toBeNull();
});

it("says why an account that was never read has no quota", () => {
  const unavailable = { code: "quota_timed_out", params: { agent: "Claude Code" }, message: "timed out" };
  const container = mount(<AccountQuotaSummary quota={quota({ windows: [], unavailable })} />);
  expect(container.textContent).toContain("Quota unavailable");
  expect(plain(container)).toContain("Claude Code took too long to report its quota.");
  expect(container.querySelector('[data-slot="meter"]')).toBeNull();
});

it("sets the binary and its path variable apart as code in a missing-binary failure", () => {
  const unavailable = { code: "quota_binary_not_found", params: { binary: "claude" }, message: "not found" };
  const container = mount(<AccountQuotaSummary quota={quota({ windows: [], unavailable })} />);
  expect(Array.from(container.querySelectorAll("code"), (code) => code.textContent)).toEqual(["claude", "PATH"]);
});

it("keeps the last good windows as out of date, with when they were read and why the newer read failed", () => {
  const unavailable = { code: "quota_command_failed", params: { agent: "Claude Code", detail: "exit 1" }, message: "failed" };
  const container = mount(<AccountQuotaSummary quota={quota({ unavailable })} />);
  expect(container.textContent).toContain("Out of date: last read 5 minutes ago");
  expect(plain(container)).toContain("Claude Code could not report its quota: exit 1");
  expect(container.querySelectorAll('[data-slot="meter"]')).toHaveLength(3);
});

it("falls back to the daemon's own wording for a failure code it has no message for", () => {
  const unavailable = { code: "quota_from_the_future", params: {}, message: "Newer daemon wording" };
  const container = mount(<AccountQuotaSummary quota={quota({ windows: [], unavailable })} />);
  expect(container.textContent).toContain("Newer daemon wording");
});

it("shows nothing for an account the daemon has not reported on", () => {
  const container = mount(<><AccountQuotaSummary quota={undefined} /><QuotaPlan quota={undefined} /></>);
  expect(container.textContent).toBe("");
});

it("shows the plan the agent names", () => {
  expect(mount(<QuotaPlan quota={quota({ plan: "max" })} />).textContent).toBe("Max");
});

it("shows no plan when the agent names none", () => {
  expect(mount(<QuotaPlan quota={quota({ plan: null })} />).textContent).toBe("");
});
