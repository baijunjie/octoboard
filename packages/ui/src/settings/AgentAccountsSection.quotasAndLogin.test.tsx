// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import type { Account, AccountQuota, AgentAvailability } from "../protocol";
import { trackMountRefocus } from "../dialogs/dialogTestSupport";
import { accountLogins, accountQuotas, availability, request, toastError } from "./agentAccountsSectionTestState";
import { buttonNamed, mount } from "./agentAccountsSectionTestSupport";

vi.mock("../store", async (importOriginal) => (await import("./agentAccountsSectionTestState")).storeStandIn(await importOriginal()));
vi.mock("../components/AgentIcon", async (importOriginal) => (await import("./agentAccountsSectionTestState")).agentIconStandIn(await importOriginal()));
vi.mock("../components/FadeOverflow", async () => ({ FadeOverflow: (await import("./agentAccountsSectionTestState")).FadeOverflowStandIn }));
vi.mock("../components/MarkedPath", async () => ({ MarkedPath: (await import("./agentAccountsSectionTestState")).MarkedPathStandIn }));
vi.mock("./useSectionRefocus", async (importOriginal) => (await import("./agentAccountsSectionTestState")).sectionRefocusStandIn(await importOriginal()));
vi.mock("../dialogs/OptionSelect", async () => ({ OptionSelect: (await import("./agentAccountsSectionTestState")).OptionSelectStandIn }));

/** Waits out React Aria's focus restore after a dialog closes and what runs in the frames after it. */
const frames = () => act(() => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(resolve, 0)))));

const quotaOf = (agent: Account["agent"], account: string | null, patch: Partial<AccountQuota> = {}): AccountQuota => ({
  agent,
  account,
  fetched_at: Date.now(),
  plan: "plus",
  windows: [{ kind: "session", label: null, used_percent: 18, resets_at: null, window_minutes: 300 }],
  unavailable: null,
  ...patch,
});

it("asks the daemon to refresh the quotas when the section opens", () => {
  expect(mount().opening).toEqual([[{ type: "refresh_account_quotas" }]]);
});

it("still opens when the refresh request fails", async () => {
  request.mockRejectedValueOnce(new Error("not connected"));
  mount();
  await act(async () => {});
  expect(toastError).not.toHaveBeenCalled();
});

it("shows the plan and the quota of the default account and of an owned one, each under its own row", () => {
  accountQuotas.push(quotaOf("codex", null, { plan: "plus" }), quotaOf("codex", "codex-work", { plan: "pro_lite" }));
  const { container } = mount("Codex");
  const rows = [...container.querySelectorAll("[data-list-motion]")].filter((row) => row.querySelector('[data-slot="meter"]'));
  expect(rows).toHaveLength(2);
  expect(rows[0]!.textContent).toContain("Plus");
  expect(rows[1]!.textContent).toContain("Pro Lite");
  expect(rows[1]!.textContent).toContain("5h 82% remaining");
});

it("explains the default account in a help tip beside its name rather than in a line of its own", () => {
  availability.set("claude", { availability: "available", default_account_dir: "/home/dev/.claude" } as AgentAvailability);
  const { container } = mount();
  const row = container.querySelector("[data-list-motion]:not([role=tab])")!;
  const tip = row.querySelector("button")!;
  expect(tip.getAttribute("aria-label")).toBe("About the default account");
  expect(row.textContent).not.toContain("The setup a session runs under");
  expect(row.textContent).toContain("/home/dev/.claude");
  // It ends the name line: after the plan when there is one, straight after the name when there is not.
  const nameLine = tip.parentElement!;
  expect(nameLine.lastElementChild).toBe(tip);
  expect(nameLine.textContent).toBe("Default");
  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    tip.focus();
  });
  expect(document.querySelector('[role="tooltip"]')?.textContent).toBe("The setup a session runs under when no account is pinned.");
});

