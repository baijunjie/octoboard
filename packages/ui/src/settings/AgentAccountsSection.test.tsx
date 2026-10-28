// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { describe, expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "../errorAlerts";
import { currentLanguage, setLanguage, t } from "../i18n/language";
import type { Console, Project, Session } from "../protocol";
import { AccountsNavigationContext } from "./AccountsNavigationContext";
import { AgentAccountsSection } from "./AgentAccountsSection";
import { account, availability, bus, consoles, projects, request, sectionEntry, sessions, settings, toastError } from "./agentAccountsSectionTestState";
import { animate, buttonNamed, chooseItem, menuItem, mount, openDetailsOf, openTabMenu, rowOf, selectTab } from "./agentAccountsSectionTestSupport";

vi.mock("../store", async (importOriginal) => (await import("./agentAccountsSectionTestState")).storeStandIn(await importOriginal()));
vi.mock("../components/AgentIcon", async (importOriginal) => (await import("./agentAccountsSectionTestState")).agentIconStandIn(await importOriginal()));
vi.mock("../components/FadeOverflow", async () => ({ FadeOverflow: (await import("./agentAccountsSectionTestState")).FadeOverflowStandIn }));
vi.mock("../components/MarkedPath", async () => ({ MarkedPath: (await import("./agentAccountsSectionTestState")).MarkedPathStandIn }));
vi.mock("./useSectionRefocus", async (importOriginal) => (await import("./agentAccountsSectionTestState")).sectionRefocusStandIn(await importOriginal()));
vi.mock("../dialogs/OptionSelect", async () => ({ OptionSelect: (await import("./agentAccountsSectionTestState")).OptionSelectStandIn }));

/** Waits out React Aria's focus restore after a dialog closes and what runs in the frames after it. */
const frames = () => act(() => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(resolve, 0)))));

/** The approval selector of the agent's tab in view. */
function approvalOf(container: HTMLElement): HTMLSelectElement {
  return container.querySelector<HTMLSelectElement>('[role="tabpanel"] select')!;
}

it("offers one Codex mode above the table, following configuration by default", () => {
  const { container } = mount("Codex");
  const select = approvalOf(container);
  expect(select.value).toBe("inherit");
  expect(select.getAttribute("aria-label")).toBe("Permission approval for Codex");
  expect([...select.options].map((option) => option.value)).toEqual(["inherit", "auto_review", "user"]);
  expect(container.querySelectorAll("tbody select")).toHaveLength(0);
});

it("says the permission approval once, as a row above the table with its help, not in a column or the details", async () => {
  const { container } = mount("Codex");
  const panel = container.querySelector('[role="tabpanel"]')!;
  expect(panel.textContent?.match(/Permission approval/g)).toHaveLength(1);
  expect(panel.textContent).toContain("Applies to every Codex account");
  expect(panel.querySelectorAll('button[aria-label="About permission approval"]')).toHaveLength(1);
  expect(panel.querySelector("select")!.compareDocumentPosition(panel.querySelector("table")!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  const dialog = await openDetailsOf(container, "codex-work");
  expect(dialog.querySelector("select")).toBeNull();
  expect(dialog.textContent).not.toContain("Permission approval");
});

it("shows no permission approval row for Grok", () => {
  const { container } = mount("Grok Build");
  expect(container.querySelector("select")).toBeNull();
  expect(container.textContent).not.toContain("Permission approval");
});

it("shows the stored Codex choice", () => {
  settings.codex_approval_reviewer = "user";
  const { container } = mount("Codex");
  expect(approvalOf(container).value).toBe("user");
});

it.each([
  ["auto_review", "auto_review"],
  ["user", "user"],
  ["inherit", null],
] as const)("saves the Codex selector as %s at once", async (value, reviewer) => {
  settings.codex_approval_reviewer = value === "inherit" ? "user" : null;
  const { container, change } = mount("Codex");
  change(approvalOf(container), value);
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "set_codex_approval_reviewer", reviewer });
  await act(async () => {});
});

it("keeps the stored value and reports a failed save", async () => {
  const { container, change } = mount("Codex");
  const select = approvalOf(container);
  request.mockRejectedValueOnce(new Error("save refused"));
  change(select, "auto_review");
  await act(async () => {});
  expect(select.value).toBe("inherit");
  expect(errorAlerts.getSnapshot().map((alert) => alert.title)).toEqual(["Could not save the permission approval"]);
  while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
  expect(select.disabled).toBe(false);
});

