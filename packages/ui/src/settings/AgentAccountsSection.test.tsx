// @vitest-environment jsdom
import { act, StrictMode, useSyncExternalStore } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { afterEach, beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";

import type { Account, AccountLogin, AccountQuota, Agent, AgentAvailability, Console, Project, Session, Settings } from "../protocol";
import { DaemonRequestError } from "../daemon-client";
import { errorAlerts } from "../errorAlerts";
import { ErrorAlerts } from "../dialogs/ErrorAlerts";
import { trackMountRefocus } from "../dialogs/dialogTestSupport";
import { SettingsDialog } from "./SettingsDialog";
import { registerSettingsLauncher } from "./settingsLauncher";
import { AccountsNavigationContext } from "./AccountsNavigationContext";
import { AgentAccountsSection } from "./AgentAccountsSection";

const request = vi.fn(() => Promise.resolve({}));
const toastError = vi.fn();
const settings: Settings = {
  auto_pull_repositories: false,
  auto_delete_unresumable_archives: true,
  added_agents: ["claude", "codex", "grok"], default_clone_dir: "/home/dev/Projects",
  accounts: [],
};
const accountQuotas: AccountQuota[] = [];
const accountLogins: AccountLogin[] = [];
const availability = new Map<Agent, AgentAvailability>();
const consoles = new Map<string, Console>();
const projects = new Map<string, Project>();
const sessions = new Map<string, Session>();
const dismissAccountLogin = vi.fn((_store: unknown, login: AccountLogin) => {
  const at = accountLogins.indexOf(login);
  if (at >= 0) accountLogins.splice(at, 1);
});
const trackAccountLogin = vi.fn();
// The store is not reactive here; a test that changes it calls `bus.notify`, which re-renders whatever reads it.
const bus = vi.hoisted(() => {
  let version = 0;
  const listeners = new Set<() => void>();
  return {
    subscribe: (listener: () => void) => (listeners.add(listener), () => listeners.delete(listener)),
    version: () => version,
    notify: () => {
      version += 1;
      listeners.forEach((listener) => listener());
    },
  };
});
const state = () => ({ settings, agentAvailability: availability, accountQuotas, accountLogins, consoles, projects, sessions });
vi.mock("../store", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../store")>()),
  useDaemon: () => ({ request, toastError, store: { getState: () => state() } }),
  useDaemonStore: (selector: (s: unknown) => unknown) => {
    useSyncExternalStore(bus.subscribe, bus.version);
    return selector(state());
  },
  useDaemonStoreEqual: (selector: (s: unknown) => unknown) => selector(state()),
  dismissAccountLogin: (...args: Parameters<typeof dismissAccountLogin>) => dismissAccountLogin(...args),
  trackAccountLogin: (...args: unknown[]) => trackAccountLogin(...args),
}));
vi.mock("../components/AgentIcon", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../components/AgentIcon")>()),
  AgentIcon: () => null,
}));
vi.mock("../components/FadeOverflow", () => ({ FadeOverflow: ({ children }: { children: React.ReactNode }) => children }));
vi.mock("../components/MarkedPath", () => ({ MarkedPath: ({ path }: { path: string }) => path }));
// What a section does when focus is lost, with the Settings dialog's section entry stood in for by a button.
const sectionEntry = vi.hoisted(() => ({ current: null as HTMLElement | null }));
vi.mock("./useSectionRefocus", async (importOriginal) => {
  const { useEffect } = await import("react");
  return {
    ...(await importOriginal<typeof import("./useSectionRefocus")>()),
    useSectionRefocus: (deps: unknown[]) => useEffect(() => {
      if (document.activeElement === document.body) sectionEntry.current?.focus();
    }, deps),
  };
});
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
  const globals = globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean };
  const actEnvironment = globals.IS_REACT_ACT_ENVIRONMENT;
  const modality = getInteractionModality();
  globals.IS_REACT_ACT_ENVIRONMENT = true;
  onTestFinished(() => {
    // The getter can return the initial null state, which the setter accepts at runtime.
    act(() => Reflect.apply(setInteractionModality, undefined, [modality]));
    if (actEnvironment === undefined) delete globals.IS_REACT_ACT_ENVIRONMENT;
    else globals.IS_REACT_ACT_ENVIRONMENT = actEnvironment;
  });
  HTMLElement.prototype.animate = animate as unknown as typeof HTMLElement.prototype.animate;
  animate.mockClear();
  request.mockReset().mockResolvedValue({});
  toastError.mockClear();
  dismissAccountLogin.mockClear();
  trackAccountLogin.mockClear();
  settings.added_agents = ["claude", "codex", "grok"];
  consoles.clear();
  projects.clear();
  sessions.clear();
  settings.accounts = [account("claude-work", "claude"), account("codex-work", "codex"), account("grok-work", "grok")];
  settings.codex_default_approval_reviewer = null;
  settings.claude_default_approval_mode = null;
  accountQuotas.length = 0;
  accountLogins.length = 0;
  settings.default_account_names = undefined;
  availability.clear();
});
afterEach(() => {
  act(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); });
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

function mount(tab?: string, strict = false) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const entry = document.body.appendChild(document.createElement("button"));
  sectionEntry.current = entry;
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    entry.remove();
    sectionEntry.current = null;
  });
  let navigation = { pending: false, dismissForms: 0, consume: () => {} };
  const render = () => act(() => {
    const section = <AccountsNavigationContext.Provider value={navigation}><AgentAccountsSection /></AccountsNavigationContext.Provider>;
    root.render(strict ? <StrictMode>{section}</StrictMode> : section);
    bus.notify();
  });
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
  const navigate = (addAgent: boolean) => {
    navigation = { ...navigation, pending: addAgent, dismissForms: navigation.dismissForms + 1 };
    render();
  };
  return { container, selects, change, opening, render, navigate };
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
  expect(nameLine.textContent).toBe("DefaultPlusLogged in");
  expect(tip.previousElementSibling?.textContent).toBe("Logged in");
});