it("puts the help tip after the plan on the default account's name line when it has one", () => {
  availability.set("claude", { availability: "available", default_account_dir: "/home/dev/.claude" } as AgentAvailability);
  accountQuotas.push(quotaOf("claude", null, { plan: "plus" }));
  const { container } = mount();
  const tip = container.querySelector("[data-list-motion] button")!;
  const nameLine = tip.parentElement!;
  expect(nameLine.lastElementChild).toBe(tip);
  expect(nameLine.textContent).toBe("Default, Plus, Logged in");
  // When the reading was taken shares the directory's line instead.
  expect(nameLine.nextElementSibling?.textContent).toMatch(/^\/home\/dev\/\.claude.*Updated /);
  expect(tip.previousElementSibling?.textContent).toBe("Logged in");
});

it("badges an account known to be logged in, one not logged in yet, and none whose state is not known", () => {
  availability.set("claude", { agent: "claude", availability: "available", default_account_dir: "/home/dev/.claude" } as AgentAvailability);
  accountQuotas.push(quotaOf("claude", null));
  accountQuotas.push(quotaOf("claude", "claude-work", { plan: null, windows: [], unavailable: { code: "quota_not_signed_in", params: { agent: "Claude Code" }, message: "x" } }));
  const { container } = mount();
  const badges = [...container.querySelectorAll("span[data-variant]")].map((chip) => chip.textContent);
  expect(badges).toContain("Logged in");
  expect(badges).toContain("Not logged in");
  expect(container.textContent).not.toContain("Not logged in yet");
  accountQuotas.length = 0;
  accountQuotas.push(quotaOf("claude", null, { unavailable: { code: "quota_timed_out", params: {}, message: "x" } }));
  const failed = mount();
  expect([...failed.container.querySelectorAll("span[data-variant]")].map((chip) => chip.textContent)).not.toContain("Logged in");
});

it("marks a failed reading on the account's name line instead of writing the failure into the row", () => {
  availability.set("claude", { agent: "claude", availability: "available", default_account_dir: "/home/dev/.claude" } as AgentAvailability);
  accountQuotas.push(quotaOf("claude", "claude-work", { plan: null, windows: [], unavailable: { code: "quota_timed_out", params: { agent: "Claude Code" }, message: "x" } }));
  const { container } = mount();
  const mark = container.querySelector<HTMLElement>('[data-list-motion] button[aria-label="Quota unavailable"]')!;
  expect(mark).not.toBeNull();
  expect(mark.parentElement?.textContent).not.toContain("took too long");
  expect(container.textContent).not.toContain("took too long");
});

it("logs in a signed-out account from the button after its badge", async () => {
  availability.set("claude", { agent: "claude", availability: "available", default_account_dir: "/home/dev/.claude" } as AgentAvailability);
  accountQuotas.push(quotaOf("claude", "claude-work", { plan: null, windows: [], unavailable: { code: "quota_not_signed_in", params: { agent: "Claude Code" }, message: "x" } }));
  const { container } = mount();
  const badge = [...container.querySelectorAll("span[data-variant]")].find((chip) => chip.textContent === "Not logged in")!;
  const login = badge.nextElementSibling as HTMLButtonElement;
  expect(login.textContent).toBe("Log in");
  expect(login.dataset.size).toBe("xs");
  await act(async () => login.click());
  expect(request).toHaveBeenLastCalledWith({ type: "start_account_login", agent: "claude", account: "claude-work" });
});

