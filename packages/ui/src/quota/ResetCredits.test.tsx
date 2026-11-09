// @vitest-environment jsdom
import { act } from "react";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { currentLanguage, setLanguage } from "../i18n/language";
import type { AccountQuota, Event, ClientRequestBody, ResetCredit, ResetCreditOutcome } from "../protocol";
import type { Daemon } from "../store";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { READING_WAIT_MS, ResetCreditsLine, UseResetCreditDialog, useResetFocusReturn } from "./ResetCredits";
import { afterQueuedWork, fakeTimers, flushPromises } from "../test/clock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const NOW = Date.UTC(2026, 9, 18, 12, 0, 0);
const DAY = 24 * 60 * 60_000;
const language = currentLanguage();

beforeEach(() => {
  // Only `Date` is faked here, so the dates are the same on every run and the rest of the clock stays real. A test that
  // also needs the timers installs the whole clock with `withTimers`, which replaces this one.
  fakeTimers({ now: NOW, toFake: ["Date"] });
  act(() => setLanguage("en"));
});
afterEach(() => {
  act(() => setLanguage(language));
});

const credit = (overrides: Partial<ResetCredit> = {}): ResetCredit => ({
  id: "credit-1",
  title: "Full reset (Weekly + 5 hr)",
  description: "Thanks for using Codex! You've been granted one free rate limit reset.",
  granted_at: NOW - DAY,
  expires_at: NOW + 6 * DAY + 3 * 60 * 60_000,
  ...overrides,
});

const quotaOf = (reset_credits: AccountQuota["reset_credits"]): AccountQuota => ({
  agent: "codex",
  account: null,
  fetched_at: NOW,
  plan: "plus",
  email: null,
  windows: [{ kind: "session", label: null, used_percent: 40, resets_at: NOW + DAY, window_minutes: 300 }],
  unavailable: null,
  reset_credits,
});

function fakeDaemon(quota: AccountQuota, reply: (body: ClientRequestBody) => Promise<Event>) {
  const request = vi.fn(reply);
  const showNotice = vi.fn();
  const daemon = testDaemon({ state: { accountQuotas: [quota] }, request, showNotice });
  const { store } = daemon;
  return { daemon, request, showNotice, store };
}

const mount = (daemon: Daemon, ui: React.ReactElement) => mountHost(ui, { daemon });

const press = (element: Element) => act(() => (element as HTMLElement).click());
const used = (outcome: ResetCreditOutcome) => (_body: ClientRequestBody) => Promise.resolve({ type: "reset_credit_used", id: null, outcome } as Event);

it("shows the count and when the first credit expires, beside an action that asks to use it", () => {
  const { daemon } = fakeDaemon(quotaOf(undefined), used("reset"));
  const onUse = vi.fn();
  const container = mount(daemon, <ResetCreditsLine quota={quotaOf({ available_count: 1, credits: [credit()] })} accountName="Codex Work" onUse={onUse} />);
  expect(container.textContent).toContain("1 usage limit reset available · expires in 6d 3h");
  const button = container.querySelector("button")!;
  expect(button.textContent).toBe("Redeem reset");
  expect(button.getAttribute("aria-label")).toBe("Redeem reset for ⁨Codex Work⁩");
  press(button);
  expect(onUse).toHaveBeenCalledOnce();
});

it("says only the count when the list is not known or the first credit never expires", () => {
  const { daemon } = fakeDaemon(quotaOf(undefined), used("reset"));
  const text = (resetCredits: AccountQuota["reset_credits"]) =>
    mount(daemon, <ResetCreditsLine quota={quotaOf(resetCredits)} accountName="A" onUse={() => {}} />).textContent;
  expect(text({ available_count: 2, credits: null })).toContain("2 usage limit resets available");
  expect(text({ available_count: 2, credits: null })).not.toContain("expires");
  expect(text({ available_count: 1, credits: [credit({ expires_at: null })] })).not.toContain("expires");
});

it("shows nothing for an account with no reset credit, a reading still to come, or no reading", () => {
  const { daemon } = fakeDaemon(quotaOf(undefined), used("reset"));
  for (const quota of [quotaOf(undefined), "loading", undefined] as const) {
    const container = mount(daemon, <ResetCreditsLine quota={quota} accountName="A" onUse={() => {}} />);
    expect(container.textContent).toBe("");
  }
});