it("badges an account known to be logged in, one not logged in yet, and none whose state is not known", () => {
  availability.set("claude", { agent: "claude", availability: "available", default_account_dir: "/home/dev/.claude" } as AgentAvailability);
  accountQuotas.push(quotaOf("claude", null));
  accountQuotas.push(quotaOf("claude", "claude-work", { plan: null, windows: [], unavailable: { code: "quota_not_signed_in", params: { agent: "Claude Code" }, message: "x" } }));
  const { container } = mount();
  const badges = [...container.querySelectorAll("[data-inline-chip]")].map((chip) => chip.textContent);
  expect(badges).toContain("Logged in");
  expect(badges).toContain("Not logged in");
  expect(container.textContent).not.toContain("Not logged in yet");
  accountQuotas.length = 0;
  accountQuotas.push(quotaOf("claude", null, { unavailable: { code: "quota_timed_out", params: {}, message: "x" } }));
  const failed = mount();
  expect([...failed.container.querySelectorAll("[data-inline-chip]")].map((chip) => chip.textContent)).not.toContain("Logged in");
});

it("logs in a signed-out account from the button after its badge", async () => {
  availability.set("claude", { agent: "claude", availability: "available", default_account_dir: "/home/dev/.claude" } as AgentAvailability);
  accountQuotas.push(quotaOf("claude", "claude-work", { plan: null, windows: [], unavailable: { code: "quota_not_signed_in", params: { agent: "Claude Code" }, message: "x" } }));
  const { container } = mount();
  const badge = [...container.querySelectorAll("[data-inline-chip]")].find((chip) => chip.textContent === "Not logged in")!;
  const login = badge.nextElementSibling as HTMLButtonElement;
  expect(login.textContent).toBe("Log in");
  expect(login.dataset.size).toBe("xs");
  await act(async () => login.click());
  expect(request).toHaveBeenLastCalledWith({ type: "start_account_login", agent: "claude", account: "claude-work" });
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

it("starts a new account on the agent of the tab in view", () => {
  const { container } = mount("Codex");
  const add = [...container.querySelectorAll("button")].find((button) => button.textContent === "Add account")!;
  act(() => add.click());
  expect(document.body.querySelector<HTMLSelectElement>('select[aria-label="Agent"]')!.value).toBe("codex");
});

it("still refreshes the quotas of every agent while only one tab is shown", () => {
  expect(mount().opening).toEqual([[{ type: "refresh_account_quotas" }]]);
});

it("marks an agent that is not installed in its tab, not in its panel", () => {
  availability.set("grok", { agent: "grok", availability: "unavailable" });
  const { container } = mount("Grok Build");
  const tabs = [...container.querySelectorAll('[role="tab"]')];
  expect(tabs.map((tab) => tab.getAttribute("aria-label"))).toEqual([null, null, "Grok Build (not installed)"]);
  expect(tabs.map((tab) => tab.querySelector("svg.lucide-triangle-alert") !== null)).toEqual([false, false, true]);
  expect(container.querySelector('[role="tabpanel"]')?.textContent).not.toContain("not installed");
});

it("marks no tab while an agent's availability is not determined", () => {
  availability.set("codex", { agent: "codex", availability: "not_determined" });
  const { container } = mount();
  expect(container.querySelector("svg.lucide-triangle-alert")).toBeNull();
  expect(container.querySelector('[role="tab"][aria-label]')).toBeNull();
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

function notInstalledTab(container: HTMLElement) {
  return [...container.querySelectorAll<HTMLElement>('[role="tab"]')].find((el) => el.getAttribute("aria-label") === "Codex (not installed)")!;
}

it("shows what the not-installed icon means in a tooltip when its tab takes keyboard focus", () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  availability.set("codex", { agent: "codex", availability: "unavailable" });
  const { container } = mount();
  const tab = notInstalledTab(container);
  act(() => setInteractionModality("keyboard"));
  act(() => tab.focus());
  act(() => void vi.advanceTimersByTime(70));
  expect(document.querySelector("[role=tooltip]")?.textContent).toContain("Codex is not installed");
  act(() => tab.blur());
  expect(document.querySelector("[role=tooltip]")).toBeNull();
});

it("does not show the not-installed tooltip for a tab focused by a press", () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  availability.set("codex", { agent: "codex", availability: "unavailable" });
  const { container } = mount();
  const tab = notInstalledTab(container);
  act(() => setInteractionModality("pointer"));
  act(() => tab.focus());
  act(() => void vi.advanceTimersByTime(200));
  expect(document.querySelector("[role=tooltip]")).toBeNull();
  act(() => setInteractionModality("keyboard"));
  act(() => void vi.advanceTimersByTime(200));
  expect(document.querySelector("[role=tooltip]")).toBeNull();
});

/** Moves a mouse pointer onto or off a tab; React derives its enter and leave from over and out. */
function pointTo(tab: HTMLElement, on: boolean) {
  const event = new MouseEvent(on ? "pointerover" : "pointerout", { bubbles: true, relatedTarget: on ? document.body : null });
  Object.defineProperty(event, "pointerType", { value: "mouse" });
  act(() => void tab.dispatchEvent(event));
}

it("opens the not-installed tooltip again as keyboard focus arrives after a key closed the one the pointer opened", () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  availability.set("codex", { agent: "codex", availability: "unavailable" });
  const { container } = mount();
  const tab = notInstalledTab(container);
  pointTo(tab, true);
  act(() => void vi.runOnlyPendingTimers());
  expect(document.querySelector("[role=tooltip]")).not.toBeNull();
  act(() => void document.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true })));
  expect(document.querySelector("[role=tooltip]")).toBeNull();
  act(() => setInteractionModality("keyboard"));
  act(() => tab.focus());
  act(() => void vi.advanceTimersByTime(70));
  expect(document.querySelector("[role=tooltip]")).not.toBeNull();
});

