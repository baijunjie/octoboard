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

it("shows short and general weekly windows before model windows with their remaining bars", () => {
  const reversed = [...windows].reverse();
  const container = mount(<AccountQuotaSummary quota={quota({ windows: reversed })} />);
  expect(reversed).toEqual([windows[2], windows[1], windows[0]]);
  expect(headlines(container)).toEqual([
    "5-hour 82% remainingresets in 1h 57m",
    "Weekly 95% remainingresets in 3d 0h",
    "Fable weekly 100% remaining",
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

it("tells an account that is not signed in, or whose directory is missing, to log in and not that reading failed", () => {
  for (const code of ["quota_not_signed_in", "quota_config_dir_missing"]) {
    const unavailable = { code, params: { agent: "Claude Code", dir: "/home/dev/.claude-work" }, message: "raw text" };
    const container = mount(<AccountQuotaSummary quota={quota({ windows: [], unavailable })} />);
    const recovery = container.querySelector("p")!;
    expect(recovery.textContent).toBe("Log in to use this account.");
    expect(container.querySelector("span")?.textContent).toBe("Not logged in yet.");
    expect(recovery.previousElementSibling?.textContent).toBe("Not logged in yet.");
    expect(container.textContent).not.toContain("Quota unavailable");
  }
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
  const summary = Array.from(container.querySelectorAll("span")).find((node) => node.textContent === "Claude Code could not report its quota.")!;
  expect(summary).toBeDefined();
  const agent = summary.querySelector("[data-inline-chip]")!;
  expect(agent.textContent).toBe("Claude Code");
  expect(agent.getAttribute("dir")).toBe("auto");
  expect(agent.textContent).not.toMatch(/[\u2066-\u2069]/);
  const diagnostic = container.querySelector("pre")!;
  expect(diagnostic.textContent).toBe("exit 1");
  expect(diagnostic.getAttribute("dir")).toBe("ltr");
  expect(container.querySelectorAll("pre")).toHaveLength(1);
  expect(summary.contains(diagnostic)).toBe(false);
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

it("draws the 5-hour and weekly windows as one two-layer meter, each bar keeping its own label", () => {
  const container = mount(<AccountQuotaSummary quota={quota()} />);
  const pairs = container.querySelectorAll(".grid > div");
  expect(pairs).toHaveLength(2);
  expect(Array.from(pairs[0]!.querySelectorAll('[data-slot="meter"]'), (meter) => meter.getAttribute("aria-label"))).toEqual(["5-hour", "Weekly"]);
});

it("keeps a single bar for an account without a 5-hour window", () => {
  const credits: QuotaWindow = { kind: "credit_period", label: null, used_percent: 10, resets_at: null, window_minutes: 10_080 };
  const container = mount(<AccountQuotaSummary quota={quota({ agent: "grok", windows: [credits] })} />);
  expect(container.querySelectorAll('[data-slot="meter"]')).toHaveLength(1);
});
