// Shared by the `AgentAccountsSection` test files, which split the section's tests by area. Importing it resets the
// daemon state of `./agentAccountsSectionTestState` before each test of the importing file, and clears what a test
// left behind after it; each file still declares the six `vi.mock` calls that state module has the stand-ins for.
import { act, StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { afterEach, beforeEach, onTestFinished, vi } from "vitest";

import { errorAlerts } from "../errorAlerts";
import { AccountsNavigationContext } from "./AccountsNavigationContext";
import { AgentAccountsSection } from "./AgentAccountsSection";
import {
  account,
  accountLogins,
  accountQuotas,
  accountSharing,
  availability,
  bus,
  consoles,
  dismissAccountLogin,
  projects,
  request,
  sectionEntry,
  sessions,
  settings,
  toastError,
  trackAccountLogin,
} from "./agentAccountsSectionTestState";
import { SettingsFocusContext } from "./useSectionRefocus";

// jsdom has no Web Animations, and without them the list motion drops a departing row's copy at once, so a
// stub is what lets a test see the motion a removal or a tab switch starts.
export const animate = vi.fn(() => ({ cancel: vi.fn(), effect: { getComputedTiming: () => ({ progress: 0 }) }, onfinish: null }));
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
  settings.codex_approval_reviewer = null;
  settings.claude_approval_mode = null;
  accountQuotas.length = 0;
  accountLogins.length = 0;
  accountSharing.clear();
  settings.default_account_names = undefined;
  settings.disabled_default_accounts = undefined;
  availability.clear();
});
afterEach(() => {
  act(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); });
  delete (HTMLElement.prototype as Partial<HTMLElement>).animate;
});

/** Presses an agent's tab the way a pointer does. */
export function selectTab(container: HTMLElement, name: string) {
  const tab = [...container.querySelectorAll<HTMLElement>('[role="tab"]')].find((el) => el.textContent === name)!;
  act(() => {
    for (const type of ["pointerdown", "mousedown", "pointerup", "mouseup", "click"]) {
      tab.dispatchEvent(new MouseEvent(type, { bubbles: true, button: 0 }));
    }
  });
}

export function mount(tab?: string, strict = false) {
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
  let navigation: React.ContextType<typeof AccountsNavigationContext> = { pending: false, dismissForms: 0, consume: () => {}, consumeAccount: () => {} };
  const render = () => act(() => {
    const section = <SettingsFocusContext.Provider value={() => entry}><AccountsNavigationContext.Provider value={navigation}><AgentAccountsSection /></AccountsNavigationContext.Provider></SettingsFocusContext.Provider>;
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
  const showSharingOf = (account: string) => {
    navigation = { ...navigation, account, dismissForms: navigation.dismissForms + 1 };
    render();
  };
  return { container, selects, change, opening, render, navigate, showSharingOf };
}

export const sharingSwitches = (container: HTMLElement) => [...container.querySelectorAll<HTMLInputElement>('input[role="switch"]')].filter((input) => input.getAttribute("aria-label")?.includes("Share default configuration"));

export const buttonNamed = (container: ParentNode, name: string) => [...container.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === name || b.getAttribute("aria-label") === name);

export async function openTabMenu(container: HTMLElement, name: string) {
  const tab = [...container.querySelectorAll<HTMLElement>('[role="tab"]')].find((el) => el.textContent === name)!;
  await act(async () => {
    tab.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 10, clientY: 10 }));
  });
}
export const menuItem = (name: string) => [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((entry) => entry.textContent === name)!;
export const chooseItem = (name: string) => act(async () => {
  const entry = menuItem(name);
  entry.focus();
  entry.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  entry.dispatchEvent(new KeyboardEvent("keyup", { key: "Enter", bubbles: true }));
});

/** The table row of an account: its id, or `default:<agent>` for an agent's default account. */
export const rowOf = (container: ParentNode, id: string) => container.querySelector<HTMLElement>(`[data-account-row="${id}"]`)!;

/** Opens the actions menu of an account's row, the way a pointer does, and returns its trigger. */
async function openRowMenu(container: ParentNode, id: string): Promise<HTMLElement> {
  const trigger = rowOf(container, id).querySelector<HTMLElement>("[data-row-actions] button")!;
  await act(async () => {
    trigger.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, pointerType: "mouse", button: 0 }));
    trigger.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, pointerType: "mouse", button: 0 }));
    trigger.click();
  });
  return trigger;
}

/** Chooses the Settings entry (`entry` is its word in another language), which opens the account's `AccountDetailsDialog`,
 * from the actions menu of an account's row and returns the modal it opens. */
export async function openDetailsOf(container: ParentNode, id: string, entry = "Settings"): Promise<HTMLElement> {
  // One that is open is closed first: it holds the rest of the window inert.
  const open = document.body.querySelector<HTMLElement>('[role="dialog"] button[aria-label="Close"]');
  if (open) {
    await act(async () => open.click());
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
  }
  await openRowMenu(container, id);
  const item = [...document.querySelectorAll<HTMLElement>("[role=menuitem]")].find((candidate) => candidate.textContent === entry)!;
  await act(async () => {
    item.focus();
    item.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    item.dispatchEvent(new KeyboardEvent("keyup", { key: "Enter", bubbles: true }));
  });
  return document.body.querySelector<HTMLElement>('[role="dialog"]')!;
}

const ACCOUNT_OF_TAB: Record<string, string> = { "Claude Code": "claude-work", Codex: "codex-work", "Grok Build": "grok-work" };

/** `mount` with the details of the tab's owned account open, whose controls are then found in the modal rather than
 * in the section: `container` is the modal, and `section` the section's own. */
export async function mountDetails(tab: string, strict = false) {
  const mounted = mount(tab, strict);
  const dialog = await openDetailsOf(mounted.container, ACCOUNT_OF_TAB[tab]!);
  return { ...mounted, container: dialog, section: mounted.container, dialog };
}

/** The icon Log in button of an account's row, if it shows one. */
export const rowLogin = (row: ParentNode) => row.querySelector<HTMLButtonElement>('button[aria-label^="Log in to"]') ?? undefined;
