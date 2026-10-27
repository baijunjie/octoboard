// @vitest-environment jsdom
import { act } from "react";
import { describe, expect, it, vi } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import type { Account, AccountQuota } from "../protocol";
import { accountQuotas, request, settings, toastError } from "./agentAccountsSectionTestState";
import { mount, mountDetails, rowOf } from "./agentAccountsSectionTestSupport";

vi.mock("../store", async (importOriginal) => (await import("./agentAccountsSectionTestState")).storeStandIn(await importOriginal()));
vi.mock("../components/AgentIcon", async (importOriginal) => (await import("./agentAccountsSectionTestState")).agentIconStandIn(await importOriginal()));
vi.mock("../components/FadeOverflow", async () => ({ FadeOverflow: (await import("./agentAccountsSectionTestState")).FadeOverflowStandIn }));
vi.mock("../components/MarkedPath", async () => ({ MarkedPath: (await import("./agentAccountsSectionTestState")).MarkedPathStandIn }));
vi.mock("./useSectionRefocus", async (importOriginal) => (await import("./agentAccountsSectionTestState")).sectionRefocusStandIn(await importOriginal()));
vi.mock("../dialogs/OptionSelect", async () => ({ OptionSelect: (await import("./agentAccountsSectionTestState")).OptionSelectStandIn }));

const quotaOf = (agent: Account["agent"], account: string | null, patch: Partial<AccountQuota> = {}): AccountQuota => ({
  agent,
  account,
  fetched_at: Date.now(),
  plan: "max",
  windows: [
    { kind: "session", label: null, used_percent: 18, resets_at: Date.now() + 60 * 60_000, window_minutes: 300 },
    { kind: "weekly", label: null, used_percent: 5, resets_at: Date.now() + 3 * 24 * 60 * 60_000, window_minutes: 10_080 },
    { kind: "weekly_model", label: "Fable", used_percent: 0, resets_at: null, window_minutes: 10_080 },
  ],
  unavailable: null,
  ...patch,
});

const disableSwitch = (scope: ParentNode) => [...scope.querySelectorAll<HTMLInputElement>('input[role="switch"]')].find((input) => input.getAttribute("aria-label")?.startsWith("Disable "))!;

describe("turning an account off", () => {
  it("asks the daemon from the Disable switch of an owned account, and of the default account by agent", async () => {
    const { container } = mount("Codex");
    expect(disableSwitch(rowOf(container, "codex-work")).checked).toBe(false);
    await act(async () => disableSwitch(rowOf(container, "codex-work")).click());
    expect(request).toHaveBeenLastCalledWith({ type: "set_account_disabled", agent: "codex", account: "codex-work", disabled: true });
    await act(async () => disableSwitch(rowOf(container, "default:codex")).click());
    expect(request).toHaveBeenLastCalledWith({ type: "set_account_disabled", agent: "codex", account: null, disabled: true });
  });

  it("names the switch for its account and turns an account back on", async () => {
    settings.accounts[1]!.disabled = true;
    const { container } = mount("Codex");
    const input = disableSwitch(rowOf(container, "codex-work"));
    expect(input.getAttribute("aria-label")?.replace(/[⁨⁩]/g, "")).toBe("Disable codex-work");
    expect(input.checked).toBe(true);
    await act(async () => input.click());
    expect(request).toHaveBeenLastCalledWith({ type: "set_account_disabled", agent: "codex", account: "codex-work", disabled: false });
  });

  it("shows a refusal in an error alert and puts the switch back", async () => {
    const { container } = mount("Codex");
    request.mockRejectedValueOnce(new DaemonRequestError("unknown", "unknown_account", { account: "codex-work" }));
    await act(async () => disableSwitch(rowOf(container, "codex-work")).click());
    await act(async () => {});
    expect(toastError).toHaveBeenCalled();
    expect(disableSwitch(rowOf(container, "codex-work")).checked).toBe(false);
  });

  it("shows a turned-off row only by its switch, with no sentence of its own, no muting and nothing hidden", () => {
    accountQuotas.push(quotaOf("codex", "codex-work", { agent: "codex" } as Partial<AccountQuota>));
    settings.accounts[1]!.disabled = true;
    const { container } = mount("Codex");
    const row = rowOf(container, "codex-work");
    expect(row.textContent).not.toContain("Skipped by automatic account selection");
    expect(row.textContent).not.toContain("Disabled");
    expect(row.querySelector("td:nth-child(3)")!.classList.contains("text-app-muted")).toBe(false);
    expect(disableSwitch(row).checked).toBe(true);
    // The usage and the actions stay.
    expect(row.querySelector(".quota-bar-track")).not.toBeNull();
    expect(row.querySelector('button[aria-label^="Actions for"]')).not.toBeNull();
  });

  it("turns off the default account of an agent alone", () => {
    settings.disabled_default_accounts = ["claude"];
    const { container } = mount("Claude Code");
    expect(disableSwitch(rowOf(container, "default:claude")).checked).toBe(true);
    expect(disableSwitch(rowOf(container, "claude-work")).checked).toBe(false);
  });

  it("puts the Disable account switch, with its explanation, last in the details", async () => {
    const { dialog } = await mountDetails("Codex");
    expect(dialog.textContent).toContain("Disable account");
    expect(disableSwitch(dialog).checked).toBe(false);
    expect(dialog.textContent).toContain("Once this is on, this account no longer appears in account choices or usage displays, and new sessions will not use it. Running sessions keep running.");
    expect([...dialog.querySelectorAll('input[role="switch"]')].at(-1)).toBe(disableSwitch(dialog));
    await act(async () => disableSwitch(dialog).click());
    expect(request).toHaveBeenLastCalledWith({ type: "set_account_disabled", agent: "codex", account: "codex-work", disabled: true });
  });

  it("describes a turned-off account's Disable account switch by its state", async () => {
    settings.accounts[1]!.disabled = true;
    const { dialog } = await mountDetails("Codex");
    expect(dialog.textContent).toContain("This account is disabled: it no longer appears in account choices or usage displays");
    expect(dialog.textContent).not.toContain("Once on, this account");
  });

  it("keeps the sharing switch of a turned-off account in its details", async () => {
    settings.accounts[1]!.disabled = true;
    const { dialog } = await mountDetails("Codex");
    expect(disableSwitch(dialog).checked).toBe(true);
    expect(dialog.querySelector("[data-sharing-account]")).not.toBeNull();
  });
});

it("ignores a second press while the first is unanswered, and follows the recorded choice rather than the press", async () => {
  const { container } = mount("Codex");
  let answer!: () => void;
  request.mockReturnValueOnce(new Promise((resolve) => { answer = () => resolve({}); }));
  const input = disableSwitch(rowOf(container, "codex-work"));
  await act(async () => input.click());
  await act(async () => input.click());
  expect(request).toHaveBeenCalledTimes(1);
  // Nothing moves until the daemon says so.
  expect(disableSwitch(rowOf(container, "codex-work")).checked).toBe(false);
  await act(async () => answer());
  await act(async () => disableSwitch(rowOf(container, "codex-work")).click());
  expect(request).toHaveBeenCalledTimes(2);
});
