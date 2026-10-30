// @vitest-environment jsdom
import { act } from "react";
import { describe, expect, it, vi } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import type { Account, AccountQuota } from "../protocol";
import { accountQuotas, request, settings, toastError } from "./agentAccountsSectionTestState";
import { mount, mountSettings, openDetailsOf, openSettingsOf, rowOf } from "./agentAccountsSectionTestSupport";

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
  email: null, ...patch,
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

  it("shows a turned-off row by its switch and a Disabled badge in its Status cell, with no sentence of its own, no muting and nothing hidden", () => {
    accountQuotas.push(quotaOf("codex", "codex-work", { agent: "codex" } as Partial<AccountQuota>));
    settings.accounts[1]!.disabled = true;
    const { container } = mount("Codex");
    const row = rowOf(container, "codex-work");
    expect(row.textContent).not.toContain("Skipped by automatic account selection");
    expect(row.querySelector("td:nth-child(3)")!.classList.contains("text-app-muted")).toBe(false);
    expect(disableSwitch(row).checked).toBe(true);
    // The usage and the actions stay.
    expect(row.querySelector(".quota-bar-track")).not.toBeNull();
    expect(row.querySelector('button[aria-label^="Actions for"]')).not.toBeNull();
  });

  describe("the Status cell", () => {
    const statusOf = (row: Element) => row.querySelectorAll("td")[2]!;
    const signedOut = (account: string | null) => quotaOf("codex", account, { plan: null, windows: [], unavailable: { code: "quota_not_signed_in", params: { agent: "Codex" }, message: "x" } });
    const badges = (cell: Element) => [...cell.querySelectorAll("span[data-variant]")].map((chip) => chip.textContent);

    it("says Disabled in place of the login state, and no Log in icon, for a turned-off account", () => {
      accountQuotas.push(signedOut("codex-work"));
      settings.accounts[1]!.disabled = true;
      const { container } = mount("Codex");
      const status = statusOf(rowOf(container, "codex-work"));
      expect(badges(status)).toEqual(["Disabled"]);
      expect(status.querySelector('button[aria-label^="Log in"]')).toBeNull();
      // Log in stays in the actions menu, which this cell does not touch.
      expect(rowOf(container, "codex-work").querySelector('button[aria-label^="Actions for"]')).not.toBeNull();
    });

    it("shows the login state and the Log in icon again once the account is turned on", () => {
      accountQuotas.push(signedOut("codex-work"));
      const { container, render } = mount("Codex");
      expect(badges(statusOf(rowOf(container, "codex-work")))).toEqual(["Not logged in"]);
      expect(statusOf(rowOf(container, "codex-work")).querySelector('button[aria-label^="Log in"]')).not.toBeNull();
      settings.accounts[1]!.disabled = true;
      render();
      expect(badges(statusOf(rowOf(container, "codex-work")))).toEqual(["Disabled"]);
      expect(statusOf(rowOf(container, "codex-work")).querySelector('button[aria-label^="Log in"]')).toBeNull();
      settings.accounts[1]!.disabled = false;
      render();
      expect(badges(statusOf(rowOf(container, "codex-work")))).toEqual(["Not logged in"]);
    });

    it("leaves the screen-reader separator out of the Status cell's badge, and keeps it in the details dialog's", async () => {
      accountQuotas.push(quotaOf("codex", "codex-work"), quotaOf("codex", null));
      settings.accounts[1]!.disabled = true;
      const { container } = mount("Codex");
      expect(badges(statusOf(rowOf(container, "codex-work")))).toEqual(["Disabled"]);
      expect(statusOf(rowOf(container, "codex-work")).querySelector(".sr-only")).toBeNull();
      expect(badges(statusOf(rowOf(container, "default:codex")))).toEqual(["Logged in"]);
      expect(statusOf(rowOf(container, "default:codex")).querySelector(".sr-only")).toBeNull();
      const dialog = await openDetailsOf(container, "default:codex");
      const badge = [...dialog.querySelectorAll("span[data-variant]")].find((chip) => chip.textContent?.includes("Logged in"))!;
      expect(badge.previousElementSibling?.classList.contains("sr-only")).toBe(true);
    });

    it("says nothing of Disabled in the details or the settings dialog of a turned-off account", async () => {
      accountQuotas.push(signedOut("codex-work"));
      settings.accounts[1]!.disabled = true;
      const { container } = mount("Codex");
      for (const open of [openDetailsOf, openSettingsOf]) {
        const dialog = await open(container, "codex-work");
        expect(dialog.textContent).not.toMatch(/Disabled/);
      }
    });

    it("says Disabled for the default account of an agent turned off, and leaves an enabled account's badge", () => {
      accountQuotas.push(quotaOf("codex", null), quotaOf("codex", "codex-work"));
      settings.disabled_default_accounts = ["codex"];
      const { container } = mount("Codex");
      expect(badges(statusOf(rowOf(container, "default:codex")))).toEqual(["Disabled"]);
      expect(badges(statusOf(rowOf(container, "codex-work")))).toEqual(["Logged in"]);
    });
  });

  it("turns off the default account of an agent alone", () => {
    settings.disabled_default_accounts = ["claude"];
    const { container } = mount("Claude Code");
    expect(disableSwitch(rowOf(container, "default:claude")).checked).toBe(true);
    expect(disableSwitch(rowOf(container, "claude-work")).checked).toBe(false);
  });

  it("puts the Disable account switch, with its explanation, last in the settings", async () => {
    const { dialog } = await mountSettings("Codex");
    expect(dialog.textContent).toContain("Disable account");
    expect(disableSwitch(dialog).checked).toBe(false);
    expect(dialog.textContent).toContain("Once this is on, this account no longer appears in account choices or usage displays, except while a running session still uses it. New sessions will not use it, and running sessions keep running.");
    expect([...dialog.querySelectorAll('input[role="switch"]')].at(-1)).toBe(disableSwitch(dialog));
    await act(async () => disableSwitch(dialog).click());
    expect(request).toHaveBeenLastCalledWith({ type: "set_account_disabled", agent: "codex", account: "codex-work", disabled: true });
  });

  it("describes a turned-off account's Disable account switch by its state", async () => {
    settings.accounts[1]!.disabled = true;
    const { dialog } = await mountSettings("Codex");
    expect(dialog.textContent).toContain("This account is disabled: it no longer appears in account choices or usage displays");
    expect(dialog.textContent).not.toContain("Once on, this account");
  });

  it("keeps the sharing switch of a turned-off account in its settings", async () => {
    settings.accounts[1]!.disabled = true;
    const { dialog } = await mountSettings("Codex");
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
