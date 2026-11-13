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
import type { UpdaterCapability } from "../platform";
import { PlatformProvider } from "../platform/react";
import type { DaemonUpdateStatus } from "../protocol";
import { DaemonProvider } from "../store";
import { testDaemon } from "../test/testDaemon";
import { UpdatesProvider } from "../updates/UpdatesProvider";
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
vi.mock("./AppearanceSection", () => ({ AppearanceSection: () => <p>Appearance section</p> }));
vi.mock("./GitSection", () => ({ GitSection: () => <p>Git section</p> }));
vi.mock("./TrustedFoldersSection", () => ({ TrustedFoldersSection: () => <p>Trusted</p> }));
vi.mock("./PermissionsSection", () => ({ PermissionsSection: () => <p>Permissions</p> }));
vi.mock("./CleanupSection", () => ({ CleanupSection: () => <p>Cleanup section</p> }));
vi.mock("./AgentBehaviorSection", () => ({ AgentBehaviorSection: () => <p>Agent behavior</p> }));
vi.mock("./NotificationsSection", () => ({ NotificationsSection: () => <p>Notifications</p> }));
vi.mock("./ConfigurationSection", () => ({ ConfigurationSection: () => <p>Configuration</p> }));

/** What makes something update: the desktop shell's updater, or a daemon that updates itself (`daemonUpdate`). */
interface Updating {
  updater?: UpdaterCapability;
  daemonUpdate?: DaemonUpdateStatus;
}

function mount(first: SettingsNavigation, width = 1440, host: HostFacts = MAC_HOST, updating: Updating = {}) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const daemon = updating.daemonUpdate && testDaemon({ state: { daemon: { version: __OCTOBOARD_VERSION__, generation: 1 }, daemonUpdate: updating.daemonUpdate } });
  const dialog = (navigation: typeof first) => <SettingsDialog navigation={navigation} onClose={() => {}} />;
  const show = (navigation: typeof first) =>
    act(() =>
      root.render(
        <PlatformProvider value={{ kind: "tauri", host, updater: updating.updater }}>
          {daemon ? <DaemonProvider value={daemon}><UpdatesProvider firstCheckDelayMs={null}>{dialog(navigation)}</UpdatesProvider></DaemonProvider> : updating.updater ? <UpdatesProvider firstCheckDelayMs={null}>{dialog(navigation)}</UpdatesProvider> : dialog(navigation)}
        </PlatformProvider>,
      ),
    );
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

const classes = (element: Element | null | undefined) => (element?.getAttribute("class") ?? "").split(/\s+/);
const list = () => document.querySelector<HTMLElement>('[role="tablist"]')!;
const container = () => list().closest<HTMLElement>(".tabs__list-container")!;
/** The wide menu column's decorative ground, whose end edge is the divider: aria-hidden, one step below the dialog. */
const grounds = () => [...document.querySelectorAll<HTMLElement>('[aria-hidden="true"].bg-surface-secondary')];
const searchField = () => document.querySelector<HTMLElement>(".search-field")!;
const indicators = () => document.querySelectorAll('[role="tablist"] [data-slot="tabs-indicator"]');

it("shows the wide sections as a menu column: no indicator in any tab, and a decorative ground behind it", async () => {
  mount({ nonce: 1, section: "git", addAgent: false }, 1440);
  await act(async () => {});
  expect(list().getAttribute("aria-orientation")).toBe("vertical");
  expect(document.querySelectorAll('[role="tab"]').length).toBeGreaterThan(5);
  expect(indicators()).toHaveLength(0);
  // Not the shared content-fit strip, which the narrow layout uses.
  expect(classes(container())).not.toContain("w-max");
  expect(classes(list())).not.toContain("w-max");
  for (const tab of document.querySelectorAll('[role="tab"]')) expect(classes(tab)).not.toContain("w-auto");
  expect(grounds()).toHaveLength(1);
  expect(grounds()[0]!.textContent).toBe("");
  expect(grounds()[0]!.getAttribute("role")).toBeNull();
  // The field is the white variant, standing on the darker ground.
  expect(classes(searchField())).toContain("search-field--primary");
  expect(classes(searchField())).not.toContain("search-field--secondary");
});

