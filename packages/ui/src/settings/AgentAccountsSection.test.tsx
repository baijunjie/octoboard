// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import type { Account, AccountQuota, Agent, AgentAvailability, Settings } from "../protocol";
import { AgentAccountsSection } from "./AgentAccountsSection";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const request = vi.fn(() => Promise.resolve({}));
const toastError = vi.fn();
const settings: Settings = {
  auto_pull_repositories: false,
  default_clone_dir: "/home/dev/Projects",
  accounts: [],
};
const accountQuotas: AccountQuota[] = [];
const availability = new Map<Agent, AgentAvailability>();
vi.mock("../store", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../store")>()),
  useDaemon: () => ({ request, toastError }),
  useDaemonStore: (selector: (s: unknown) => unknown) => selector({ settings, agentAvailability: availability, accountQuotas }),
}));
vi.mock("../components/AgentIcon", () => ({ AgentIcon: () => null }));
vi.mock("../components/FadeOverflow", () => ({ FadeOverflow: ({ children }: { children: React.ReactNode }) => children }));
vi.mock("../components/MarkedPath", () => ({ MarkedPath: ({ path }: { path: string }) => path }));
vi.mock("./useSectionRefocus", () => ({ useSectionRefocus: () => {} }));
vi.mock("../dialogs/OptionSelect", () => ({
  OptionSelect: ({ label, value, onChange, options, isDisabled }: {
    label: string;
    value: string;
    onChange: (value: string) => void;
    options: { value: string; label: string; isDisabled?: boolean }[];
    isDisabled?: boolean;
  }) => (
    <select aria-label={label} value={value} onChange={(e) => onChange(e.target.value)} disabled={isDisabled}>
      {options.map((option) => <option key={option.value} value={option.value} disabled={option.isDisabled}>{option.label}</option>)}
    </select>
  ),
}));

const account = (id: string, agent: Account["agent"]): Account => ({ id, agent, name: id, config_dir: `/home/dev/.${id}` });
// jsdom has no Web Animations, and without them the list motion drops a departing row's copy at once, so a
// stub is what lets a test see the motion a removal or a tab switch starts.
const animate = vi.fn(() => ({ cancel: vi.fn(), effect: { getComputedTiming: () => ({ progress: 0 }) }, onfinish: null }));
beforeEach(() => {
  HTMLElement.prototype.animate = animate as unknown as typeof HTMLElement.prototype.animate;
  animate.mockClear();
  request.mockReset().mockResolvedValue({});
  toastError.mockClear();
  settings.accounts = [account("claude-work", "claude"), account("codex-work", "codex"), account("grok-work", "grok")];
  settings.codex_default_approval_reviewer = null;
  settings.claude_default_approval_mode = null;
  accountQuotas.length = 0;
  availability.clear();
});
afterEach(() => {
  delete (HTMLElement.prototype as Partial<HTMLElement>).animate;
});

/** Presses an agent's tab the way a pointer does. */
function selectTab(container: HTMLElement, name: string) {
  const tab = [...container.querySelectorAll<HTMLElement>('[role="tab"]')].find((el) => el.textContent === name)!;
  act(() => {
    for (const type of ["pointerdown", "mousedown", "pointerup", "mouseup", "click"]) {
      tab.dispatchEvent(new MouseEvent(type, { bubbles: true, button: 0 }));
    }
  });
}

function mount(tab?: string) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const render = () => act(() => root.render(<AgentAccountsSection />));
  render();
  // What opening the section asked for is kept apart, so a test sees only the requests it makes.
  const opening = request.mock.calls.slice();
  request.mockClear();
  if (tab) selectTab(container, tab);
  const selects = [...container.querySelectorAll("select")];
  const change = (select: HTMLSelectElement, value: string) => act(() => {
    select.value = value;
    select.dispatchEvent(new Event("change", { bubbles: true }));
  });
  return { container, selects, change, opening, render };
}

it("offers a mode for the Codex default and each Codex account, following configuration by default", () => {
  const { selects } = mount("Codex");
  expect(selects).toHaveLength(2);
  expect(selects.map((select) => select.value)).toEqual(["inherit", "inherit"]);
  expect(selects[1]!.getAttribute("aria-label")).toContain("codex-work");
});

it("shows stored choices separately for default and named accounts", () => {
  settings.codex_default_approval_reviewer = "user";
  settings.accounts[1]!.approval_reviewer = "auto_review";
  expect(mount("Codex").selects.map((select) => select.value)).toEqual(["user", "auto_review"]);
});

it.each([
  [0, "auto_review", null, "auto_review"],
  [1, "user", "codex-work", "user"],
  [1, "inherit", "codex-work", null],
] as const)("saves selector %i as %s without affecting other accounts", async (index, value, account, reviewer) => {
  settings.accounts[1]!.approval_reviewer = "auto_review";
  const { selects, change } = mount("Codex");
  change(selects[index]!, value);
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "set_account_approval_reviewer", account, reviewer });
  await act(async () => {});
});

it("keeps the stored value and reports a failed save", async () => {
  const { selects, change } = mount("Codex");
  request.mockRejectedValueOnce(new Error("save refused"));
  change(selects[0]!, "auto_review");
  await act(async () => {});
  expect(selects[0]!.value).toBe("inherit");
  expect(toastError).toHaveBeenCalledOnce();
  expect(selects[0]!.disabled).toBe(false);
});