it.each([
  ["before", true], ["after", false],
] as const)("puts focus on the account's actions once a login from its own button succeeds, the button going %s the dialog closes", async (_when, quotaFirst) => {
  availability.set("claude", { agent: "claude", availability: "available", default_account_dir: "/home/dev/.claude" } as AgentAvailability);
  const signedOut = quotaOf("claude", "claude-work", { plan: null, windows: [], unavailable: { code: "quota_not_signed_in", params: { agent: "Claude Code" }, message: "x" } });
  accountQuotas.push(signedOut);
  const { container, render } = mount();
  const row = container.querySelector('[data-list-motion="claude-work"]')!;
  const login = buttonNamed(row, "Log in")!;
  act(() => login.focus());
  await act(async () => login.click());
  const settled = trackMountRefocus();
  accountLogins.push({ agent: "claude", account: "claude-work", status: "waiting", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
  render();
  await settled();
  expect(document.querySelector('[role="dialog"]')).not.toBeNull();
  const signIn = () => accountQuotas.splice(accountQuotas.indexOf(signedOut), 1, quotaOf("claude", "claude-work"));
  if (quotaFirst) signIn();
  accountLogins.splice(0, 1, { agent: "claude", account: "claude-work", status: "succeeded", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
  render();
  // While the button is still there, React Aria's restore may put focus back on it before the hand-off runs.
  if (!quotaFirst) act(() => login.focus());
  await frames();
  if (!quotaFirst) {
    signIn();
    render();
    await frames();
  }
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  expect(buttonNamed(row, "Log in")).toBeUndefined();
  expect(document.activeElement?.getAttribute("aria-label")?.replace(/[\u2068\u2069]/g, "")).toBe("Actions for claude-work (Claude Code)");
  // The success is said after that focus move, not with it.
  expect(row.querySelector('[role="status"]')!.textContent).toBe("");
});

it("leaves focus where React Aria puts it back after a login its own button opened by a press that did not focus it", async () => {
  availability.set("claude", { agent: "claude", availability: "available", default_account_dir: "/home/dev/.claude" } as AgentAvailability);
  const signedOut = quotaOf("claude", "claude-work", { plan: null, windows: [], unavailable: { code: "quota_not_signed_in", params: { agent: "Claude Code" }, message: "x" } });
  accountQuotas.push(signedOut);
  const { container, render } = mount();
  const before = buttonNamed(container, "Add account")!;
  act(() => before.focus());
  await act(async () => buttonNamed(container.querySelector('[data-list-motion="claude-work"]')!, "Log in")!.click());
  const settled = trackMountRefocus();
  accountLogins.push({ agent: "claude", account: "claude-work", status: "waiting", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
  render();
  await settled();
  accountQuotas.splice(accountQuotas.indexOf(signedOut), 1, quotaOf("claude", "claude-work"));
  accountLogins.splice(0, 1, { agent: "claude", account: "claude-work", status: "succeeded", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
  render();
  await frames();
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  expect(document.activeElement).toBe(before);
});

it.each([
  ["its own Log in button", false], ["its actions, the button having gone", true],
] as const)("puts focus on %s once a login the button opened has been cancelled", async (_where, buttonGoes) => {
  availability.set("claude", { agent: "claude", availability: "available", default_account_dir: "/home/dev/.claude" } as AgentAvailability);
  const signedOut = quotaOf("claude", "claude-work", { plan: null, windows: [], unavailable: { code: "quota_not_signed_in", params: { agent: "Claude Code" }, message: "x" } });
  accountQuotas.push(signedOut);
  const { container, render } = mount();
  const row = container.querySelector('[data-list-motion="claude-work"]')!;
  const login = buttonNamed(row, "Log in")!;
  act(() => login.focus());
  await act(async () => login.click());
  const settled = trackMountRefocus();
  accountLogins.push({ agent: "claude", account: "claude-work", status: "starting", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
  render();
  await settled();
  // The button stays disabled until the daemon reports the login cancelled, after the dialog has gone.
  await act(async () => buttonNamed(document.querySelector('[role="dialog"]')!, "Cancel login")!.click());
  await frames();
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  expect(login.disabled).toBe(true);
  if (buttonGoes) accountQuotas.splice(accountQuotas.indexOf(signedOut), 1, quotaOf("claude", "claude-work", { unavailable: { code: "quota_timed_out", params: {}, message: "x" } }));
  accountLogins.splice(0, 1, { agent: "claude", account: "claude-work", status: "cancelled", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
  render();
  await frames();
  if (buttonGoes) expect(document.activeElement?.getAttribute("aria-label")?.replace(/[\u2068\u2069]/g, "")).toBe("Actions for claude-work (Claude Code)");
  else expect(document.activeElement).toBe(login);
});

it("gives focus back to the row's own Log in button once a login cancelled before the daemon had it has ended", async () => {
  availability.set("claude", { agent: "claude", availability: "available", default_account_dir: "/home/dev/.claude" } as AgentAvailability);
  accountQuotas.push(quotaOf("claude", "claude-work", { plan: null, windows: [], unavailable: { code: "quota_not_signed_in", params: { agent: "Claude Code" }, message: "x" } }));
  let registered!: () => void;
  request.mockImplementation(((message: { type: string }) => message.type === "start_account_login"
    ? new Promise((resolve) => { registered = () => resolve({}); })
    : Promise.resolve({})) as never);
  const { container, render } = mount();
  const login = buttonNamed(container.querySelector('[data-list-motion="claude-work"]')!, "Log in")!;
  act(() => login.focus());
  await act(async () => login.click());
  const settled = trackMountRefocus();
  await settled();
  await act(async () => buttonNamed(document.querySelector('[role="dialog"]')!, "Cancel login")!.click());
  await frames();
  expect(document.activeElement).toBe(login);
  // The daemon registers the login after the dialog has gone, and only then is it cancelled. The button turns disabled
  // meanwhile, which in a browser takes focus off it; jsdom leaves it there, so the test takes it off first.
  act(() => login.blur());
  accountLogins.push({ agent: "claude", account: "claude-work", status: "starting", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
  render();
  expect(login.disabled).toBe(true);
  await act(async () => registered());
  expect(request).toHaveBeenLastCalledWith({ type: "cancel_account_login", agent: "claude", account: "claude-work" });
  accountLogins.splice(0, 1, { agent: "claude", account: "claude-work", status: "cancelled", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
  render();
  await frames();
  expect(document.activeElement).toBe(login);
});

it("puts focus on the account's actions when a login its own button opened succeeds after all once cancelled", async () => {
  availability.set("claude", { agent: "claude", availability: "available", default_account_dir: "/home/dev/.claude" } as AgentAvailability);
  accountQuotas.push(quotaOf("claude", "claude-work", { plan: null, windows: [], unavailable: { code: "quota_not_signed_in", params: { agent: "Claude Code" }, message: "x" } }));
  const { container, render } = mount();
  const login = buttonNamed(container.querySelector('[data-list-motion="claude-work"]')!, "Log in")!;
  act(() => login.focus());
  await act(async () => login.click());
  const settled = trackMountRefocus();
  accountLogins.push({ agent: "claude", account: "claude-work", status: "waiting", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
  render();
  await settled();
  await act(async () => buttonNamed(document.querySelector('[role="dialog"]')!, "Cancel login")!.click());
  // The login had already succeeded when the cancel reached the daemon; the button stays until the quota is read again.
  accountLogins.splice(0, 1, { agent: "claude", account: "claude-work", status: "succeeded", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
  render();
  await frames();
  expect(document.activeElement?.getAttribute("aria-label")?.replace(/[\u2068\u2069]/g, "")).toBe("Actions for claude-work (Claude Code)");
});

it("gives focus back to the row's own Log in button when the login it opened is closed without success", async () => {
  availability.set("claude", { agent: "claude", availability: "available", default_account_dir: "/home/dev/.claude" } as AgentAvailability);
  accountQuotas.push(quotaOf("claude", "claude-work", { plan: null, windows: [], unavailable: { code: "quota_not_signed_in", params: { agent: "Claude Code" }, message: "x" } }));
  const { container, render } = mount();
  const login = buttonNamed(container.querySelector('[data-list-motion="claude-work"]')!, "Log in")!;
  act(() => login.focus());
  await act(async () => login.click());
  const settled = trackMountRefocus();
  accountLogins.push({ agent: "claude", account: "claude-work", status: "failed", authorization_url: null, device_code: null, opened_by_agent: false,
    failure: { code: "login_failed", params: { agent: "Claude Code", detail: "Login failed: 400" }, message: "x" } });
  render();
  await settled();
  await act(async () => buttonNamed(document.querySelector('[role="dialog"]')!, "Close")!.click());
  await frames();
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  expect(document.activeElement).toBe(login);
});

it("waits for the first reading of an account of an available agent that has no entry yet", () => {
  availability.set("codex", { agent: "codex", availability: "available" });
  const { container } = mount("Codex");
  expect(container.textContent?.match(/Reading quota/g)).toHaveLength(2);
});

it("leaves an account whose agent is not available without a quota block", () => {
  availability.set("codex", { agent: "codex", availability: "unavailable" });
  const { container } = mount("Codex");
  expect(container.querySelector('[data-slot="meter"]')).toBeNull();
  expect(container.textContent).not.toContain("Reading quota");
});