it("keeps the trigger available while saving and prevents another option selection until answered", async () => {
  let finish!: (value: object) => void;
  const { container, change } = mount("Codex");
  const select = approvalOf(container);
  request.mockImplementationOnce(() => new Promise((resolve) => { finish = resolve; }));
  change(select, "auto_review");
  expect(select.disabled).toBe(false);
  expect([...select.options].every((option) => option.disabled)).toBe(true);
  await act(async () => finish({}));
  expect([...select.options].every((option) => !option.disabled)).toBe(true);
});

it("offers Claude Code's four choices, following configuration by default, and shows the stored one", () => {
  const { container } = mount("Claude Code");
  const select = approvalOf(container);
  expect(select.value).toBe("inherit");
  expect(select.getAttribute("aria-label")).toBe("Permission approval for Claude Code");
  expect([...select.options].map((option) => option.value)).toEqual(["inherit", "auto", "manual", "bypass"]);
  settings.claude_approval_mode = "manual";
  expect(approvalOf(mount("Claude Code").container).value).toBe("manual");
});

it.each([
  ["auto", "auto"],
  ["manual", "manual"],
  ["inherit", null],
] as const)("saves the Claude Code selector as %s at once", async (value, mode) => {
  settings.claude_approval_mode = value === "inherit" ? "auto" : null;
  const { container, change } = mount("Claude Code");
  change(approvalOf(container), value);
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "set_claude_approval_mode", mode });
  await act(async () => {});
});

it("keeps the confirmation open and the choice unchanged when saving Approve everything fails", async () => {
  const { container, change } = mount("Claude Code");
  const select = approvalOf(container);
  change(select, "bypass");
  request.mockRejectedValueOnce(new Error("save refused"));
  act(() => [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Approve everything")!.click());
  await act(async () => {});
  expect(document.body.textContent).toContain("in every Claude Code account's sessions");
  expect(select.value).toBe("inherit");
});

it("saves Approve everything only once the warning, which names the agent, is confirmed", async () => {
  const { container, change } = mount("Claude Code");
  const select = approvalOf(container);
  change(select, "bypass");
  expect(request).not.toHaveBeenCalled();
  const buttons = () => [...document.querySelectorAll<HTMLButtonElement>("button")];
  const press = (label: string) => act(() => buttons().find((b) => b.textContent === label)!.click());
  expect(document.querySelector('[role="alertdialog"] h2')?.textContent).toBe("Approve everything");
  expect(document.body.textContent).toContain("in every Claude Code account's sessions");
  press("Cancel");
  expect(request).not.toHaveBeenCalled();
  expect(document.body.textContent).not.toContain("in every Claude Code account's sessions");
  change(select, "bypass");
  press("Approve everything");
  await act(async () => {});
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "set_claude_approval_mode", mode: "bypass" });
});

it("shows one agent at a time, opening on the first and switching by tab", () => {
  const { container } = mount();
  const tabs = [...container.querySelectorAll('[role="tab"]')];
  expect(tabs.map((tab) => tab.textContent)).toEqual(["Claude Code", "Codex", "Grok Build"]);
  expect(tabs.map((tab) => tab.getAttribute("aria-selected"))).toEqual(["true", "false", "false"]);
  expect(container.textContent).toContain("claude-work");
  expect(container.textContent).not.toContain("codex-work");
  selectTab(container, "Grok Build");
  expect(container.textContent).toContain("grok-work");
  expect(container.textContent).not.toContain("claude-work");
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

it("replaces a panel on a tab switch without any list motion", async () => {
  const { container } = mount();
  await act(async () => {});
  selectTab(container, "Codex");
  await act(async () => {});
  expect(container.querySelector("[data-list-motion-ghost]")).toBeNull();
  expect(animate).not.toHaveBeenCalled();
  expect(container.textContent).toContain("codex-work");
});

it("drops the row of an account removed and adds the row of one added within the panel in view", async () => {
  const { container, render } = mount();
  const rows = () => [...container.querySelectorAll("tbody > tr")].map((row) => row.getAttribute("data-account-row"));
  expect(rows()).toEqual(["default:claude", "claude-work"]);
  const removed = rowOf(container, "claude-work");
  const work = settings.accounts[0]!;
  settings.accounts = settings.accounts.filter((a) => a !== work);
  await act(async () => render());
  expect(rows()).toEqual(["default:claude"]);
  expect(removed.isConnected).toBe(false);
  const visual = container.querySelector('[data-list-motion-ghost][data-account-row="claude-work"]')!;
  expect(visual.getAttribute("aria-hidden")).toBe("true");
  expect(visual.hasAttribute("inert")).toBe(true);
  settings.accounts = [...settings.accounts, account("claude-home", "claude")];
  await act(async () => render());
  expect(rows()).toEqual(["default:claude", "claude-home"]);
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

it("shows the agent tab of an account Settings was asked about and opens its details on the sharing switch", async () => {
  const { container, showSharingOf } = mount("Claude Code");
  const switchOf = () => document.body.querySelector('[data-sharing-account="codex-work"] input[role="switch"]');
  expect(switchOf()).toBeNull();
  showSharingOf("codex-work");
  await act(async () => { await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined))); });
  expect(container.querySelector('[role="tab"][aria-selected="true"]')?.textContent).toBe("Codex");
  expect(document.querySelector('[role="dialog"]')).not.toBeNull();
  expect(switchOf()).not.toBeNull();
  expect(document.activeElement).toBe(switchOf());
});

it("shows the focus ring on the sharing switch even after a pointer press opened Settings", async () => {
  const { showSharingOf } = mount("Claude Code");
  act(() => setInteractionModality("pointer"));
  showSharingOf("codex-work");
  await act(async () => { await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined))); });
  expect(document.activeElement).toBe(document.body.querySelector('[data-sharing-account="codex-work"] input[role="switch"]'));
  expect(getInteractionModality()).toBe("keyboard");
});