it("shows the narrow sections as the shared content-fit tab strip with HeroUI's indicator, no ground behind it, and the filled search field", async () => {
  const { selected } = mount({ nonce: 1, section: "git", addAgent: false }, 500);
  await act(async () => {});
  expect(list().getAttribute("aria-orientation")).toBe("horizontal");
  expect(classes(container())).toEqual(expect.arrayContaining(["w-max", "max-w-full"]));
  expect(classes(list())).toEqual(expect.arrayContaining(["w-max", "p-1"]));
  const tabs = [...document.querySelectorAll('[role="tab"]')];
  expect(tabs.length).toBeGreaterThan(5);
  for (const tab of tabs) {
    expect(classes(tab)).toEqual(expect.arrayContaining(["w-auto", "flex-none"]));
  }
  // HeroUI keeps one indicator, in the selected tab, and slides it.
  expect(indicators()).toHaveLength(1);
  expect(selected()!.querySelector('[data-slot="tabs-indicator"]')).not.toBeNull();
  expect(selected()?.textContent).toContain("Git");
  expect(grounds()).toHaveLength(0);
  expect(classes(searchField())).toContain("search-field--secondary");
  expect(classes(searchField())).not.toContain("search-field--primary");
});

it("swaps the menu column and the shared strip when the window crosses the tablet breakpoint, keeping the selected section", async () => {
  const { selected, resize } = mount({ nonce: 1, section: "git", addAgent: false }, 1440);
  await act(async () => {});
  /** What the swap flips: HeroUI's indicator, the column's ground, the field's variant and the strip's fit to its tabs. */
  const layout = () => ({
    indicators: indicators().length,
    grounds: grounds().length,
    field: classes(searchField()).find((name) => /^search-field--(primary|secondary)$/.test(name)),
    stripFit: classes(list()).includes("w-max"),
  });
  const wide = { indicators: 0, grounds: 1, field: "search-field--primary", stripFit: false };
  const narrow = { indicators: 1, grounds: 0, field: "search-field--secondary", stripFit: true };
  expect(layout()).toEqual(wide);
  await resize(500);
  expect(layout()).toEqual(narrow);
  expect(selected()?.textContent).toContain("Git");
  await resize(1440);
  expect(layout()).toEqual(wide);
  expect(selected()?.textContent).toContain("Git");
});

const dialog = () => document.querySelector<HTMLElement>('[role="dialog"]')!;

it.each([[1440], [500]])("carries the raised fills on the dialog, which the wide menu's field and selected row read their fills from, at %ipx", async (width) => {
  mount({ nonce: 1, section: "git", addAgent: false }, width);
  await act(async () => {});
  expect(classes(dialog())).toEqual(expect.arrayContaining(["control-fills", "raised-fills"]));
});

it("fills the wide menu's search field from the dialog's raised fill and its selected row from the dialog's selected fill, and neither the narrow field nor the strip uses either", async () => {
  const { selected, resize } = mount({ nonce: 1, section: "git", addAgent: false }, 1440);
  await act(async () => {});
  const fill = (element: Element | null | undefined, name: string) => (element?.getAttribute("class") ?? "").includes(name);
  expect(fill(searchField(), "--menu-raised")).toBe(true);
  expect(fill(searchField(), "--menu-selected")).toBe(false);
  expect(fill(selected(), "--menu-selected")).toBe(true);
  expect(fill(selected(), "--menu-raised")).toBe(false);
  await resize(500);
  expect(fill(searchField(), "--menu-raised")).toBe(false);
  expect(fill(searchField(), "--menu-selected")).toBe(false);
  expect(fill(selected(), "--menu-raised")).toBe(false);
  expect(fill(selected(), "--menu-selected")).toBe(false);
});

it("draws the wide column's divider as the ground's groove, not as a border", async () => {
  mount({ nonce: 1, section: "git", addAgent: false }, 1440);
  await act(async () => {});
  expect(classes(grounds()[0])).toContain("groove-e");
  expect(classes(grounds()[0]).filter((name) => name.startsWith("border"))).toEqual([]);
});

