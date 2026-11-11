// @vitest-environment jsdom
import { setInteractionModality } from "react-aria";
import { useContext, useEffect } from "react";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { AccountsNavigationContext } from "./accounts/AccountsNavigationContext";
import { MAX_FRAMES_TO_WAIT_FOR_MODAL, SettingsDialog } from "./SettingsDialog";
import type { SettingsNavigation } from "./sections";
import type { HostFacts } from "../platform/host";
import { PlatformProvider } from "../platform/react";
import { browserHost } from "../platform/host";
import { MAC_HOST, WINDOWS_HOST } from "../test/hosts";
import { matchesWidthQuery } from "../test/viewport";
import { framesAsTimers, settleFrames } from "../components/dialog/dialogTestSupport";
import { fakeTimers, runOutTimers } from "../test/clock";

beforeEach(() => {
  const globals = globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean };
  const actEnvironment = globals.IS_REACT_ACT_ENVIRONMENT;
  globals.IS_REACT_ACT_ENVIRONMENT = true;
  onTestFinished(() => {
    if (actEnvironment === undefined) delete globals.IS_REACT_ACT_ENVIRONMENT;
    else globals.IS_REACT_ACT_ENVIRONMENT = actEnvironment;
  });
});

const taken = vi.hoisted(() => ({ count: 0, dismiss: 0 }));
vi.mock("./accounts/AgentAccountsSection", () => ({
  AGENT_STRIP_HEIGHT: 41,
  AgentAccountsSection: () => {
    const accountsNavigation = useContext(AccountsNavigationContext);
    taken.dismiss = accountsNavigation.dismissForms;
    useEffect(() => {
      if (!accountsNavigation.pending) return;
      taken.count += 1;
      accountsNavigation.consume();
    }, [accountsNavigation.pending]);
    return <p>accounts section</p>;
  },
}));
vi.mock("./GeneralSection", () => ({ GeneralSection: () => <p>General</p> }));
vi.mock("./AppBehaviorSection", () => ({ AppBehaviorSection: () => <p>App behavior section</p> }));
vi.mock("./GitSection", () => ({ GitSection: () => <p>Git section</p> }));
vi.mock("./TrustedFoldersSection", () => ({ TrustedFoldersSection: () => <p>Trusted</p> }));
vi.mock("./PermissionsSection", () => ({ PermissionsSection: () => <p>Permissions</p> }));
vi.mock("./CleanupSection", () => ({ CleanupSection: () => <p>Cleanup section</p> }));
vi.mock("./AgentBehaviorSection", () => ({ AgentBehaviorSection: () => <p>Agent behavior</p> }));
vi.mock("./NotificationsSection", () => ({ NotificationsSection: () => <p>Notifications</p> }));
vi.mock("./ConfigurationSection", () => ({ ConfigurationSection: () => <p>Configuration</p> }));

function mount(first: SettingsNavigation, width = 1440, host: HostFacts = MAC_HOST) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const show = (navigation: typeof first) => act(() => root.render(<PlatformProvider value={{ kind: "tauri", host }}><SettingsDialog navigation={navigation} onClose={() => {}} /></PlatformProvider>));
  taken.count = 0;
  document.documentElement.style.setProperty("--breakpoint-tablet", "768px");
  document.documentElement.style.setProperty("--breakpoint-phone", "390px");
  const listeners = new Set<() => void>();
  vi.stubGlobal("matchMedia", (media: string) => ({
    media,
    get matches() { return matchesWidthQuery(media, width); },
    addEventListener: (_: string, listener: () => void) => listeners.add(listener),
    removeEventListener: (_: string, listener: () => void) => listeners.delete(listener),
    addListener() {}, removeListener() {},
  }));
  const resize = (next: number) => act(async () => {
    width = next;
    for (const listener of [...listeners]) listener();
  });
  onTestFinished(() => {
    vi.unstubAllGlobals();
    document.documentElement.style.removeProperty("--breakpoint-tablet");
    document.documentElement.style.removeProperty("--breakpoint-phone");
  });
  show(first);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const selected = () => document.querySelector('[role="tab"][aria-selected="true"]');
  return { show, selected, resize };
}

it("keeps iPad portrait in horizontal navigation and switches above 768px without losing the selected section", async () => {
  const { selected, resize } = mount({ nonce: 1, section: "git", addAgent: false }, 768);
  const orientation = () => document.querySelector('[role="tablist"]')?.getAttribute("aria-orientation");
  expect(orientation()).toBe("horizontal");
  for (const width of [767, 768, 768.5, 769, 1024, 1440, 768]) {
    await resize(width);
    expect(orientation()).toBe(width <= 768 ? "horizontal" : "vertical");
    expect(selected()?.textContent).toContain("Git");
  }
});

it("opens on the section it is asked for and takes the Add agent request up once", async () => {
  const { selected } = mount({ nonce: 1, section: "accounts", addAgent: true });
  await act(async () => {});
  expect(selected()?.textContent).toContain("Agent accounts");
  expect(taken.count).toBe(1);
});