const dialogText = () => document.body.querySelector("[role=alertdialog]")?.textContent ?? "";
/** The confirm button, the last of the footer's; it is no `type=submit` while it is pending. */
const submit = () => document.body.querySelector<HTMLButtonElement>("[role=alertdialog] footer > button:last-child, .alert-dialog__footer > button:last-child")!;

it("names the credit, its expiry and what using it does, and uses it on confirmation, then offers Done", async () => {
  const { daemon, request, showNotice } = fakeDaemon(quotaOf({ available_count: 1, credits: [credit()] }), used("reset"));
  const onClose = vi.fn();
  mount(daemon, <UseResetCreditDialog agent="codex" account={null} accountName="Codex Work" onClose={onClose} />);
  const text = dialogText();
  expect(text).toContain("Redeem reset");
  expect(text).toContain("Full reset (Weekly + 5 hr)");
  expect(text).toContain("You've been granted one free rate limit reset.");
  expect(text).toContain("Expires in 6d 3h");
  expect(text).toContain("resets the account's eligible usage windows now");
  expect(text).not.toContain("cannot be undone");
  expect(request).not.toHaveBeenCalled();

  press(submit());
  await flushPromises();
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "use_reset_credit", agent: "codex", account: null });
  // A use that worked is told in the dialog, which stays open for the meters to fill; no notice, no closing yet.
  expect(showNotice).not.toHaveBeenCalled();
  expect(onClose).not.toHaveBeenCalled();
  expect(dialogText()).toContain("Usage reset");
  expect(dialogText()).toContain("Usage of");
  expect(dialogText()).toContain("Updating usage…");
  expect(dialogText()).not.toContain("resets the account's eligible usage windows");
  expect(footerButtons()).toEqual(["Done"]);
  press(submit());
  expect(onClose).toHaveBeenCalledOnce();
});

const footerButtons = () => Array.from(document.body.querySelectorAll(".alert-dialog__footer > button"), (button) => button.textContent);
const meterValues = () => Array.from(document.body.querySelectorAll("[role=alertdialog] [aria-valuenow]"), (meter) => meter.getAttribute("aria-valuenow"));

it("keeps the meters as they were until the reading after the use arrives, then shows the new values", async () => {
  const { daemon, store } = fakeDaemon(quotaOf({ available_count: 1, credits: [credit()] }), used("reset"));
  mount(daemon, <UseResetCreditDialog agent="codex" account={null} accountName="Codex Work" onClose={() => {}} />);
  expect(meterValues()).toEqual(["60"]);
  press(submit());
  await flushPromises();
  // A broadcast that is not a newer reading changes nothing.
  act(() => store.setState({ accountQuotas: [{ ...quotaOf({ available_count: 1, credits: [credit()] }) }] }));
  expect(meterValues()).toEqual(["60"]);
  expect(dialogText()).toContain("Updating usage…");

  const after = { ...quotaOf(undefined), fetched_at: NOW + 1000, windows: [{ kind: "session" as const, label: null, used_percent: 0, resets_at: NOW + DAY, window_minutes: 300 }] };
  act(() => store.setState({ accountQuotas: [after] }));
  expect(meterValues()).toEqual(["100"]);
  expect(dialogText()).toContain("Usage updated.");
  expect(dialogText()).not.toContain("Updating usage…");
});