it("keeps the not-installed tooltip closed on a blur while the pointer stays on the tab after a key closed it", () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  availability.set("codex", { agent: "codex", availability: "unavailable" });
  const { container } = mount();
  const tab = notInstalledTab(container);
  pointTo(tab, true);
  act(() => setInteractionModality("keyboard"));
  act(() => tab.focus());
  act(() => void vi.advanceTimersByTime(70));
  expect(document.querySelector("[role=tooltip]")).not.toBeNull();
  act(() => void document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(document.querySelector("[role=tooltip]")).toBeNull();
  act(() => tab.blur());
  act(() => void vi.runOnlyPendingTimers());
  expect(document.querySelector("[role=tooltip]")).toBeNull();
});

it("holds the not-installed tooltip back while the scroll that reveals a focused tab goes on, and opens it once that stops", () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  availability.set("codex", { agent: "codex", availability: "unavailable" });
  const { container } = mount();
  const tab = notInstalledTab(container);
  const scroll = () => act(() => void tab.parentElement!.dispatchEvent(new Event("scroll")));
  act(() => setInteractionModality("keyboard"));
  act(() => tab.focus());
  expect(document.querySelector("[role=tooltip]")).toBeNull();
  act(() => void vi.advanceTimersByTime(50));
  scroll();
  act(() => void vi.advanceTimersByTime(90));
  scroll();
  act(() => void vi.advanceTimersByTime(90));
  expect(document.querySelector("[role=tooltip]")).toBeNull();
  act(() => void vi.advanceTimersByTime(20));
  expect(document.querySelector("[role=tooltip]")).not.toBeNull();
});

it("opens the not-installed tooltip within a moment of keyboard focus when nothing scrolls", () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  availability.set("codex", { agent: "codex", availability: "unavailable" });
  const { container } = mount();
  act(() => setInteractionModality("keyboard"));
  act(() => notInstalledTab(container).focus());
  act(() => void vi.advanceTimersByTime(70));
  expect(document.querySelector("[role=tooltip]")).not.toBeNull();
});

it("cancels the not-installed tooltip's pending open when a key or a press comes before the scroll has settled", () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  availability.set("codex", { agent: "codex", availability: "unavailable" });
  const { container } = mount();
  const tab = notInstalledTab(container);
  act(() => setInteractionModality("keyboard"));
  act(() => tab.focus());
  act(() => void document.dispatchEvent(new KeyboardEvent("keydown", { key: "a", bubbles: true })));
  act(() => void vi.advanceTimersByTime(300));
  expect(document.querySelector("[role=tooltip]")).toBeNull();
  act(() => tab.blur());
  act(() => setInteractionModality("keyboard"));
  act(() => tab.focus());
  act(() => void document.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true })));
  act(() => void vi.advanceTimersByTime(300));
  expect(document.querySelector("[role=tooltip]")).toBeNull();
});

it("keeps a cancelled not-installed tooltip away when the pointer crosses the tab while it keeps focus", () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  availability.set("codex", { agent: "codex", availability: "unavailable" });
  const { container } = mount();
  const tab = notInstalledTab(container);
  act(() => setInteractionModality("keyboard"));
  act(() => tab.focus());
  act(() => void document.dispatchEvent(new KeyboardEvent("keydown", { key: "a", bubbles: true })));
  pointTo(tab, true);
  pointTo(tab, false);
  act(() => void vi.advanceTimersByTime(300));
  expect(document.querySelector("[role=tooltip]")).toBeNull();
});

it("shows the not-installed tooltip on hover again once a cancelled one's tab has lost focus", () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  availability.set("codex", { agent: "codex", availability: "unavailable" });
  const { container } = mount();
  const tab = notInstalledTab(container);
  act(() => setInteractionModality("keyboard"));
  act(() => tab.focus());
  act(() => void document.dispatchEvent(new KeyboardEvent("keydown", { key: "a", bubbles: true })));
  act(() => tab.blur());
  pointTo(tab, true);
  act(() => void vi.advanceTimersByTime(3000));
  expect(document.querySelector("[role=tooltip]")).not.toBeNull();
});

const tabNames = (container: HTMLElement) => [...container.querySelectorAll('[role="tab"]')].map((tab) => tab.textContent);
const buttonNamed = (container: ParentNode, name: string) => [...container.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === name || b.getAttribute("aria-label") === name);

it("shows only the added agents, in the list's order", () => {
  settings.added_agents = ["grok", "claude"];
  const { container } = mount();
  expect(tabNames(container)).toEqual(["Grok Build", "Claude Code"]);
  expect(container.textContent).toContain("grok-work");
  expect(container.textContent).not.toContain("codex-work");
});

it("shows every agent while the list is undecided, with no menu to remove or reorder them", async () => {
  settings.added_agents = null;
  const { container } = mount();
  expect(tabNames(container)).toEqual(["Claude Code", "Codex", "Grok Build"]);
  await act(async () => {
    container.querySelector('[role="tab"]')!.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
  });
  expect(document.querySelector("[role=menu]")).toBeNull();
});

it("shows an empty state with Add agent, and no strip or Add account, when no agent is added", () => {
  settings.added_agents = [];
  const { container } = mount();
  expect(container.querySelector('[role="tablist"]')).toBeNull();
  expect(container.textContent).toContain("No agent added");
  expect(buttonNamed(container, "Add account")!.disabled).toBe(true);
  expect(buttonNamed(container, "Add agent")!.disabled).toBe(false);
});

it("opens the Add agent dialog from its button", () => {
  const { container } = mount();
  act(() => buttonNamed(container, "Add agent")!.click());
  expect(document.querySelector('[role="dialog"]')?.textContent).toContain("Only an agent installed on this machine can be added.");
});

