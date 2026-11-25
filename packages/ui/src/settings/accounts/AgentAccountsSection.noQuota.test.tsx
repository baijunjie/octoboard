// @vitest-environment jsdom
import { expect, it, vi } from "vitest";

import type { AccountQuota, AgentAvailability } from "../../protocol";
import { press } from "../../components/hint/tooltipTestSupport";
import { account, availability, settings } from "./agentAccountsSectionTestState";
import { menuItem, mount, openDetailsOf, openRowMenu, rowLogin, rowOf, selectTab } from "./agentAccountsSectionTestSupport";

vi.mock("../../components/icons/AgentIcon", async (importOriginal) => (await import("./agentAccountsSectionTestState")).agentIconStandIn(await importOriginal()));
vi.mock("../../components/text/FadeOverflow", async () => ({ FadeOverflow: (await import("./agentAccountsSectionTestState")).FadeOverflowStandIn }));
vi.mock("../../components/text/MarkedPath", async () => ({ MarkedPath: (await import("./agentAccountsSectionTestState")).MarkedPathStandIn }));
vi.mock("../useSectionRefocus", async (importOriginal) => (await import("./agentAccountsSectionTestState")).sectionRefocusStandIn(await importOriginal()));
vi.mock("../../components/form/OptionSelect", async () => ({ OptionSelect: (await import("./agentAccountsSectionTestState")).OptionSelectStandIn }));

// The test-only provider declares no plan quota and no login flow, and has a sign-in hint of its own.
const SIGNED_OUT: AccountQuota = {
  agent: "test_agent",
  account: "test-work",
  fetched_at: Date.now(),
  plan: null,
  windows: [],
  unavailable: { code: "quota_not_signed_in", params: { agent: "Test Agent" }, message: "x" },
  email: null,
};
const SIGNED_IN: AccountQuota = { ...SIGNED_OUT, account: null, unavailable: null, email: "dev@example.com" };

function mountTestAgent() {
  settings.added_agents = ["claude", "test_agent"];
  settings.accounts = [account("test-work", "test_agent")];
  // Available, which is when an account of a provider that has a plan quota waits for its first reading.
  availability.set("test_agent", { agent: "test_agent", availability: "available", default_account_dir: "/home/dev/.test_agent" } as AgentAvailability);
  return mount("Test Agent");
}

it("has no Usage column for a provider with no plan quota, whether or not a reading is held, and keeps the others' columns", () => {
  const { container, set } = mountTestAgent();
  const headers = () => [...container.querySelectorAll('[role="tabpanel"]:not([hidden]) thead th')].map((th) => th.textContent);
  // Nothing held yet: not "Reading usage…", and not a dash that says a reading is missing.
  expect(container.textContent).not.toContain("Reading usage");
  expect(headers()).toEqual(["", "Account", "Status", "Disable", "Actions"]);
  set({ accountQuotas: [SIGNED_OUT, SIGNED_IN] });
  expect(headers()).toEqual(["", "Account", "Status", "Disable", "Actions"]);
  for (const id of ["test-work", "default:test_agent"]) {
    const row = rowOf(container, id);
    expect(row.querySelectorAll("td, th")).toHaveLength(5);
    expect(row.querySelector(".quota-bar-track")).toBeNull();
    expect(row.textContent).not.toContain("No usage reading");
  }
  selectTab(container, "Claude Code");
  expect(headers()).toContain("Usage");
});

it("still waits for the first reading of an account of a provider that has a plan quota", () => {
  settings.added_agents = ["claude", "test_agent"];
  availability.set("claude", { agent: "claude", availability: "available", default_account_dir: "/home/dev/.claude" } as AgentAvailability);
  const { container } = mount("Claude Code");
  expect(rowOf(container, "default:claude").textContent).toContain("Reading usage");
});

it("shows a Not logged in badge and the provider's own hint, and no Log in, for a signed-out account of a provider with no login flow", () => {
  const { container, set } = mountTestAgent();
  set({ accountQuotas: [SIGNED_OUT] });
  const row = rowOf(container, "test-work");
  expect([...row.querySelectorAll("span[data-variant]")].map((chip) => chip.textContent)).toContain("Not logged in");
  expect(rowLogin(row)).toBeUndefined();
  const tip = row.querySelector<HTMLButtonElement>('button[aria-label="About logging in to ⁨test-work⁩"]')!;
  expect(tip).not.toBeNull();
  press(tip);
  const hint = document.querySelector('[role="dialog"][aria-label^="About logging in"]')!;
  expect(hint.textContent).toBe("To log in, run test-agent login in a terminal, with /home/dev/.test-work as its configuration directory.");
  expect([...hint.querySelectorAll("[data-inline-reference]")].map((chip) => chip.textContent)).toEqual(["test-agent login", "/home/dev/.test-work"]);
});