it("opens no details for an account whose agent is not in the list, focusing the section's tab", async () => {
  settings.added_agents = ["claude", "grok"];
  const { container, showSharingOf } = mount("Claude Code");
  const entry = document.body.lastElementChild as HTMLElement;
  showSharingOf("codex-work");
  await act(async () => { await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined))); });
  expect(document.activeElement).toBe(entry);
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  // The agent comes back into the list afterwards: nothing opens by itself.
  settings.added_agents = ["claude", "codex", "grok"];
  act(() => bus.notify());
  selectTab(container, "Codex");
  await act(async () => { await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined))); });
  expect(document.querySelector('[role="dialog"]')).toBeNull();
});

it("gives focus back to the row's actions button after its details close, in a language whose button name is not English", async () => {
  const before = currentLanguage();
  setLanguage("zh-Hans");
  onTestFinished(() => { act(() => setLanguage(before)); });
  const { container } = mount("Codex");
  const trigger = rowOf(container, "codex-work").querySelector<HTMLElement>("[data-row-actions] button")!;
  expect(trigger.getAttribute("aria-label")).not.toContain("Actions for");
  const dialog = await openDetailsOf(container, "codex-work", t("settings.accounts.settingsItem"));
  await act(async () => buttonNamed(dialog, t("common.close"))!.click());
  await frames();
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  expect(document.activeElement).toBe(trigger);
});

it("opens the details of an account from the first entry of its actions menu, gives focus back to the menu's button on closing, and closes with the account", async () => {
  const { container, render } = mount("Codex");
  const trigger = rowOf(container, "codex-work").querySelector<HTMLElement>('button[aria-label^="Actions for"]')!;
  const dialog = await openDetailsOf(container, "codex-work");
  expect(dialog.textContent).toContain("codex-work");
  expect(dialog.querySelector("h2")?.textContent).toBe("Account settings");
  // The body leads with the account's name.
  expect(dialog.querySelector("[data-slot=modal-body]")?.textContent).toMatch(/^codex-work/);
  expect(dialog.textContent).not.toContain("Codex account");
  expect(dialog.textContent).toContain("/home/dev/.codex-work");
  // No footer: the header's close button closes it.
  expect(buttonNamed(dialog, "Close")).toBe(dialog.querySelector('button[aria-label="Close"]'));
  await act(async () => buttonNamed(dialog, "Close")!.click());
  await frames();
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  expect(document.activeElement).toBe(trigger);
  await openDetailsOf(container, "codex-work");
  expect(document.querySelector('[role="dialog"]')).not.toBeNull();
  settings.accounts = settings.accounts.filter((held) => held.id !== "codex-work");
  render();
  expect(document.querySelector('[role="dialog"]')).toBeNull();
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

it("offers the same entries in an actions button of each tab, named for its agent", async () => {
  const { container } = mount();
  // The tab's own button, in the wrapper the strip places it with; the tab's context menu has a hidden trigger of the same name.
  const actions = (agent: string) => container.querySelector<HTMLElement>(`[data-tab-actions] button[aria-label="Actions for ${agent}"]`) ?? undefined;
  for (const agent of ["Claude Code", "Codex", "Grok Build"]) expect(actions(agent)).toBeDefined();
  await act(async () => {
    const trigger = actions("Codex")!;
    trigger.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse", button: 0 }));
    trigger.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, pointerType: "mouse", button: 0 }));
    trigger.click();
  });
  expect([...document.querySelectorAll("[role=menuitem]")].map((entry) => entry.textContent)).toEqual(["Move earlier", "Move later", "Remove agent"]);
  await chooseItem("Move later");
  expect(request).toHaveBeenCalledExactlyOnceWith({ type: "reorder_agents", agents: ["claude", "grok", "codex"] });
});