it("opens the Add agent dialog once when Settings asked for it, and takes the request up", () => {
  const consume = vi.fn();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const render = (pending: boolean) => act(() => {
    root.render(<AccountsNavigationContext.Provider value={{ pending, consume, dismissForms: 0 }}><AgentAccountsSection /></AccountsNavigationContext.Provider>);
    bus.notify();
  });
  render(true);
  expect(consume).toHaveBeenCalledOnce();
  expect(document.querySelector('[role="dialog"]')?.textContent).toContain("Only an agent installed on this machine can be added.");
  // Taken up, the request is gone: the section returning later (another section shown meanwhile) does not reopen it.
  act(() => root.unmount());
  const second = document.body.appendChild(document.createElement("div"));
  const secondRoot = createRoot(second);
  onTestFinished(() => {
    act(() => secondRoot.unmount());
    second.remove();
  });
  act(() => {
    secondRoot.render(<AccountsNavigationContext.Provider value={{ pending: false, consume, dismissForms: 0 }}><AgentAccountsSection /></AccountsNavigationContext.Provider>);
    bus.notify();
  });
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  expect(consume).toHaveBeenCalledOnce();
});

it("closes its Add agent dialog when Settings asks for anything else", () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const render = (value: { pending: boolean; dismissForms: number }) => act(() => {
    root.render(<AccountsNavigationContext.Provider value={{ ...value, consume: () => {} }}><AgentAccountsSection /></AccountsNavigationContext.Provider>);
    bus.notify();
  });
  render({ pending: true, dismissForms: 0 });
  expect(document.querySelector('[role="dialog"]')).not.toBeNull();
  render({ pending: false, dismissForms: 1 });
  expect(document.querySelector('[role="dialog"]')).toBeNull();
});

it("flags only an added agent that is not installed", () => {
  settings.added_agents = ["claude", "grok"];
  availability.set("codex", { agent: "codex", availability: "unavailable" });
  availability.set("grok", { agent: "grok", availability: "unavailable" });
  const { container } = mount();
  expect(tabNames(container)).toEqual(["Claude Code", "Grok Build"]);
  expect([...container.querySelectorAll('[role="tab"]')].map((tab) => tab.getAttribute("aria-label"))).toEqual([null, "Grok Build (not installed)"]);
});

async function openTabMenu(container: HTMLElement, name: string) {
  const tab = [...container.querySelectorAll<HTMLElement>('[role="tab"]')].find((el) => el.textContent === name)!;
  await act(async () => {
    tab.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 10, clientY: 10 }));
  });
}
const menuItem = (name: string) => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((entry) => entry.textContent === name)!;
const chooseItem = (name: string) => act(async () => {
  const entry = menuItem(name);
  entry.focus();
  entry.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  entry.dispatchEvent(new KeyboardEvent("keyup", { key: "Enter", bubbles: true }));
});

it("opens an agent tab's menu with the Menu key and Shift+F10 as well as a right-click", async () => {
  const { container } = mount();
  const tab = container.querySelector<HTMLElement>('[role="tab"]')!;
  tab.focus();
  await act(async () => {
    tab.dispatchEvent(new KeyboardEvent("keydown", { key: "F10", shiftKey: true, bubbles: true, cancelable: true }));
  });
  expect([...document.querySelectorAll("[role=menuitem]")].map((entry) => entry.textContent)).toEqual(["Move earlier", "Move later", "Remove agent"]);
});

it("moves a tab a step with the menu, sending the whole new order, and disables the move past either end", async () => {
  settings.added_agents = ["claude", "codex", "grok"];
  const { container } = mount();
  await openTabMenu(container, "Claude Code");
  expect(menuItem("Move earlier").getAttribute("aria-disabled")).toBe("true");
  expect(menuItem("Move later").getAttribute("aria-disabled")).not.toBe("true");
  await chooseItem("Move later");
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "reorder_agents", agents: ["codex", "claude", "grok"] });
});

it("moves the last tab left, and disables moving it right", async () => {
  const { container } = mount();
  await openTabMenu(container, "Grok Build");
  expect(menuItem("Move later").getAttribute("aria-disabled")).toBe("true");
  await chooseItem("Move earlier");
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "reorder_agents", agents: ["claude", "grok", "codex"] });
});

it("reports a refused reorder", async () => {
  const { container } = mount();
  request.mockRejectedValueOnce(new Error("order refused"));
  await openTabMenu(container, "Codex");
  await chooseItem("Move earlier");
  await act(async () => {});
  expect(toastError).toHaveBeenCalledOnce();
});

it("offers the same entries in a visible more button for the agent of the tab in view", async () => {
  const { container } = mount();
  const more = (agent: string) => container.querySelector<HTMLElement>(`button[aria-label="More actions for ${agent}"]`);
  expect(more("Claude Code")).not.toBeNull();
  selectTab(container, "Codex");
  expect(more("Claude Code")).toBeNull();
  await act(async () => {
    const trigger = more("Codex")!;
    trigger.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse", button: 0 }));
    trigger.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, pointerType: "mouse", button: 0 }));
    trigger.click();
  });
  expect([...document.querySelectorAll("[role=menuitem]")].map((entry) => entry.textContent)).toEqual(["Move earlier", "Move later", "Remove agent"]);
  await chooseItem("Move later");
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "reorder_agents", agents: ["claude", "grok", "codex"] });
});

it("offers no more button while the list is undecided", () => {
  settings.added_agents = null;
  expect(mount().container.querySelector('button[aria-label^="More actions for"]')).toBeNull();
});