it("keeps the Log in icon, and offers no hint, for a signed-out account of a provider with a login flow", () => {
  const { container, set } = mountTestAgent();
  set({ accountQuotas: [{ ...SIGNED_OUT, agent: "claude", account: null }] });
  selectTab(container, "Claude Code");
  const row = rowOf(container, "default:claude");
  expect(rowLogin(row)).toBeDefined();
  expect(row.querySelector('button[aria-label^="About logging in"]')).toBeNull();
});

it("leaves Log in out of the actions menu of an account of a provider with no login flow", async () => {
  const { container, set } = mountTestAgent();
  set({ accountQuotas: [SIGNED_OUT] });
  await openRowMenu(container, "test-work");
  expect(menuItem("Details")).toBeDefined();
  expect([...document.querySelectorAll("[role=menuitem]")].map((entry) => entry.textContent)).not.toContain("Log in");
});

it("shows the email and a Logged in badge for a signed-in account, with no usage and no hint", () => {
  const { container, set } = mountTestAgent();
  set({ accountQuotas: [SIGNED_IN] });
  const row = rowOf(container, "default:test_agent");
  expect(row.textContent).toContain("dev@example.com");
  expect([...row.querySelectorAll("span[data-variant]")].map((chip) => chip.textContent)).toContain("Logged in");
  expect(row.querySelector('button[aria-label^="About logging in"]')).toBeNull();
});

it("shows no login state while the sign-in reading cannot tell", () => {
  const { container, set } = mountTestAgent();
  set({ accountQuotas: [{ ...SIGNED_OUT, unavailable: null }] });
  const row = rowOf(container, "test-work");
  const badges = [...row.querySelectorAll("span[data-variant]")].map((chip) => chip.textContent);
  expect(badges).not.toContain("Logged in");
  expect(badges).not.toContain("Not logged in");
});

it("has no Usage part in the details of an account of a provider with no plan quota, and says how to log in while it is signed out", async () => {
  const { container, set } = mountTestAgent();
  set({ accountQuotas: [SIGNED_OUT] });
  const dialog = await openDetailsOf(container, "test-work");
  expect(dialog.textContent).toContain("Not logged in");
  expect(dialog.textContent).toContain("To log in, run test-agent login in a terminal, with /home/dev/.test-work as its configuration directory.");
  expect(dialog.textContent).not.toContain("Usage");
  expect(dialog.textContent).not.toContain("Reading usage");
  expect(rowLogin(dialog)).toBeUndefined();
});

it("shows a signed-in account's email in the details with no Usage part and no hint", async () => {
  const { container, set } = mountTestAgent();
  set({ accountQuotas: [SIGNED_IN] });
  const dialog = await openDetailsOf(container, "default:test_agent");
  expect(dialog.textContent).toContain("Logged in");
  expect(dialog.textContent).toContain("dev@example.com");
  expect(dialog.textContent).not.toContain("Usage");
  expect(dialog.textContent).not.toContain("To log in");
});

it("words a failed reading of a provider with no plan quota nowhere as usage, in the row or in the details", async () => {
  const { container, set } = mountTestAgent();
  set({ accountQuotas: [{ ...SIGNED_OUT, unavailable: { code: "quota_shell_environment_failed", params: {}, message: "x" } }] });
  expect(rowOf(container, "test-work").querySelector('[aria-label="Usage unavailable"]')).toBeNull();
  expect(rowOf(container, "test-work").textContent).not.toContain("Usage unavailable");
  const dialog = await openDetailsOf(container, "test-work");
  expect(dialog.querySelector('[aria-label="Usage unavailable"]')).toBeNull();
  expect(dialog.textContent).not.toContain("Usage");
  expect(dialog.textContent).not.toContain("Not logged in");
});

it("keeps the Usage part in the details of an account of a provider that has a plan quota", async () => {
  settings.added_agents = ["claude", "test_agent"];
  const { container } = mount();
  const dialog = await openDetailsOf(container, "default:claude");
  expect(dialog.textContent).toContain("Usage");
});

const credentialFields = (root: ParentNode) => root.querySelectorAll('input:not([type="checkbox"]):not([type="radio"]), textarea, [contenteditable="true"]');

it("offers no field for a key or any other credential in the row, or in the hint, of a signed-out account of a provider with no login flow", () => {
  const { container, set } = mountTestAgent();
  set({ accountQuotas: [SIGNED_OUT] });
  const row = rowOf(container, "test-work");
  expect(row).not.toBeNull();
  expect(credentialFields(row)).toHaveLength(0);
  press(row.querySelector<HTMLButtonElement>('button[aria-label="About logging in to ⁨test-work⁩"]')!);
  const hint = document.querySelector('[role="dialog"][aria-label^="About logging in"]')!;
  expect(hint).not.toBeNull();
  expect(credentialFields(hint)).toHaveLength(0);
});

it("offers no field for a key or any other credential in the details of a signed-out account of a provider with no login flow", async () => {
  const { container, set } = mountTestAgent();
  set({ accountQuotas: [SIGNED_OUT] });
  const dialog = await openDetailsOf(container, "test-work");
  expect(dialog.textContent).toContain("To log in");
  expect(credentialFields(dialog)).toHaveLength(0);
});
