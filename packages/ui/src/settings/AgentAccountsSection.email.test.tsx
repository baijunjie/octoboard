// @vitest-environment jsdom
import { expect, it, vi } from "vitest";

import type { Account, AccountQuota } from "../protocol";
import { accountQuotas } from "./agentAccountsSectionTestState";
import { mount, openDetailsOf, openSettingsOf, rowOf } from "./agentAccountsSectionTestSupport";

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
  plan: "plus",
  windows: [{ kind: "session", label: null, used_percent: 18, resets_at: null, window_minutes: 300 }],
  unavailable: null,
  ...patch,
});

it("shows the email an account is signed in with under its name in its row", () => {
  accountQuotas.push(quotaOf("codex", "codex-work", { email: "me@example.com" }), quotaOf("codex", null));
  const { container } = mount("Codex");
  const header = rowOf(container, "codex-work").querySelector("th")!;
  expect(header.textContent).toMatch(/^codex-work.*me@example\.com/);
  // On a line of its own: the name line, the first thing in the header's wrapper, holds no email, and the email ends the
  // header.
  const nameLine = header.firstElementChild!.firstElementChild!;
  expect(nameLine.textContent).toContain("codex-work");
  expect(nameLine.textContent).not.toContain("@");
  expect(header.textContent).toMatch(/me@example\.com$/);
  expect(rowOf(container, "default:codex").textContent).not.toContain("@");
});

it("shows the email even when the quota windows could not be read, and none for an account that is not signed in", () => {
  accountQuotas.push(
    quotaOf("codex", "codex-work", { email: "me@example.com", windows: [], unavailable: { code: "quota_timed_out", params: { agent: "Codex" }, message: "x" } }),
    quotaOf("codex", null, { email: null, windows: [], unavailable: { code: "quota_not_signed_in", params: { agent: "Codex" }, message: "x" } }),
  );
  const { container } = mount("Codex");
  expect(rowOf(container, "codex-work").textContent).toContain("me@example.com");
  expect(rowOf(container, "default:codex").textContent).not.toContain("@");
});

it("names the account with its email in both the details and the settings dialogs, and nowhere else in the section", async () => {
  accountQuotas.push(quotaOf("codex", "codex-work", { email: "me@example.com" }));
  const { container } = mount("Codex");
  for (const open of [openDetailsOf, openSettingsOf]) {
    const dialog = await open(container, "codex-work");
    const body = dialog.querySelector("[data-slot=modal-body]")!;
    const email = [...body.querySelectorAll("*")].find((node) => node.children.length === 0 && node.textContent === "me@example.com")!;
    expect(email.getAttribute("dir")).toBe("ltr");
    // The name line, with whatever follows the name, comes first and the email stands on a line of its own under it.
    const name = [...body.querySelectorAll("span")].find((node) => node.textContent === "codex-work")!;
    expect(name.parentElement).not.toBe(email.parentElement);
    expect(name.compareDocumentPosition(email) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(name.parentElement!.contains(email)).toBe(false);
  }
  const label = rowOf(container, "codex-work").querySelector("[data-row-actions] button")!.getAttribute("aria-label")!;
  expect(label).not.toContain("@");
});