it("keeps keyboard focus on the tab that was moved, not on the Settings section entry", async () => {
  const { container } = mount();
  const tabOf = (name: string) => [...container.querySelectorAll<HTMLElement>('[role="tab"]')].find((el) => el.textContent === name)!;
  act(() => tabOf("Claude Code").focus());
  await openTabMenu(container, "Claude Code");
  await chooseItem("Move later");
  // At once, before the menu hands focus back to where it took it from a moment later.
  expect(tabNames(container)).toEqual(["Codex", "Claude Code", "Grok Build"]);
  expect(document.activeElement).toBe(tabOf("Claude Code"));
});

it("shows the order asked for at once, and puts the old one back when it is refused", async () => {
  const { container } = mount();
  let refuse: (reason: Error) => void = () => {};
  request.mockReturnValueOnce(new Promise((_, reject) => void (refuse = reject)));
  await openTabMenu(container, "Claude Code");
  await chooseItem("Move later");
  expect(tabNames(container)).toEqual(["Codex", "Claude Code", "Grok Build"]);
  await act(async () => refuse(new Error("order refused")));
  expect(tabNames(container)).toEqual(["Claude Code", "Codex", "Grok Build"]);
});

it("asks before removing an agent, says its accounts are kept, and removes it once confirmed", async () => {
  const { container } = mount();
  await openTabMenu(container, "Codex");
  await chooseItem("Remove agent");
  const dialog = document.querySelector('[role="alertdialog"], [role="dialog"]')!;
  expect(dialog.textContent).toContain("Remove Codex?");
  expect(dialog.textContent).toContain("Its accounts are kept");
  expect(dialog.textContent).not.toContain("Still using");
  expect(request).not.toHaveBeenCalled();
  await act(async () => buttonNamed(dialog, "Cancel")!.click());
  expect(request).not.toHaveBeenCalled();
  await openTabMenu(container, "Codex");
  await chooseItem("Remove agent");
  await act(async () => buttonNamed(document.querySelector('[role="alertdialog"], [role="dialog"]')!, "Remove")!.click());
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "remove_agent", agent: "codex" });
});

it("names what still uses the agent when removing it, and says it keeps its setting", async () => {
  consoles.set("c", { id: "c", name: "Main", default_agent: "codex" } as Console);
  consoles.set("d", { id: "d", name: "Other", default_agent: "claude" } as Console);
  projects.set("p", { id: "p", name: "Site", default_agent: "codex" } as Project);
  sessions.set("s", { id: "s", title: "Fix build", agent: "codex", status: "idle" } as Session);
  sessions.set("a", { id: "a", title: "Old", agent: "codex", status: "archived" } as Session);
  const { container } = mount();
  await openTabMenu(container, "Codex");
  await chooseItem("Remove agent");
  const dialog = document.querySelector('[role="alertdialog"], [role="dialog"]')!;
  const text = dialog.textContent!.replace(/[\u2066-\u2069]/g, "");
  expect(text).toContain("1 console: Main");
  expect(text).toContain("1 project: Site");
  expect(text).toContain("1 session: Fix build");
  expect(text).toContain("keep that setting");
  expect(text).not.toMatch(/[()]/);
  // The names stand as chips, with no brackets around them.
  expect([...dialog.querySelectorAll("[data-inline-chip]")].map((chip) => chip.textContent?.replace(/[\u2066-\u2069]/g, ""))).toEqual(["Main", "Site", "Fix build"]);
});

describe("dragging a tab", () => {
  /** Lays the tabs out 100px wide side by side, as jsdom has no layout of its own. */
  function layOut(container: HTMLElement) {
    [...container.querySelectorAll<HTMLElement>('[role="tab"]')].forEach((tab, index) => {
      tab.getBoundingClientRect = () => ({ left: index * 100, right: index * 100 + 100, width: 100, top: 0, bottom: 30, height: 30, x: index * 100, y: 0, toJSON: () => ({}) });
    });
  }
  const pointer = (target: EventTarget, type: string, clientX: number) => act(() => {
    target.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, button: 0, clientX }));
  });

  it("sends the whole new order when a tab is dropped past another", () => {
    const { container } = mount();
    layOut(container);
    const first = container.querySelector<HTMLElement>('[role="tab"]')!;
    pointer(first, "pointerdown", 50);
    pointer(document, "pointermove", 70);
    pointer(document, "pointermove", 180);
    pointer(document, "pointerup", 180);
    expect(request).toHaveBeenCalledExactlyOnceWith({ type: "reorder_agents", agents: ["codex", "claude", "grok"] });
    expect(first.style.transform).toBe("");
  });

  it("does nothing for a press that does not move far, or a drag dropped where it started", () => {
    const { container } = mount();
    layOut(container);
    const second = container.querySelectorAll<HTMLElement>('[role="tab"]')[1]!;
    pointer(second, "pointerdown", 150);
    pointer(document, "pointermove", 152);
    pointer(document, "pointerup", 152);
    pointer(second, "pointerdown", 150);
    pointer(document, "pointermove", 170);
    pointer(document, "pointerup", 170);
    expect(request).not.toHaveBeenCalled();
  });

  it("is cancelled by Escape", () => {
    const { container } = mount();
    layOut(container);
    pointer(container.querySelector('[role="tab"]')!, "pointerdown", 50);
    pointer(document, "pointermove", 180);
    act(() => void document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
    pointer(document, "pointerup", 180);
    expect(request).not.toHaveBeenCalled();
  });

  it("is not offered while the list is undecided", () => {
    settings.added_agents = null;
    const { container } = mount();
    layOut(container);
    pointer(container.querySelector('[role="tab"]')!, "pointerdown", 50);
    pointer(document, "pointermove", 180);
    pointer(document, "pointerup", 180);
    expect(request).not.toHaveBeenCalled();
  });
});

