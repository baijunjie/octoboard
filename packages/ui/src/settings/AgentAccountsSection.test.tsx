// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { describe, expect, it, onTestFinished, vi } from "vitest";

import type { Console, Project, Session } from "../protocol";
import { AccountsNavigationContext } from "./AccountsNavigationContext";
import { AgentAccountsSection } from "./AgentAccountsSection";
import { account, availability, bus, consoles, projects, request, sessions, settings, toastError } from "./agentAccountsSectionTestState";
import { animate, buttonNamed, chooseItem, menuItem, mount, openTabMenu, selectTab } from "./agentAccountsSectionTestSupport";

vi.mock("../store", async (importOriginal) => (await import("./agentAccountsSectionTestState")).storeStandIn(await importOriginal()));
vi.mock("../components/AgentIcon", async (importOriginal) => (await import("./agentAccountsSectionTestState")).agentIconStandIn(await importOriginal()));
vi.mock("../components/FadeOverflow", async () => ({ FadeOverflow: (await import("./agentAccountsSectionTestState")).FadeOverflowStandIn }));
vi.mock("../components/MarkedPath", async () => ({ MarkedPath: (await import("./agentAccountsSectionTestState")).MarkedPathStandIn }));
vi.mock("./useSectionRefocus", async (importOriginal) => (await import("./agentAccountsSectionTestState")).sectionRefocusStandIn(await importOriginal()));
vi.mock("../dialogs/OptionSelect", async () => ({ OptionSelect: (await import("./agentAccountsSectionTestState")).OptionSelectStandIn }));

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
  expect(add.closest('[role="tabpanel"]')).toBe(container.querySelector('[role="tabpanel"]'));
  expect(buttonNamed(container, "Add agent")!.closest('[role="tabpanel"]')).toBeNull();
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
  expect(buttonNamed(container, "Add account")).toBeUndefined();
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
    root.render(<AccountsNavigationContext.Provider value={{ pending, consume, consumeAccount: () => {}, dismissForms: 0 }}><AgentAccountsSection /></AccountsNavigationContext.Provider>);
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
    secondRoot.render(<AccountsNavigationContext.Provider value={{ pending: false, consume, consumeAccount: () => {}, dismissForms: 0 }}><AgentAccountsSection /></AccountsNavigationContext.Provider>);
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
    root.render(<AccountsNavigationContext.Provider value={{ ...value, consume: () => {}, consumeAccount: () => {} }}><AgentAccountsSection /></AccountsNavigationContext.Provider>);
    bus.notify();
  });
  render({ pending: true, dismissForms: 0 });
  expect(document.querySelector('[role="dialog"]')).not.toBeNull();
  render({ pending: false, dismissForms: 1 });
  expect(document.querySelector('[role="dialog"]')).toBeNull();
});

it("shows the agent tab of an account Settings was asked about and focuses its sharing switch", async () => {
  const { container, showSharingOf } = mount("Claude Code");
  const switchOf = () => container.querySelector('[data-sharing-account="codex-work"] input[role="switch"]');
  expect(switchOf()).toBeNull();
  showSharingOf("codex-work");
  await act(async () => { await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined))); });
  expect(container.querySelector('[role="tab"][aria-selected="true"]')?.textContent).toBe("Codex");
  expect(switchOf()).not.toBeNull();
  expect(document.activeElement).toBe(switchOf());
});

it("shows the focus ring on the sharing switch even after a pointer press opened Settings", async () => {
  const { container, showSharingOf } = mount("Claude Code");
  act(() => setInteractionModality("pointer"));
  showSharingOf("codex-work");
  await act(async () => { await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined))); });
  expect(document.activeElement).toBe(container.querySelector('[data-sharing-account="codex-work"] input[role="switch"]'));
  expect(getInteractionModality()).toBe("keyboard");
});

it("gives up on the sharing switch of an account whose agent is not in the list, focusing the section's tab and not hunting again", async () => {
  settings.added_agents = ["claude", "grok"];
  const { container, showSharingOf } = mount("Claude Code");
  const entry = document.body.lastElementChild as HTMLElement;
  const waitFrames = async (count: number) => {
    for (let waited = 0; waited < count; waited++) await act(async () => { await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined))); });
  };
  showSharingOf("codex-work");
  await waitFrames(35);
  expect(document.activeElement).toBe(entry);
  // The agent comes back into the list afterwards: the switch exists now, and must not take focus.
  settings.added_agents = ["claude", "codex", "grok"];
  act(() => bus.notify());
  selectTab(container, "Codex");
  await waitFrames(3);
  expect(container.querySelector('[data-sharing-account="codex-work"] input[role="switch"]')).not.toBeNull();
  expect(document.activeElement).not.toBe(container.querySelector('[data-sharing-account="codex-work"] input[role="switch"]'));
});

it("focuses the section's tab when the account to show is unknown", async () => {
  const { showSharingOf } = mount("Claude Code");
  const entry = document.body.lastElementChild as HTMLElement;
  showSharingOf("nobody");
  await act(async () => { await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined))); });
  expect(document.activeElement).toBe(entry);
});

it("flags only an added agent that is not installed", () => {
  settings.added_agents = ["claude", "grok"];
  availability.set("codex", { agent: "codex", availability: "unavailable" });
  availability.set("grok", { agent: "grok", availability: "unavailable" });
  const { container } = mount();
  expect(tabNames(container)).toEqual(["Claude Code", "Grok Build"]);
  expect([...container.querySelectorAll('[role="tab"]')].map((tab) => tab.getAttribute("aria-label"))).toEqual([null, "Grok Build (not installed)"]);
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
  expect([...dialog.querySelectorAll("[data-inline-reference]")].map((chip) => chip.textContent?.replace(/[\u2066-\u2069]/g, ""))).toEqual(["Codex", "Codex", "Codex", "Main", "Site", "Fix build"]);
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
