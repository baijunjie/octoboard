// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";

import type { Account, AccountQuota, AgentAvailability } from "../protocol";
import { accountQuotas, availability } from "./agentAccountsSectionTestState";
import { mount, rowOf } from "./agentAccountsSectionTestSupport";

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

describe("the accounts table", () => {
  it("names its columns once, in a table of the agent's own, one row per account with the default account first", () => {
    const { container } = mount("Codex");
    const table = container.querySelector("table")!;
    expect(table.getAttribute("aria-label")).toBe("Codex accounts");
    expect([...table.querySelectorAll("thead th")].map((th) => th.textContent)).toEqual(["Account", "Status", "Usage", "Disable", "Actions"]);
    expect([...table.querySelectorAll("tbody tr")].map((row) => row.getAttribute("data-account-row"))).toEqual(["default:codex", "codex-work"]);
    // No row repeats a column's name or its help: the sharing switch is in the details and the approval select above the table.
    expect(table.querySelector("tbody")!.textContent).not.toContain("Permission approval");
    expect(table.querySelector("tbody select")).toBeNull();
    expect(table.querySelector("tbody [data-sharing-account]")).toBeNull();
    expect(table.querySelectorAll('tbody button[aria-label^="About"]')).toHaveLength(0);
  });

  it("draws the usage of an account as the picker's two-layer bar, described in words, with no model window and no countdown", () => {
    availability.set("claude", { agent: "claude", availability: "available", default_account_dir: "/home/dev/.claude" } as AgentAvailability);
    accountQuotas.push(quotaOf("claude", null));
    const { container } = mount();
    const row = rowOf(container, "default:claude");
    expect(row.querySelector('[data-slot="meter"]')).toBeNull();
    expect(row.querySelectorAll(".quota-bar-track")).toHaveLength(2);
    expect([...row.querySelectorAll(".sr-only")].map((note) => note.textContent)).toContain("Session 82% remaining, Weekly · all models 95% remaining");
    expect(row.textContent).not.toContain("resets in");
    expect(row.textContent).not.toContain("Updated");
  });

  it("marks a failed reading, and one out of date, in the usage cell, not by the name", () => {
    const failure = { code: "quota_timed_out", params: {}, message: "x" };
    accountQuotas.push(
      quotaOf("claude", "claude-work", { plan: null, windows: [], unavailable: failure }),
      quotaOf("claude", null, { unavailable: failure }),
    );
    const { container } = mount("Claude Code");
    const usage = (id: string) => rowOf(container, id).querySelectorAll("td")[1]!;
    expect(usage("claude-work").querySelector('button[aria-label="Quota unavailable"]')).not.toBeNull();
    expect(rowOf(container, "claude-work").querySelector("th button")).toBeNull();
    expect(usage("claude-work").textContent).not.toContain("No usage reading");
    expect(usage("default:claude").querySelector('button[aria-label^="Out of date"]')).not.toBeNull();
  });

  it("keeps the table in a sideways scroller with its actions column pinned at the end edge", () => {
    const { container } = mount("Codex");
    const table = container.querySelector("table")!;
    expect(table.parentElement!.className).toContain("scroll-shadow--horizontal");
    const actions = (cells: Element) => cells.lastElementChild!;
    expect(actions(table.querySelector("thead tr")!).className).toContain("sticky");
    expect(actions(table.querySelector("thead tr")!).className).toContain("end-0");
    for (const row of table.querySelectorAll("tbody tr")) {
      expect(actions(row).className).toContain("sticky");
      expect(actions(row).className).toContain("end-0");
      expect(actions(row).querySelector('button[aria-label^="Actions for"]')).not.toBeNull();
    }
  });

  it("shows the Disable switch without its word, named for its account, and says what it means once, in the header", () => {
    const { container } = mount("Codex");
    const row = rowOf(container, "codex-work");
    expect(row.textContent).not.toContain("Disable");
    expect(row.querySelector('input[role="switch"]')!.getAttribute("aria-label")).toContain("Disable ");
    expect(container.querySelectorAll('thead button[aria-label="About disabling an account"]')).toHaveLength(1);
  });
});