describe("an account's actions menu", () => {
  // The account's name is isolated in the label, and the tabs' own hidden menus are named alike.
  const triggers = (container: HTMLElement) => [...container.querySelectorAll<HTMLElement>('button[aria-label^="Actions for"]:not([hidden])')];
  async function openAccountMenu(container: HTMLElement, at = 1) {
    const trigger = triggers(container)[at]!;
    await act(async () => {
      trigger.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse", button: 0 }));
      trigger.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, pointerType: "mouse", button: 0 }));
      trigger.click();
    });
  }

  const items = () => [...document.querySelectorAll("[role=menuitem]")].map((entry) => entry.textContent);

  it("holds Edit, Log in and Remove for an owned account, in place of buttons of their own", async () => {
    const { container } = mount();
    expect(buttonNamed(container, "Edit")).toBeUndefined();
    await openAccountMenu(container, 1);
    expect(items()).toEqual(["Edit", "Log in", "Remove"]);
  });

  it("holds Rename and Log in for the default account, which has no directory or removal to offer", async () => {
    const { container } = mount();
    await openAccountMenu(container, 0);
    expect(items()).toEqual(["Rename", "Log in"]);
  });

  it("is offered by the default account and each owned account", () => {
    const { container } = mount();
    expect(triggers(container)).toHaveLength(2);
    expect(triggers(container)[0]!.getAttribute("aria-label")).toContain("Default");
    expect(triggers(container)[1]!.getAttribute("aria-label")).toContain("claude-work");
  });

  it("starts the login of the account it was chosen on in a modal, the default account's with no account", async () => {
    const { container } = mount();
    await openAccountMenu(container, 1);
    await chooseItem("Log in");
    expect(request).toHaveBeenLastCalledWith({ type: "start_account_login", agent: "claude", account: "claude-work" });
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain("Log in to Claude Code account claude-work");
    expect(dialog.textContent).toContain("Logging in…");
    await act(async () => buttonNamed(dialog, "Cancel login")!.click());
    expect(request).toHaveBeenLastCalledWith({ type: "cancel_account_login", agent: "claude", account: "claude-work" });
    expect(document.querySelector('[role="dialog"]')).toBeNull();
    await openAccountMenu(container, 0);
    await chooseItem("Log in");
    expect(request).toHaveBeenLastCalledWith({ type: "start_account_login", agent: "claude", account: null });
  });

  it("shows the default account under the name the user gave it, and renames it from its menu", async () => {
    settings.default_account_names = { claude: "Personal" };
    const { container } = mount();
    expect(container.textContent).toContain("Personal");
    expect(triggers(container)[0]!.getAttribute("aria-label")).toContain("Personal");
    await openAccountMenu(container, 0);
    await chooseItem("Rename");
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain("Rename the default Claude Code account");
    // Only the name is edited: the default account's directory is not.
    expect(dialog.querySelectorAll("input")).toHaveLength(1);
    expect((dialog.querySelector("input") as HTMLInputElement).value).toBe("Personal");
  });

  it("refuses, in place, a default name an account of the agent has", async () => {
    const { container } = mount();
    await openAccountMenu(container, 0);
    await chooseItem("Rename");
    const input = document.querySelector('[role="dialog"] input') as HTMLInputElement;
    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
      setter.call(input, "Claude-Work");
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await act(async () => buttonNamed(document.querySelector('[role="dialog"]')!, "Save")!.click());
    expect(request).not.toHaveBeenCalled();
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain("already has an account named");
  });

  it("takes up a login found running when the section opens, with the page link, the device code and Cancel login", async () => {
    accountLogins.push({ agent: "claude", account: "claude-work", status: "waiting", authorization_url: "https://claude.test/authorize", device_code: null, opened_by_agent: false, failure: null });
    const { container } = mount();
    const dialog = () => document.querySelector('[role="dialog"]')!;
    expect(request).not.toHaveBeenCalled();
    expect(dialog().textContent).toContain("Waiting for you to authorize in your browser");
    expect(dialog().querySelector('a[href="https://claude.test/authorize"]')?.textContent).toContain("Open the page again");
    expect([...dialog().querySelectorAll("[data-inline-chip]")].map((chip) => chip.textContent?.replace(/[\u2068\u2069]/g, ""))).toEqual(["claude-work"]);
    await act(async () => buttonNamed(dialog(), "Cancel login")!.click());
    expect(request).toHaveBeenLastCalledWith({ type: "cancel_account_login", agent: "claude", account: "claude-work" });
    expect(container.querySelector('[role="dialog"]')).toBeNull();
  });

  it("shows the device code in the modal", () => {
    accountLogins.push({ agent: "claude", account: null, status: "waiting", authorization_url: null, device_code: "ABCD-1234", opened_by_agent: true, failure: null });
    mount();
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain("Confirm this code in your browser: ABCD-1234");
  });

  it("closes the modal when the login succeeds", async () => {
    accountLogins.push({ agent: "claude", account: "claude-work", status: "waiting", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
    const { render } = mount();
    expect(document.querySelector('[role="dialog"]')).not.toBeNull();
    accountLogins.splice(0, 1, { agent: "claude", account: "claude-work", status: "succeeded", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
    render();
    await act(async () => {});
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });

  it("keeps a failed login in the modal with its reason worded from the catalog, and logs in again from it", async () => {
    accountLogins.push({ agent: "claude", account: "claude-work", status: "waiting", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
    const { render } = mount();
    accountLogins.splice(0, 1, { agent: "claude", account: "claude-work", status: "failed", authorization_url: null, device_code: null, opened_by_agent: false,
      failure: { code: "login_failed", params: { agent: "Claude Code", detail: "Login failed: 400" }, message: "x" } });
    render();
    await act(async () => {});
    const dialog = document.querySelector('[role="dialog"]')!;
    const text = dialog.textContent!.replace(/[\u2068\u2069]/g, "");
    expect(text).toContain("Could not log in to claude-work.");
    expect(text).toContain("Claude Code reported: Login failed: 400");
    expect(buttonNamed(dialog, "Cancel login")).toBeUndefined();
    await act(async () => buttonNamed(dialog, "Log in again")!.click());
    expect(dismissAccountLogin).toHaveBeenCalledOnce();
    expect(request).toHaveBeenLastCalledWith({ type: "start_account_login", agent: "claude", account: "claude-work" });
    await act(async () => buttonNamed(document.querySelector('[role="dialog"]')!, "Close")?.click());
  });

  it("leaves no line in the row for a login that ended without success", () => {
    accountLogins.push({ agent: "claude", account: "claude-work", status: "failed", authorization_url: null, device_code: null, opened_by_agent: false,
      failure: { code: "login_failed", params: { agent: "Claude Code", detail: "Login failed: 400" }, message: "x" } });
    const { container } = mount();
    expect([...container.querySelectorAll('[role="status"]')].map((el) => el.textContent).join("")).toBe("");
    expect(buttonNamed(container, "Dismiss")).toBeUndefined();
    expect(container.textContent).not.toContain("Login failed: 400");
  });

  it("leaves no visible line for a login that succeeded, and says so in the hidden status region", () => {
    accountLogins.push({ agent: "claude", account: "claude-work", status: "succeeded", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
    const { container } = mount();
    const status = container.querySelector<HTMLElement>('[role="status"]:not(:empty)')!;
    expect(status.textContent?.replace(/[\u2068\u2069]/g, "")).toBe("claude-work: logged in.");
    expect(status.parentElement?.classList.contains("sr-only")).toBe(true);
    expect(buttonNamed(container, "Dismiss")).toBeUndefined();
    expect(buttonNamed(container, "Cancel login")).toBeUndefined();
  });

  it("starts the login once under StrictMode, and shows how the login really ended, not a refusal of a second start", async () => {
    // The daemon refuses a second start of a login that is running.
    let starts = 0;
    request.mockImplementation(((message: { type: string }) => {
      if (message.type !== "start_account_login") return Promise.resolve({});
      starts += 1;
      return starts === 1 ? Promise.resolve({}) : Promise.reject(Object.assign(new Error("running"), { code: "login_already_running", params: { agent: "Claude Code" } }));
    }) as never);
    const { container, render } = mount(undefined, true);
    await openAccountMenu(container, 1);
    await chooseItem("Log in");
    await act(async () => {});
    expect(starts).toBe(1);
    accountLogins.push({ agent: "claude", account: "claude-work", status: "failed", authorization_url: null, device_code: null, opened_by_agent: false,
      failure: { code: "login_failed", params: { agent: "Claude Code", detail: "Login failed: 400" }, message: "x" } });
    render();
    await act(async () => {});
    const text = document.querySelector('[role="dialog"]')!.textContent!.replace(/[\u2068\u2069]/g, "");
    expect(text).toContain("Claude Code reported: Login failed: 400");
    expect(text).not.toContain("already running");
  });

  it("keeps the modal open for a login started on an account whose earlier login ended in a record the store still holds", async () => {
    accountLogins.push({ agent: "claude", account: "claude-work", status: "cancelled", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
    const { container } = mount();
    await openAccountMenu(container, 1);
    await chooseItem("Log in");
    await act(async () => {});
    expect(dismissAccountLogin).toHaveBeenCalledOnce();
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain("Logging in…");
  });

  it("cancels a login that was still being started when the modal was closed, once the daemon has it", async () => {
    let registered!: () => void;
    request.mockImplementation(((message: { type: string }) => message.type === "start_account_login"
      ? new Promise((resolve) => { registered = () => resolve({}); })
      : Promise.resolve({})) as never);
    const { container } = mount();
    await openAccountMenu(container, 1);
    await chooseItem("Log in");
    await act(async () => buttonNamed(document.querySelector('[role="dialog"]')!, "Cancel login")!.click());
    expect(trackAccountLogin).toHaveBeenLastCalledWith(expect.anything(), "claude", "claude-work", false);
    expect(request).not.toHaveBeenCalledWith({ type: "cancel_account_login", agent: "claude", account: "claude-work" });
    await act(async () => registered());
    expect(request).toHaveBeenLastCalledWith({ type: "cancel_account_login", agent: "claude", account: "claude-work" });
  });

  it("closes the modal when the login's record is gone after it was seen, and attaches to a login already running", async () => {
    request.mockRejectedValueOnce(Object.assign(new Error("running"), { code: "login_already_running", params: {} }));
    const { container, render } = mount();
    await openAccountMenu(container, 1);
    await chooseItem("Log in");
    await act(async () => {});
    expect(document.querySelector('[role="dialog"]')?.textContent).not.toContain("already running");
    accountLogins.push({ agent: "claude", account: "claude-work", status: "waiting", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
    render();
    await act(async () => {});
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain("Waiting for you to authorize");
    accountLogins.length = 0;
    render();
    await act(async () => {});
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });

  it("keeps a failed login in the modal when a snapshot drops its record", async () => {
    accountLogins.push({ agent: "claude", account: "claude-work", status: "waiting", authorization_url: null, device_code: null, opened_by_agent: false, failure: null });
    const { render } = mount();
    accountLogins.splice(0, 1, { agent: "claude", account: "claude-work", status: "failed", authorization_url: null, device_code: null, opened_by_agent: false,
      failure: { code: "login_failed", params: { agent: "Claude Code", detail: "Login failed: 400" }, message: "x" } });
    render();
    await act(async () => {});
    accountLogins.length = 0;
    render();
    await act(async () => {});
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain("Login failed: 400");
    expect(buttonNamed(dialog, "Log in again")).toBeDefined();
  });

  it("notes that this window started the login it asked for, and shows a refusal in the modal", async () => {
    const { container } = mount();
    request.mockRejectedValueOnce(Object.assign(new Error("busy"), { code: "login_codex_busy", params: {} }));
    await openAccountMenu(container, 0);
    await chooseItem("Log in");
    await act(async () => {});
    expect(trackAccountLogin).toHaveBeenLastCalledWith(expect.anything(), "claude", null, false);
    const dialog = document.querySelector('[role="dialog"]')!;
    expect(dialog.textContent).toContain("Another Codex account is logging in");
    expect(buttonNamed(dialog, "Log in again")).toBeDefined();
    expect(toastError).not.toHaveBeenCalled();
  });

  it("keeps the login status region out of display:none while it is empty, so its first text is announced", () => {
    const { container } = mount();
    const status = container.querySelector<HTMLElement>('[role="status"]')!;
    expect(status.textContent).toBe("");
    expect(status.closest(".hidden")).toBeNull();
    // Out of the grid's flow, so it takes no row of its own.
    expect(status.parentElement!.className).toBe("sr-only");
  });

  it("opens the account form from Edit and the confirmation from Remove", async () => {
    const { container } = mount();
    await openAccountMenu(container);
    await chooseItem("Edit");
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain("Edit Claude Code account");
    act(() => buttonNamed(document, "Cancel")!.click());
    await openAccountMenu(container);
    await chooseItem("Remove");
    expect(document.querySelector('[role="alertdialog"], [role="dialog"]')?.textContent).toContain("Remove account");
    await act(async () => buttonNamed(document.querySelector('[role="alertdialog"], [role="dialog"]')!, "Remove")!.click());
    expect(request).toHaveBeenCalledExactlyOnceWith({ type: "delete_account", account: "claude-work" });
  });
});

it.each([
  ["edit", false], ["edit", true],
  ["add", false], ["add", true],
  ["rename", false], ["rename", true],
  ["remove account", false], ["remove account", true],
  ["remove agent", false], ["remove agent", true],
] as const)("dismisses the %s form for navigation requesting Add agent=%s", async (form, addAgent) => {
  const { container, navigate } = mount();
  if (form === "add") {
    act(() => buttonNamed(container, "Add account")!.click());
  } else if (form === "remove agent") {
    await openTabMenu(container, "Claude Code");
    await chooseItem("Remove agent");
  } else {
    const row = container.querySelector(`[data-list-motion="${form === "rename" ? "default:claude" : "claude-work"}"]`)!;
    const trigger = row.querySelector<HTMLButtonElement>('button[aria-label^="Actions"]')!;
    await act(async () => {
      trigger.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse", button: 0 }));
      trigger.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, pointerType: "mouse", button: 0 }));
      trigger.click();
    });
    await chooseItem(form === "edit" ? "Edit" : form === "rename" ? "Rename" : "Remove");
  }
  expect(document.querySelector('[role="dialog"], [role="alertdialog"]')).not.toBeNull();
  navigate(addAgent);
  await act(async () => {});
  const dialogs = [...document.querySelectorAll('[role="dialog"], [role="alertdialog"]')];
  if (addAgent) {
    expect(dialogs).toHaveLength(1);
    expect(dialogs[0]!.textContent).toContain("Only an agent installed on this machine can be added.");
  } else {
    expect(dialogs).toHaveLength(0);
  }
});

it.each([false, true])("keeps an active login for navigation requesting Add agent=%s", async (addAgent) => {
  const login: AccountLogin = {
    agent: "claude", account: "claude-work", status: "waiting", authorization_url: null,
    device_code: null, opened_by_agent: false, failure: null,
  };
  accountLogins.push(login);
  const { navigate } = mount();
  await act(async () => {});
  const dialog = document.querySelector('[role="dialog"]')!;
  expect(dialog.textContent).toContain("Waiting for you to authorize");
  navigate(addAgent);
  await act(async () => {});
  expect(dialog.isConnected).toBe(true);
  expect(dialog.textContent).toContain("Waiting for you to authorize");
  expect(accountLogins).toEqual([login]);
  expect(request).not.toHaveBeenCalledWith(expect.objectContaining({ type: "cancel_account_login" }));
});

it("lets Manage accounts recovery dismiss a stale editor and focus the real Settings section", async () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  document.documentElement.style.setProperty("--breakpoint-docked", "1168px");
  onTestFinished(() => { document.documentElement.style.removeProperty("--breakpoint-docked"); });
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: media.startsWith("(min-width:"), addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  onTestFinished(() => { vi.unstubAllGlobals(); });
  let nonce = 1;
  const render = () => act(() => root.render(<><SettingsDialog navigation={{ nonce, section: "accounts", addAgent: false }} onClose={() => {}} /><ErrorAlerts /></>));
  const unregister = registerSettingsLauncher(() => { nonce += 1; render(); });
  onTestFinished(() => {
    unregister();
    act(() => {
      root.unmount();
      while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
    });
    container.remove();
  });
  render();
  await trackMountRefocus()();
  const row = document.querySelector('[data-list-motion="claude-work"]')!;
  await act(async () => {
    const trigger = row.querySelector<HTMLButtonElement>('button[aria-label^="Actions"]')!;
    trigger.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse", button: 0 }));
    trigger.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, pointerType: "mouse", button: 0 }));
    trigger.click();
  });
  await chooseItem("Edit");
  await trackMountRefocus()();
  act(() => {
    settings.accounts = settings.accounts.filter((account) => account.id !== "claude-work");
    bus.notify();
  });
  request.mockRejectedValueOnce(new DaemonRequestError("Account no longer exists.", "unknown_account", {}));
  await act(async () => buttonNamed(document, "Save")!.click());
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("Manage accounts…");
  act(() => buttonNamed(document, "Manage accounts…")!.click());
  await act(async () => {
    for (let frame = 0; frame < 4; frame++) await new Promise((resolve) => requestAnimationFrame(resolve));
  });
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
  expect(document.querySelectorAll('[role="dialog"]')).toHaveLength(1);
  const selected = document.querySelector('[role="tab"][aria-selected="true"]');
  expect(selected?.textContent).toContain("Agent accounts");
  expect(document.activeElement).toBe(selected);
});