// jsdom loads no stylesheet, so the geometry itself is out of reach; these are the hooks it is built from.
it("lets the wide column's list scroll within its own stretched container, with HeroUI's scroll buttons hidden there", async () => {
  mount({ nonce: 1, section: "git", addAgent: false }, 1440);
  await act(async () => {});
  const wideContainer = classes(container());
  // Stretches over its grid row, and may shrink below its list's height so that the list scrolls rather than the dialog.
  expect(wideContainer).toContain("tablet:self-stretch");
  expect(wideContainer).toContain("tablet:min-h-0");
  expect(wideContainer).not.toContain("tablet:self-start");
  // HeroUI's scroll buttons are the container's own buttons (the start and the end one), so hiding the container's
  // direct buttons hides exactly those two and nothing else.
  expect(wideContainer).toContain("tablet:[&>button]:hidden");
  const directButtons = [...container().querySelectorAll(":scope > button")];
  expect(directButtons.flatMap((button) => classes(button).filter((name) => name.startsWith("tabs__list-container__scroll-"))).sort()).toEqual([
    "tabs__list-container__scroll-next",
    "tabs__list-container__scroll-prev",
  ]);
  expect(directButtons).toHaveLength(2);
});

it("sets the narrow strip's start margin and width cap from one inset, so that its end clears the dialog's edge by the margin at every narrow width", async () => {
  mount({ nonce: 1, section: "git", addAgent: false }, 500);
  await act(async () => {});
  const stripClasses = classes(container());
  const insets = stripClasses.filter((name) => /^(max-phone:)?\[--strip-inset:/.test(name));
  // The inset is defined once for the narrow range and once, smaller, at phone width.
  expect(insets).toHaveLength(2);
  expect(insets.some((name) => name.startsWith("max-phone:"))).toBe(true);
  const margin = stripClasses.filter((name) => /^ms-/.test(name));
  const cap = stripClasses.filter((name) => /^max-tablet:max-w-/.test(name));
  expect(margin).toHaveLength(1);
  expect(cap).toHaveLength(1);
  expect(margin[0]).toContain("--strip-inset");
  expect(cap[0]).toContain("--strip-inset");
  // No margin or cap fixed on its own, which would drift apart from the inset at one width.
  expect(stripClasses.filter((name) => /^(max-phone:)?(ms-|max-tablet:max-w-|max-phone:max-w-)/.test(name)).sort()).toEqual([...margin, ...cap].sort());
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

it("lists Agent behavior before Agent accounts and Git and opens on it", async () => {
  const { selected } = mount({ nonce: 1, section: "agent-permissions", addAgent: false });
  await act(async () => {});
  expect(selected()?.textContent).toContain("Agent behavior");
  const tabs = [...document.querySelectorAll('[role="tab"]')].map((tab) => tab.textContent);
  expect(tabs.indexOf("Agent behavior")).toBe(tabs.indexOf("Appearance") + 1);
  expect(tabs).toEqual(["General", "Appearance", "Agent behavior", "Agent accounts", "Git", "Trusted folders", "Permissions", "Notifications", "Keyboard shortcuts", "Cleanup", "Configuration", "About"]);
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

it.each([
  ["the desktop application", { updater: { currentVersion: async () => "0.1.0", check: async () => undefined, install: async () => {} } }, true],
  ["a daemon that updates itself", { daemonUpdate: { phase: "idle", available: null, progress: null, installed: null, last_check: null, install: null, manual: null, updated_from: null } }, true],
  ["nothing that updates", {}, false],
] as const)("lists the Updates section just above About only where something updates: %s", async (_name, updating, listed) => {
  mount({ nonce: 1, section: "general", addAgent: false }, 1440, MAC_HOST, updating as Updating);
  await act(async () => {});
  const tabs = [...document.querySelectorAll('[role="tab"]')].map((tab) => tab.textContent);
  expect(tabs.includes("Updates")).toBe(listed);
  if (listed) expect(tabs.indexOf("Updates")).toBe(tabs.indexOf("About") - 1);
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