it("marks the selected tab as the dialog's opening focus", async () => {
  const { selected } = mount({ nonce: 1, section: "git", addAgent: false });
  await act(async () => {});
  expect([...document.querySelectorAll("[data-opening-focus]")]).toEqual([selected()]);
});

it("marks no tab when the Add agent dialog takes focus in its place", async () => {
  mount({ nonce: 1, section: "accounts", addAgent: true });
  await act(async () => {});
  expect(document.querySelector("[data-opening-focus]")).toBeNull();
});

it("follows a later request: another section is shown and its tab takes focus", async () => {
  framesAsTimers("pointer");
  const { show, selected } = mount({ nonce: 1, section: "accounts", addAgent: false });
  show({ nonce: 2, section: "git", addAgent: false });
  await settleFrames();
  expect(selected()?.textContent).toContain("Git");
  expect(document.activeElement?.textContent).toBe(selected()?.textContent);
});

it("opens the Add agent dialog again for a later request made while the accounts section is showing", async () => {
  const { show, selected } = mount({ nonce: 1, section: "accounts", addAgent: true });
  await act(async () => {});
  expect(taken.count).toBe(1);
  show({ nonce: 2, section: "accounts", addAgent: true });
  await act(async () => {});
  expect(selected()?.textContent).toContain("Agent accounts");
  expect(taken.count).toBe(2);
  // The same navigation again, with no new request, takes nothing up.
  show({ nonce: 2, section: "accounts", addAgent: true });
  await act(async () => {});
  expect(taken.count).toBe(2);
});

it("leaves the Add agent request for the accounts section to take up when another section is asked for first", async () => {
  const { show, selected } = mount({ nonce: 1, section: "git", addAgent: false });
  show({ nonce: 2, section: "accounts", addAgent: true });
  await act(async () => {});
  expect(selected()?.textContent).toContain("Agent accounts");
  expect(taken.count).toBe(1);
});

it("lists the Agent behavior section right after Agent accounts and opens on it", async () => {
  const { selected } = mount({ nonce: 1, section: "agent-permissions", addAgent: false });
  await act(async () => {});
  expect(selected()?.textContent).toContain("Agent behavior");
  const tabs = [...document.querySelectorAll('[role="tab"]')].map((tab) => tab.textContent);
  expect(tabs.indexOf("Agent behavior")).toBe(tabs.indexOf("Agent accounts") + 1);
  expect(tabs).toEqual(["General", "App behavior", "Git", "Agent accounts", "Agent behavior", "Trusted folders", "Permissions", "Notifications", "Keyboard shortcuts", "Cleanup", "Configuration", "About"]);
});

it.each([
  ["the macOS app", MAC_HOST, true],
  // To say there that they are managed in the macOS app.
  ["a browser on a Mac", browserHost("MacIntel"), true],
  ["the Windows app", WINDOWS_HOST, false],
  ["a browser on Windows", browserHost("Win32"), false],
] as const)("lists the Permissions section only where the system has privacy grants to give: %s", async (_name, host, listed) => {
  const tabs = () => [...document.querySelectorAll('[role="tab"]')].map((tab) => tab.textContent);
  mount({ nonce: 1, section: "general", addAgent: false }, 1440, host);
  await act(async () => {});
  expect(tabs().includes("Permissions")).toBe(listed);
  expect(tabs()).toContain("Notifications");
});

it("asks account forms to close for every later navigation request", async () => {
  const { show } = mount({ nonce: 1, section: "accounts", addAgent: true });
  await act(async () => {});
  expect(taken.dismiss).toBe(0);
  show({ nonce: 2, section: "accounts", addAgent: true });
  await act(async () => {});
  expect(taken.dismiss).toBe(1);
  show({ nonce: 3, section: "accounts", addAgent: false });
  await act(async () => {});
  expect(taken.dismiss).toBe(2);
});

it("waits for a modal still in front, such as the Add agent dialog being dismissed, before the new tab takes focus", async () => {
  act(() => setInteractionModality("pointer"));
  onTestFinished(() => { act(() => setInteractionModality("pointer")); });
  fakeTimers();
  const { show, selected } = mount({ nonce: 1, section: "accounts", addAgent: false });
  await act(async () => {});
  // What a modal in front of Settings does to it: makes it inert to assistive technology.
  const settings = document.querySelector('[role="dialog"]')!;
  settings.setAttribute("aria-hidden", "true");
  onTestFinished(() => settings.removeAttribute("aria-hidden"));
  runOutTimers();
  show({ nonce: 2, section: "git", addAgent: false });
  // Advance frames explicitly, so other pending timers cannot turn a step into less than a frame.
  // The tab tries on each frame and gives up after `MAX_FRAMES_TO_WAIT_FOR_MODAL` retries,
  // so it is still waiting one frame short of that.
  for (let frame = 0; frame < MAX_FRAMES_TO_WAIT_FOR_MODAL - 1; frame += 1) act(() => void vi.advanceTimersToNextFrame());
  expect(selected()?.textContent).toContain("Git");
  expect(document.activeElement).not.toBe(selected());
  settings.removeAttribute("aria-hidden");
  act(() => void vi.advanceTimersToNextFrame());
  expect(document.activeElement).toBe(selected());
});