it("keeps the trigger available while saving and prevents another option selection until answered", async () => {
  let finish!: (value: object) => void;
  const { selects, change } = mount("Codex");
  request.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  change(selects[0]!, "auto_review");
  expect(selects[0]!.disabled).toBe(false);
  expect([...selects[0]!.options].every((option) => option.disabled)).toBe(true);
  await act(async () => finish({}));
  expect([...selects[0]!.options].every((option) => !option.disabled)).toBe(true);
});

it("offers Claude Code's four choices for its default and each account, following configuration by default", () => {
  const { selects } = mount("Claude Code");
  expect(selects).toHaveLength(2);
  expect(selects.map((select) => select.value)).toEqual(["inherit", "inherit"]);
  expect([...selects[0]!.options].map((option) => option.value)).toEqual(["inherit", "auto", "manual", "bypass"]);
  expect(selects[1]!.getAttribute("aria-label")).toContain("claude-work");
});

it("shows stored Claude Code modes separately for default and named accounts", () => {
  settings.claude_default_approval_mode = "manual";
  settings.accounts[0]!.approval_mode = "auto";
  expect(mount("Claude Code").selects.map((select) => select.value)).toEqual(["manual", "auto"]);
});

it.each([
  [0, "auto", null, "auto"],
  [1, "manual", "claude-work", "manual"],
  [1, "inherit", "claude-work", null],
] as const)("saves Claude Code selector %i as %s at once", async (index, value, account, mode) => {
  settings.accounts[0]!.approval_mode = "auto";
  const { selects, change } = mount("Claude Code");
  change(selects[index]!, value);
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "set_account_approval_mode", account, mode });
  await act(async () => {});
});

it("keeps the confirmation open and the choice unchanged when saving Approve everything fails", async () => {
  const { selects, change } = mount("Claude Code");
  change(selects[1]!, "bypass");
  request.mockRejectedValueOnce(new Error("save refused"));
  act(() => [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Approve everything")!.click());
  await act(async () => {});
  expect(document.body.textContent).toContain("Approve everything for account claude-work?");
  expect(selects[1]!.value).toBe("inherit");
});

it("saves Approve everything only once the warning is confirmed", async () => {
  const { container, selects, change } = mount("Claude Code");
  change(selects[1]!, "bypass");
  expect(request).not.toHaveBeenCalled();
  const buttons = () => [...document.querySelectorAll<HTMLButtonElement>("button")];
  const press = (label: string) => act(() => buttons().find((b) => b.textContent === label)!.click());
  expect(document.body.textContent).toContain("Approve everything for account claude-work?");
  press("Cancel");
  expect(request).not.toHaveBeenCalled();
  expect(document.body.textContent).not.toContain("for account claude-work?");
  change(selects[0]!, "bypass");
  expect(document.body.textContent).toContain("Approve everything for the default account?");
  press("Cancel");
  change(selects[1]!, "bypass");
  press("Approve everything");
  await act(async () => {});
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "set_account_approval_mode", account: "claude-work", mode: "bypass" });
  expect(container.querySelectorAll("select")).toHaveLength(2);
});

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
  expect(rows[1]!.textContent).toContain("5-hour 82% remaining");
});

it("explains the default account in a help tip beside its name rather than in a line of its own", () => {
  availability.set("claude", { availability: "available", default_account_dir: "/home/dev/.claude" } as AgentAvailability);
  const { container } = mount();
  const row = container.querySelector("[data-list-motion]")!;
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
  expect(nameLine.textContent).toBe("DefaultPlus");
  expect(tip.previousElementSibling?.textContent).toBe("Plus");
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

it("shows one agent at a time, opening on the first and switching by tab", () => {
  const { container } = mount();
  const tabs = [...container.querySelectorAll('[role="tab"]')];
  expect(tabs.map((tab) => tab.textContent)).toEqual(["Claude Code", "Codex", "Grok Build"]);
  expect(tabs.map((tab) => tab.getAttribute("aria-selected"))).toEqual(["true", "false", "false"]);
  expect(container.textContent).toContain("claude-work");
  expect(container.textContent).not.toContain("codex-work");
  expect(container.querySelectorAll("select")).toHaveLength(2);
  selectTab(container, "Grok Build");
  expect(container.textContent).toContain("grok-work");
  expect(container.textContent).not.toContain("claude-work");
});

it("still refreshes the quotas of every agent while only one tab is shown", () => {
  expect(mount().opening).toEqual([[{ type: "refresh_account_quotas" }]]);
});

it("names an agent that is not installed inside its tab", () => {
  availability.set("grok", { agent: "grok", availability: "unavailable" });
  const { container } = mount("Grok Build");
  expect(container.querySelector('[role="tabpanel"]')?.textContent).toContain("Grok Build (not installed)");
});

it("replaces a panel on a tab switch without the list motion of rows added or removed", async () => {
  const { container } = mount();
  await act(async () => {});
  selectTab(container, "Codex");
  await act(async () => {});
  expect(container.querySelector("[data-list-motion-ghost]")).toBeNull();
  expect(animate).not.toHaveBeenCalled();
  expect(container.textContent).toContain("codex-work");
});

it("moves the rows of an account removed or added within the panel in view", async () => {
  const { container, render } = mount();
  await act(async () => {});
  const work = settings.accounts[0]!;
  settings.accounts = settings.accounts.filter((a) => a !== work);
  render();
  await act(async () => {});
  const ghosts = container.querySelectorAll("[data-list-motion-ghost]");
  expect(ghosts).toHaveLength(1);
  expect(ghosts[0]!.textContent).toContain("claude-work");
  expect(animate).toHaveBeenCalledOnce();
  settings.accounts = [...settings.accounts, account("claude-home", "claude")];
  render();
  await act(async () => {});
  expect(animate).toHaveBeenCalledTimes(2);
});