/** What the dialog's status region says once its delay has passed; the visible status line is hidden from screen readers. */
const spoken = () => {
  act(() => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
  return document.body.querySelector("[role=alertdialog] [role=status]")!.textContent;
};
const withTimers = () => void fakeTimers({ now: NOW });
const readingAt = (fetched_at: number, used_percent = 0) => ({ ...quotaOf(undefined), fetched_at, windows: [{ kind: "session" as const, label: null, used_percent, resets_at: NOW + DAY, window_minutes: 300 }] });

it("says the use worked, then the new values once the reading arrives, and hides the visible line from screen readers", async () => {
  withTimers();
  const { daemon, store } = fakeDaemon(quotaOf({ available_count: 1, credits: [credit()] }), used("reset"));
  mount(daemon, <UseResetCreditDialog agent="codex" account={null} accountName="Codex Work" onClose={() => {}} />);
  expect(spoken()).toBe("");
  press(submit());
  await flushPromises();
  expect(spoken()).toBe("Usage of ⁨Codex Work⁩ has been reset.");
  const hidden = Array.from(document.body.querySelectorAll("[role=alertdialog] [aria-hidden=true]"), (node) => node.textContent);
  expect(hidden).toContain("Updating usage…");
  act(() => store.setState({ accountQuotas: [readingAt(NOW + 1000)] }));
  expect(spoken()).toContain("Usage updated. ");
  expect(spoken()).toContain("5h 100% remaining");
});

it("says none is left when a reading takes the last credit away while asking", () => {
  withTimers();
  const { daemon, store } = fakeDaemon(quotaOf({ available_count: 1, credits: [credit()] }), used("reset"));
  mount(daemon, <UseResetCreditDialog agent="codex" account={null} accountName="Codex Work" onClose={() => {}} />);
  expect(spoken()).toBe("");
  act(() => store.setState({ accountQuotas: [readingAt(NOW + 1000, 40)] }));
  expect(spoken()).toBe("⁨Codex Work⁩ has no usage limit resets available.");
  expect(submit().disabled).toBe(true);
});

it("keeps asking from the reading at the press while the request runs, though a broadcast takes the last credit away", async () => {
  withTimers();
  let settle!: (event: Event) => void;
  const { daemon, store } = fakeDaemon(quotaOf({ available_count: 1, credits: [credit()] }), () => new Promise<Event>((resolve) => { settle = resolve; }));
  mount(daemon, <UseResetCreditDialog agent="codex" account={null} accountName="Codex Work" onClose={() => {}} />);
  press(submit());
  await flushPromises();
  // The daemon takes the used credit out of the held reading before it replies.
  act(() => store.setState({ accountQuotas: [quotaOf(undefined)] }));
  expect(dialogText()).toContain("One usage limit reset will be used on");
  expect(dialogText()).not.toContain("has no usage limit resets available.");
  expect(submit().textContent).toBe("Use reset");
  expect(submit().getAttribute("aria-busy")).toBe("true");
  expect(spoken()).toBe("");
  settle({ type: "reset_credit_used", id: null, outcome: "reset" } as Event);
  await flushPromises();
  expect(spoken()).toBe("Usage of ⁨Codex Work⁩ has been reset.");
});

it("goes back to the live reading when the request fails", async () => {
  const { daemon, store } = fakeDaemon(quotaOf({ available_count: 1, credits: [credit()] }), () => Promise.reject(new Error("refused")));
  mount(daemon, <UseResetCreditDialog agent="codex" account={null} accountName="Codex Work" onClose={() => {}} />);
  press(submit());
  await flushPromises();
  act(() => store.setState({ accountQuotas: [quotaOf(undefined)] }));
  expect(dialogText()).toContain("has no usage limit resets available.");
  expect(submit().disabled).toBe(true);
});

it("begins every write after the use with that the reset worked, with the usage after it", async () => {
  withTimers();
  const { daemon, store } = fakeDaemon(quotaOf({ available_count: 1, credits: [credit()] }), used("reset"));
  mount(daemon, <UseResetCreditDialog agent="codex" account={null} accountName="Codex Work" onClose={() => {}} />);
  press(submit());
  await flushPromises();
  act(() => store.setState({ accountQuotas: [readingAt(NOW + 1000)] }));
  expect(spoken()).toMatch(/^Usage of ⁨Codex Work⁩ has been reset\. Usage updated\. 5h 100% remaining/);
});

it("begins the write for a reading that failed with that the reset worked", async () => {
  withTimers();
  const { daemon, store } = fakeDaemon(quotaOf({ available_count: 1, credits: [credit()] }), used("reset"));
  mount(daemon, <UseResetCreditDialog agent="codex" account={null} accountName="Codex Work" onClose={() => {}} />);
  press(submit());
  await flushPromises();
  act(() => store.setState({ accountQuotas: [{ ...quotaOf(undefined), unavailable: { code: "quota_timed_out", params: { agent: "Codex" }, message: "late" } }] }));
  expect(spoken()).toBe("Usage of ⁨Codex Work⁩ has been reset. Usage could not be updated yet.");
});

it("takes the baseline for the reading after the use from the store when the reply comes, not from the last render", async () => {
  let settle!: (event: Event) => void;
  const { daemon, store } = fakeDaemon(quotaOf({ available_count: 1, credits: [credit()] }), () => new Promise<Event>((resolve) => { settle = resolve; }));
  mount(daemon, <UseResetCreditDialog agent="codex" account={null} accountName="Codex Work" onClose={() => {}} />);
  press(submit());
  await flushPromises();
  // A reading lands and the reply follows within the same turn, before the dialog has rendered with it.
  store.setState({ accountQuotas: [readingAt(NOW + 500, 40)] });
  settle({ type: "reset_credit_used", id: null, outcome: "reset" } as Event);
  await flushPromises();
  expect(dialogText()).toContain("Updating usage…");
  act(() => store.setState({ accountQuotas: [readingAt(NOW + 1000)] }));
  expect(dialogText()).toContain("Usage updated.");
});

it("stays updated when the wait for the reading would have run out after it arrived", async () => {
  withTimers();
  const { daemon, store } = fakeDaemon(quotaOf({ available_count: 1, credits: [credit()] }), used("reset"));
  mount(daemon, <UseResetCreditDialog agent="codex" account={null} accountName="Codex Work" onClose={() => {}} />);
  press(submit());
  await flushPromises();
  act(() => store.setState({ accountQuotas: [readingAt(NOW + 1000)] }));
  expect(dialogText()).toContain("Usage updated.");
  act(() => void vi.advanceTimersByTime(READING_WAIT_MS));
  expect(dialogText()).toContain("Usage updated.");
  expect(dialogText()).not.toContain("could not be updated");
  expect(meterValues()).toEqual(["100"]);
});

it("does not take a reading that landed while the use was running for the one after it", async () => {
  let settle!: (event: Event) => void;
  const { daemon, store } = fakeDaemon(quotaOf({ available_count: 1, credits: [credit()] }), () => new Promise<Event>((resolve) => { settle = resolve; }));
  mount(daemon, <UseResetCreditDialog agent="codex" account={null} accountName="Codex Work" onClose={() => {}} />);
  press(submit());
  await flushPromises();
  // A probe that began before the use and ended during it.
  act(() => store.setState({ accountQuotas: [readingAt(NOW + 500, 40)] }));
  settle({ type: "reset_credit_used", id: null, outcome: "reset" } as Event);
  await flushPromises();
  expect(dialogText()).toContain("Updating usage…");
  expect(meterValues()).toEqual(["60"]);
  act(() => store.setState({ accountQuotas: [readingAt(NOW + 1000)] }));
  expect(dialogText()).toContain("Usage updated.");
  expect(meterValues()).toEqual(["100"]);
});

it("keeps the old values, marked out of date, when the reading after the use fails", async () => {
  const { daemon, store } = fakeDaemon(quotaOf({ available_count: 1, credits: [credit()] }), used("reset"));
  mount(daemon, <UseResetCreditDialog agent="codex" account={null} accountName="Codex Work" onClose={() => {}} />);
  press(submit());
  await flushPromises();
  act(() => store.setState({ accountQuotas: [{ ...quotaOf(undefined), unavailable: { code: "quota_timed_out", params: { agent: "Codex" }, message: "late" } }] }));
  expect(meterValues()).toEqual(["60"]);
  expect(dialogText()).toContain("Usage could not be updated yet.");
});

it("stops waiting for the reading after a minute and leaves the meters as last read", async () => {
  withTimers();
  const { daemon } = fakeDaemon(quotaOf({ available_count: 1, credits: [credit()] }), used("reset"));
  mount(daemon, <UseResetCreditDialog agent="codex" account={null} accountName="Codex Work" onClose={() => {}} />);
  press(submit());
  await flushPromises();
  expect(dialogText()).toContain("Updating usage…");
  act(() => void vi.advanceTimersByTime(READING_WAIT_MS));
  expect(dialogText()).toContain("Usage could not be updated yet.");
  expect(meterValues()).toEqual(["60"]);
});

it("holds the confirm button while the request runs, so a second press sends nothing", async () => {
  let settle!: (event: Event) => void;
  const { daemon, request } = fakeDaemon({ ...quotaOf({ available_count: 1, credits: [credit()] }), account: "codex-work" }, () => new Promise<Event>((resolve) => { settle = resolve; }));
  const onClose = vi.fn();
  mount(daemon, <UseResetCreditDialog agent="codex" account="codex-work" accountName="Codex Work" onClose={onClose} />);
  press(submit());
  await flushPromises();
  expect(submit().textContent).toBe("Use reset");
  expect(submit().getAttribute("aria-busy")).toBe("true");
  press(submit());
  press(document.body.querySelector("[role=alertdialog] button[aria-label=Close]")!);
  expect(request).toHaveBeenCalledOnce();
  expect(request).toHaveBeenCalledWith({ type: "use_reset_credit", agent: "codex", account: "codex-work" });
  expect(onClose).not.toHaveBeenCalled();
  settle({ type: "reset_credit_used", id: null, outcome: "reset" } as Event);
  await flushPromises();
  expect(onClose).not.toHaveBeenCalled();
  expect(footerButtons()).toEqual(["Done"]);
});

it.each([
  ["nothing_to_reset", "does not need a reset right now."],
  ["no_credit", "has no usage limit resets available."],
] as const)("tells the outcome %s in a notice and closes", async (outcome, words) => {
  const { daemon, showNotice } = fakeDaemon(quotaOf({ available_count: 1, credits: [credit()] }), used(outcome));
  const onClose = vi.fn();
  mount(daemon, <UseResetCreditDialog agent="codex" account={null} accountName="Codex Work" onClose={onClose} />);
  press(submit());
  await flushPromises();
  expect(showNotice.mock.calls[0][0]).toContain(words);
  expect(onClose).toHaveBeenCalledOnce();
});

it("stays open when the request fails, for another try", async () => {
  const { daemon } = fakeDaemon(quotaOf({ available_count: 1, credits: [credit()] }), () => Promise.reject(new Error("refused")));
  const onClose = vi.fn();
  mount(daemon, <UseResetCreditDialog agent="codex" account={null} accountName="Codex Work" onClose={onClose} />);
  press(submit());
  await flushPromises();
  expect(onClose).not.toHaveBeenCalled();
  expect(submit().disabled).toBe(false);
});

it("names only the account when Codex gave no details, and holds confirm when none is left", () => {
  const { daemon } = fakeDaemon(quotaOf({ available_count: 2, credits: null }), used("reset"));
  mount(daemon, <UseResetCreditDialog agent="codex" account={null} accountName="Codex Work" onClose={() => {}} />);
  expect(dialogText()).toContain("One usage limit reset will be used on");
  expect(dialogText()).not.toContain("expires");
  expect(submit().disabled).toBe(false);
});

it("holds confirm and says none is left once the reading shows no reset", () => {
  const { daemon } = fakeDaemon(quotaOf(undefined), used("reset"));
  mount(daemon, <UseResetCreditDialog agent="codex" account={null} accountName="Codex Work" onClose={() => {}} />);
  expect(dialogText()).toContain("has no usage limit resets available.");
  expect(submit().disabled).toBe(true);
});

const twoFrames = () => act(() => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => void afterQueuedWork().then(resolve)))));