it("offers no per-tab actions while the list is undecided", () => {
  settings.added_agents = null;
  expect(mount().container.querySelectorAll("[data-tab-actions]")).toHaveLength(0);
});

it("puts Add agent and Add account at the end of the tab row as icon-only buttons, and Add account adds to the tab in view", () => {
  const { container } = mount("Codex");
  const strip = container.querySelector('[role="tablist"]')!.closest(".tabs__list-container")!;
  const add = buttonNamed(strip, "Add account")!;
  const addAgent = buttonNamed(strip, "Add agent")!;
  expect(add.textContent).toBe("");
  expect(addAgent.textContent).toBe("");
  expect(container.querySelector('[role="tabpanel"]')!.textContent).not.toContain("Add account");
  act(() => add.click());
  expect(document.body.querySelector<HTMLSelectElement>('select[aria-label="Agent"]')!.value).toBe("codex");
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
  expect(dialog.querySelector("h2")?.textContent).toBe("Remove agent");
  expect(dialog.textContent).toContain("Its accounts are kept");
  expect(dialog.textContent).not.toContain("still use");
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
  expect(text).toContain("1 console has it as its default agent: Main");
  expect(text).toContain("1 project has it as its default agent: Site");
  expect(text).toContain("1 session uses it: Fix build");
  expect(text).toContain("keep that setting");
  expect(text).not.toMatch(/[()]/);
  // The names stand as chips, with no brackets around them.
  expect([...dialog.querySelectorAll("[data-inline-reference]")].map((chip) => chip.textContent?.replace(/[\u2066-\u2069]/g, ""))).toEqual(["Codex", "Codex", "Main", "Site", "Fix build"]);
});

it.each(["agent", "account"] as const)("puts focus on the section's entry once a removed %s's confirmation closes, not where it stood when the menu opened", async (removed) => {
  // Another section's entry holds focus, as an accessibility press that selected this section leaves it.
  const elsewhere = document.body.appendChild(document.createElement("button"));
  elsewhere.textContent = "General";
  onTestFinished(() => elsewhere.remove());
  const { container } = mount();
  act(() => elsewhere.focus());
  // The confirmation keeps focus inside itself at once, as in a browser; jsdom's containment finds nothing in it to
  // focus and takes focus back only a frame later, too late for the menu to see its hand-back refused.
  elsewhere.addEventListener("focus", () => document.querySelector<HTMLElement>('[role="alertdialog"]')?.focus());
  if (removed === "agent") {
    await openTabMenu(container, "Grok Build");
    await chooseItem("Remove agent");
  } else {
    const trigger = rowOf(container, "claude-work").querySelector<HTMLButtonElement>('button[aria-label^="Actions"]')!;
    await act(async () => {
      trigger.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse", button: 0 }));
      trigger.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, pointerType: "mouse", button: 0 }));
      trigger.click();
    });
    await chooseItem("Remove");
  }
  // The menu has tried to give focus back to where it stood, which the confirmation refused.
  await frames();
  const confirm = buttonNamed(document.querySelector('[role="alertdialog"]')!, "Remove")!;
  act(() => confirm.focus());
  await act(async () => confirm.click());
  expect(request).toHaveBeenCalledExactlyOnceWith(removed === "agent" ? { type: "remove_agent", agent: "grok" } : { type: "delete_account", account: "claude-work" });
  if (removed === "agent") settings.added_agents = ["claude", "codex"];
  else settings.accounts = settings.accounts.filter((held) => held.id !== "claude-work");
  act(() => bus.notify());
  await frames();
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
  expect(document.activeElement).toBe(sectionEntry.current);
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