function ClosesOnPress({ restore }: { restore: () => void }): React.ReactElement {
  const handBack = useResetFocusReturn(restore);
  return <button onClick={handBack}>close</button>;
}

it("hands focus on two frames after the dialog closed, only if it fell to the page", async () => {
  const { daemon } = fakeDaemon(quotaOf(undefined), used("reset"));
  const restore = vi.fn();
  const container = mount(daemon, <ClosesOnPress restore={restore} />);
  press(container.querySelector("button")!);
  (document.activeElement as HTMLElement | null)?.blur();
  await twoFrames();
  expect(restore).toHaveBeenCalledOnce();

  // Focus that is somewhere already is left alone.
  const elsewhere = document.body.appendChild(document.createElement("input"));
  onTestFinished(() => elsewhere.remove());
  press(container.querySelector("button")!);
  elsewhere.focus();
  await twoFrames();
  expect(restore).toHaveBeenCalledOnce();
});

it("hands focus on to nothing once the surface that opened the dialog is gone", async () => {
  const { daemon } = fakeDaemon(quotaOf(undefined), used("reset"));
  const restore = vi.fn();
  const container = mount(daemon, <ClosesOnPress restore={restore} />);
  press(container.querySelector("button")!);
  container.render(null);
  await twoFrames();
  expect(restore).not.toHaveBeenCalled();
});
