// @vitest-environment jsdom
// The Remote devices page in the application's own window, through the paths that cross `App`'s own state: the rail's
// button, what the page takes the place of, how it is left, where focus goes, the narrow window's drawer, and what the
// shell hands the page.
import React, { act } from "react";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "./components/alerts/errorAlerts";
import { framesAsTimers, settleFrames } from "./components/dialog/dialogTestSupport";
import { App } from "./App";
import { forgetStoredBrowsersOtherThan, setProjectBrowserMode } from "./project-browser/browserPreferences";
import { PREFERENCE_KEYS } from "./preferences/preferenceKeys";
import type { State } from "./store";
import { fakeDaemon, mountApp, pressElement, railConsole, regionOfFocus, row, stubWindow } from "./appTestSupport";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./fixtures/builders";
import { remoteDevicePreference } from "./remoteDaemons/remoteDevicePreference";
import type { PlatformAdapter, RemoteDaemonConnectOutcome, RemoteDaemonEntry, RemoteDaemonHandedOutcome } from "./platform";
import { acknowledgeAllAlerts } from "./test/alertQueue";
import { BROWSER_HOST, MAC_HOST } from "./test/hosts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
// Each test mounts the whole App, which a machine under load can take longer than the default 5s for.
vi.setConfig({ testTimeout: 20_000 });

vi.mock("./report/ReportPanel", () => ({ ReportPanel: () => null }));
// The menu bar icon's menu is drawn with a canvas, which jsdom has none of.
vi.mock("./statusItem/statusItemIcons", () => ({ withIcons: async (draft: object) => ({ ...draft, icons: {} }) }));
vi.mock("./project-browser/LazyProjectBrowser", () => ({ LazyProjectBrowser: () => null }));
// The agent terminal's input stands in for xterm's.
vi.mock("./terminal/TerminalController", () => import("./terminalControllerTestSupport"));

const main = consoleOf("c-main", "Main");
const second = consoleOf("c-second", "Second");
const site = projectOf("p-site", main.id, "Website");
const fix = sessionOf("s-fix", main.id, site.id, "Fix header", "idle");
const needsYou = sessionOf("s-wait", second.id, undefined, "Needs an answer", "waiting_user");

const KEYED: RemoteDaemonEntry = {
  id: "keyed",
  name: "Keyed",
  host: "keyed.example.com",
  port: 22,
  user: "deploy",
  auth: "key",
  keyPath: "~/.ssh/id_ed25519",
  fingerprint: null,
  octoboarddPath: null,
};

const SECOND: RemoteDaemonEntry = { ...KEYED, id: "second", name: "Second box", host: "second.example.com" };
/** The devices the shell holds, which a test changes and `devicesChanged` tells the window of. */
let saved: RemoteDaemonEntry[] = [];
const listRead = vi.fn(async (): Promise<RemoteDaemonEntry[]> => saved);
const changeListeners = new Set<() => void>();
let handOver: ((items: RemoteDaemonHandedOutcome[]) => void) | undefined;
let askForPage: (() => void) | undefined;
const connect = vi.fn(async (_request: unknown): Promise<RemoteDaemonConnectOutcome> => ({ outcome: "opened" }));
beforeEach(() => {
  saved = [KEYED];
  listRead.mockReset().mockImplementation(async () => saved);
  changeListeners.clear();
  remoteDevicePreference.set(undefined);
  handOver = undefined;
  askForPage = undefined;
  connect.mockReset().mockResolvedValue({ outcome: "opened" });
  acknowledgeAllAlerts();
});

const DESKTOP: PlatformAdapter = {
  kind: "tauri",
  host: MAC_HOST,
  remoteDaemons: {
    list: () => listRead(),
    status: async () => [],
    onChanged: (listener) => {
      changeListeners.add(listener);
      return () => void changeListeners.delete(listener);
    },
    save: async (entry) => ({ ...entry, id: entry.id ?? "new" }),
    remove: async () => {},
    connect: (request) => connect(request),
    disconnect: async () => {},
    onPageRequested: (listener) => {
      askForPage = listener;
      return () => {};
    },
    onHandedOver: (listener) => {
      handOver = listener;
      return () => {};
    },
    showMainWindow: async () => {},
    showUpdates: async () => {},
  },
};

/** The window over two consoles, Main current with a session selected, in the desktop application's own window, `width`
 * px wide; `platform` is the browser's where given. */
async function mountDesktop(width = 1400, platform: PlatformAdapter = DESKTOP, state: Parameters<typeof snapshotState>[0] = { consoles: [main, second], projects: [site], sessions: [fix, needsYou] }) {
  stubWindow(width);
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  const daemon = fakeDaemon(snapshotState(state));
  mountApp(daemon, { platform });
  if (state.sessions?.includes(fix) && width >= 768) await pressElement(row(fix.title));
  return daemon;
}

const remoteButton = () => document.querySelector<HTMLElement>("[data-region=rail] button[aria-label^='Remote devices']");
const modal = () => [...document.body.querySelectorAll("[role=dialog], [role=alertdialog]")].at(-1);
const page = () => document.querySelector<HTMLElement>("[data-region=devices]");
const agentInput = () => document.querySelector<HTMLTextAreaElement>("textarea[aria-label='Agent terminal']");
const topBar = () => document.querySelector<HTMLElement>("[data-region=topbar]")!;
const isCurrent = (element: Element | null | undefined) => element?.getAttribute("aria-current") === "true";
/** Whether `element` is out of the layout, as what the page takes the place of is: kept mounted, `display: none`. */
const hidden = (element: Element | null | undefined) => element?.closest(".hidden") != null;
const showPage = () => pressElement(remoteButton());
/** Leaves the page for the current console's own page, by its avatar. */
const leavePage = () => pressElement(railConsole(main.name));
const pressF6 = () => act(async () => void document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "F6", bubbles: true, cancelable: true })));

it("has no Remote devices button and no page in a browser", async () => {
  await mountDesktop(1400, { kind: "browser", host: BROWSER_HOST });
  expect(remoteButton()).toBeNull();
  expect(page()).toBeNull();
});

it("has no page until the button is pressed, and then shows it in place of the sessions, keeping the terminal mounted", async () => {
  await mountDesktop();
  expect(page()).toBeNull();
  expect(isCurrent(remoteButton())).toBe(false);
  expect(hidden(document.querySelector("[data-region=sidebar]"))).toBe(false);
  expect(hidden(document.querySelector("[data-region=terminal]"))).toBe(false);
  const input = agentInput()!;
  expect(input).not.toBeNull();

  await showPage();
  expect(page()).not.toBeNull();
  expect([...page()!.querySelectorAll("h1")].map((heading) => heading.textContent)).toEqual(["Keyed"]);
  // The sidebar stays, listing the devices in place of the sessions.
  expect(hidden(document.querySelector("[data-region=sidebar]"))).toBe(false);
  expect(document.querySelector("[data-region=sidebar]")!.getAttribute("aria-label")).toBe("Remote devices");
  expect(hidden(document.querySelector("[data-region=terminal]"))).toBe(true);
  // Hidden, not unmounted: the very same terminal is there when the page is left.
  expect(agentInput()).toBe(input);
  await leavePage();
  expect(page()).toBeNull();
  expect(agentInput()).toBe(input);
  expect(hidden(document.querySelector("[data-region=terminal]"))).toBe(false);
});

it("casts the docked sidebar's edge shadow from every content area's main alike, the sessions' and the page's", async () => {
  await mountDesktop();
  const mains = () => [...document.querySelectorAll("main")];
  const shadowed = (main: Element) => main.classList.contains("pane-docked:shadow-(--panel-shadow)") && main.classList.contains("relative");
  expect(mains().length).toBeGreaterThan(0);
  expect(mains().every(shadowed)).toBe(true);
  await showPage();
  expect(mains().length).toBeGreaterThan(1);
  expect(mains().every(shadowed)).toBe(true);
  expect(shadowed(page()!)).toBe(true);
});

it("marks the button as the current item while the page shows, and no console's avatar", async () => {
  await mountDesktop();
  expect(isCurrent(railConsole(main.name))).toBe(true);
  await showPage();
  expect(isCurrent(remoteButton())).toBe(true);
  expect(isCurrent(railConsole(main.name))).toBe(false);
  expect(isCurrent(railConsole(second.name))).toBe(false);
  await leavePage();
  expect(isCurrent(remoteButton())).toBe(false);
  expect(isCurrent(railConsole(main.name))).toBe(true);
});

it("leaves the page for the console whose avatar is chosen, selecting it", async () => {
  await mountDesktop();
  await showPage();
  await pressElement(railConsole(second.name));
  expect(page()).toBeNull();
  expect(isCurrent(railConsole(second.name))).toBe(true);
  expect(isCurrent(remoteButton())).toBe(false);
});

it("leaves the page for the current console too when its own avatar is chosen", async () => {
  await mountDesktop();
  await showPage();
  await pressElement(railConsole(main.name));
  expect(page()).toBeNull();
  expect(isCurrent(railConsole(main.name))).toBe(true);
  expect(topBar().textContent).toContain(fix.title);
});

it("leaves the page when the selected session changes, as the next waiting session does", async () => {
  await mountDesktop();
  await showPage();
  const waiting = document.querySelector<HTMLElement>("[data-region=rail] button[aria-label*='waiting for you']");
  expect(waiting).not.toBeNull();
  await pressElement(waiting);
  expect(page()).toBeNull();
  expect(topBar().textContent).toContain(needsYou.title);
  expect(isCurrent(railConsole(second.name))).toBe(true);
});

it("moves focus into the page when it opens", async () => {
  await mountDesktop();
  await showPage();
  expect(document.activeElement).not.toBe(remoteButton());
  expect(page()!.contains(document.activeElement)).toBe(true);
});

it("is a stop of F6 after the devices sidebar, and the cycle skips what the page hides", async () => {
  await mountDesktop();
  await showPage();
  act(() => remoteButton()!.focus());
  expect(regionOfFocus()).toBe("rail");
  await pressF6();
  expect(regionOfFocus()).toBe("sidebar");
  await pressF6();
  expect(regionOfFocus()).toBe("devices");
  // On from the page the cycle wraps round: the sidebar lists the devices, and the terminal is not a stop while the page takes its place.
  for (let stop = 0; stop < 5; stop++) {
    await pressF6();
    expect(["topbar", "rail", "sidebar", "devices"]).toContain(regionOfFocus());
  }
});

it("shows the page at the shell's request, and when something is handed to it, though it was never opened", async () => {
  await mountDesktop();
  expect(page()).toBeNull();
  await act(async () => askForPage!());
  expect(page()).not.toBeNull();
  await leavePage();
  expect(page()).toBeNull();

  await act(async () => handOver!([{ id: KEYED.id, outcome: { outcome: "no_daemon" } }]));
  expect(page()).not.toBeNull();
  expect(modal()!.textContent).toContain("No daemon is running on");
});

it("brings the page back for a failure of a connection started on it after it was left, and not for one that opened", async () => {
  await mountDesktop();
  await showPage();
  let finish!: (outcome: RemoteDaemonConnectOutcome) => void;
  connect.mockImplementationOnce(() => new Promise((resolve) => (finish = resolve)));
  const connectButton = () => Array.from(page()!.querySelectorAll<HTMLButtonElement>("button")).find((button) => button.getAttribute("aria-label")?.replace(/[⁨⁩]/g, "") === "Connect to Keyed");
  await pressElement(connectButton());
  // Left while the connection is under way: the connection goes on with the page hidden.
  await leavePage();
  expect(page()).toBeNull();
  await act(async () => finish({ outcome: "opened" }));
  expect(page()).toBeNull();
  expect(errorAlerts.getSnapshot()).toEqual([]);

  await showPage();
  connect.mockImplementationOnce(() => new Promise((resolve) => (finish = resolve)));
  await pressElement(connectButton());
  await leavePage();
  await act(async () => finish({ outcome: "auth_failed", detail: "denied" }));
  expect(page()).not.toBeNull();
  expect(errorAlerts.getSnapshot()[0].title).toBe("Could not sign in");
});


const devicesChanged = () => act(async () => changeListeners.forEach((listener) => listener()));
const pressBack = () => pressElement(topBarButton("Back"));
const pressForward = () => pressElement(topBarButton("Forward"));
const topBarButton = (label: string) => topBar().querySelector<HTMLElement>(`[aria-label="${label}"]`);
const heading = () => page()?.querySelector("h1")?.textContent;
const sidebarRows = () => [...document.querySelectorAll<HTMLElement>("[data-region=sidebar] [data-list-motion] [role=button]")];
const deviceRow = (name: string) => sidebarRows().find((one) => one.textContent?.startsWith(name));
const field = (label: string) => [...document.querySelectorAll<HTMLInputElement>("input")].find((input) => input.labels?.[0]?.textContent === label)!;
const setValue = (input: HTMLInputElement, value: string) => act(async () => {
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
  input.dispatchEvent(new Event("input", { bubbles: true }));
});
const pressEscape = () => act(async () => void document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })));

it("only opens the page from the rail's button: pressing it on the page does nothing", async () => {
  await mountDesktop();
  await showPage();
  await showPage();
  expect(page()).not.toBeNull();
  expect(isCurrent(remoteButton())).toBe(true);
  expect(heading()).toBe("Keyed");
});

it("takes no Escape: the page stays, with focus where it was", async () => {
  await mountDesktop();
  await showPage();
  const focused = document.activeElement;
  await pressEscape();
  expect(page()).not.toBeNull();
  expect(document.activeElement).toBe(focused);
});

it("keeps Back, Forward and the sidebar's toggle in the top bar, and names the page and the device in the breadcrumb", async () => {
  await mountDesktop();
  await showPage();
  expect(topBarButton("Back")).not.toBeNull();
  expect(topBarButton("Forward")).not.toBeNull();
  expect(topBarButton("Hide remote devices")).not.toBeNull();
  const trail = topBar().textContent!;
  expect(trail).toContain("Remote devices");
  expect(trail).toContain("Keyed");
  expect(trail).not.toContain(fix.title);
});

it("names only the page in the breadcrumb while a new device is being added, and while none is saved", async () => {
  await mountDesktop();
  await showPage();
  await pressElement(document.querySelector("[data-region=sidebar] [aria-label='Add remote daemon']") as HTMLElement);
  expect(heading()).toBe("Add remote daemon");
  expect(topBar().textContent).toContain("Remote devices");
  expect(topBar().textContent).not.toContain("Keyed");
});

it("names only the page in the breadcrumb when no device is saved", async () => {
  saved = [];
  await mountDesktop();
  await showPage();
  expect(topBar().textContent).toContain("Remote devices");
  expect(heading()).toBe("No remote daemons yet");
});

it("lists the devices in the sidebar in place of the sessions, which come back when the page is left", async () => {
  saved = [KEYED, SECOND];
  await mountDesktop();
  expect(row(fix.title)).toBeDefined();
  await showPage();
  expect(row(fix.title)).toBeUndefined();
  expect(sidebarRows().map((one) => one.textContent)).toEqual(["Keyed", "Second box"]);
  expect(sidebarRows().filter((one) => one.getAttribute("aria-current") === "true").map((one) => one.textContent)).toEqual(["Keyed"]);
  await leavePage();
  expect(row(fix.title)).toBeDefined();
  expect(deviceRow("Keyed")).toBeUndefined();
});

it("enters the page as a step of Back and Forward: Back returns to the session, Forward to the page", async () => {
  await mountDesktop();
  await showPage();
  expect(page()).not.toBeNull();
  await pressBack();
  expect(page()).toBeNull();
  expect(topBar().textContent).toContain(fix.title);
  expect(isCurrent(railConsole(main.name))).toBe(true);
  await pressForward();
  expect(page()).not.toBeNull();
  expect(heading()).toBe("Keyed");
  expect(isCurrent(remoteButton())).toBe(true);
});

it("makes choosing a device a step of its own: Back returns to the device before, then to the session", async () => {
  saved = [KEYED, SECOND];
  await mountDesktop();
  await showPage();
  await pressElement(deviceRow("Second box"));
  expect(heading()).toBe("Second box");
  await pressBack();
  expect(heading()).toBe("Keyed");
  expect(deviceRow("Keyed")!.getAttribute("aria-current")).toBe("true");
  await pressBack();
  expect(page()).toBeNull();
  expect(topBar().textContent).toContain(fix.title);
  await pressForward();
  await pressForward();
  expect(heading()).toBe("Second box");
});

it("passes over a device that is no longer saved when moving Back, to the one before it", async () => {
  const third = { ...KEYED, id: "third", name: "Third box" };
  saved = [KEYED, SECOND, third];
  await mountDesktop();
  await showPage();
  await pressElement(deviceRow("Second box"));
  await pressElement(deviceRow("Third box"));
  await leavePage();
  saved = [KEYED, SECOND];
  await devicesChanged();
  await pressBack();
  expect(page()).not.toBeNull();
  expect(heading()).toBe("Second box");
});

it("opens on the device last viewed while it is still saved, else on the first", async () => {
  saved = [KEYED, SECOND];
  await mountDesktop();
  await showPage();
  expect(heading()).toBe("Keyed");
  await pressElement(deviceRow("Second box"));
  expect(remoteDevicePreference.get()).toBe(SECOND.id);
  await leavePage();
  await showPage();
  expect(heading()).toBe("Second box");
});

it("takes the device last viewed from the preference at start, and the first where that is no longer saved", async () => {
  saved = [KEYED, SECOND];
  remoteDevicePreference.set(SECOND.id);
  await mountDesktop();
  await showPage();
  expect(heading()).toBe("Second box");
  await leavePage();
  saved = [KEYED];
  await devicesChanged();
  await showPage();
  expect(heading()).toBe("Keyed");
});

it("records the device a move of Back shows as the one last viewed", async () => {
  saved = [KEYED, SECOND];
  await mountDesktop();
  await showPage();
  await pressElement(deviceRow("Second box"));
  await pressBack();
  expect(remoteDevicePreference.get()).toBe(KEYED.id);
});

it("selects the device a hand-over is for, and shows its question over the page", async () => {
  saved = [KEYED, SECOND];
  await mountDesktop();
  await act(async () => handOver!([{ id: SECOND.id, outcome: { outcome: "no_daemon" } }]));
  expect(page()).not.toBeNull();
  expect(heading()).toBe("Second box");
  expect(modal()!.textContent).toContain("No daemon is running on");
});

it("drops what was changed and not saved, and a new device's form, when the page is left", async () => {
  await mountDesktop();
  await showPage();
  await setValue(field("User"), "ops");
  await leavePage();
  await showPage();
  expect(field("User").value).toBe("deploy");

  await pressElement(document.querySelector("[data-region=sidebar] [aria-label='Add remote daemon']") as HTMLElement);
  expect(heading()).toBe("Add remote daemon");
  await leavePage();
  await showPage();
  expect(heading()).toBe("Keyed");
});

it("leaves the page for a console's avatar even while a new device's form is open", async () => {
  await mountDesktop();
  await showPage();
  await pressElement(document.querySelector("[data-region=sidebar] [aria-label='Add remote daemon']") as HTMLElement);
  await pressElement(railConsole(second.name));
  expect(page()).toBeNull();
  expect(isCurrent(railConsole(second.name))).toBe(true);
});

const noConsoles = { consoles: [], projects: [], sessions: [] };
const welcome = () => document.querySelector("[data-region=welcome]");
const sidebar = () => document.querySelector("[data-region=sidebar]");

it("shows the welcome screen of a window with no console as before, with no sidebar, until the page is opened", async () => {
  await mountDesktop(1400, DESKTOP, noConsoles);
  expect(welcome()).not.toBeNull();
  expect(sidebar()).toBeNull();
  expect(page()).toBeNull();
});

it("gives the page of a window with no console its device sidebar in place of the welcome screen, and the welcome screen again once it is left", async () => {
  saved = [KEYED, SECOND];
  await mountDesktop(1400, DESKTOP, noConsoles);
  await showPage();
  expect(page()).not.toBeNull();
  expect(welcome()).toBeNull();
  expect(sidebar()!.getAttribute("aria-label")).toBe("Remote devices");
  expect(sidebar()!.textContent).toContain("Remote devices");
  expect(sidebar()!.querySelector("[aria-label='Add remote daemon']")).not.toBeNull();
  expect(sidebarRows().map((one) => one.textContent)).toEqual(["Keyed", "Second box"]);
  // The list switches devices.
  await pressElement(deviceRow("Second box"));
  expect(heading()).toBe("Second box");
  expect(deviceRow("Second box")!.getAttribute("aria-current")).toBe("true");
  // Back leaves the page, as there is no console's avatar to leave it by: first to the device before, then off the page.
  await pressBack();
  expect(heading()).toBe("Keyed");
  await pressBack();
  expect(page()).toBeNull();
  expect(sidebar()).toBeNull();
  expect(welcome()).not.toBeNull();
});

it("gives a window with no console and no device saved the compact empty row, which starts Add", async () => {
  saved = [];
  await mountDesktop(1400, DESKTOP, noConsoles);
  await showPage();
  expect(heading()).toBe("No remote daemons yet");
  const emptyRow = [...sidebar()!.querySelectorAll<HTMLElement>("button")].find((one) => one.textContent?.includes("No remote daemons yet"))!;
  expect(emptyRow).toBeDefined();
  await pressElement(emptyRow);
  expect(heading()).toBe("Add remote daemon");
  expect([...sidebar()!.querySelectorAll("button")].some((one) => one.textContent?.includes("No remote daemons yet"))).toBe(true);
});

it("docks, hides and shows the device sidebar of a window with no console as it does a console's", async () => {
  await mountDesktop(1400, DESKTOP, noConsoles);
  await showPage();
  expect(topBarButton("Hide remote devices")).not.toBeNull();
  await pressElement(topBarButton("Hide remote devices"));
  expect(topBarButton("Show remote devices")).not.toBeNull();
  await pressElement(topBarButton("Show remote devices"));
  expect(topBarButton("Hide remote devices")).not.toBeNull();
});

it("has the device sidebar of a narrow window with no console as a drawer", async () => {
  saved = [KEYED, SECOND];
  await mountDesktop(390, DESKTOP, noConsoles);
  await showPage();
  expect(page()).not.toBeNull();
  await openDrawer();
  expect(sidebarRows().map((one) => one.textContent)).toEqual(["Keyed", "Second box"]);
  await pressElement(deviceRow("Second box"));
  expect(heading()).toBe("Second box");
  expect(topBarButton("Show remote devices")).not.toBeNull();
});

/** Has the narrow window's drawer open, whether its toggle is named for the sessions or for the devices. */
async function openDrawer() {
  const show = topBarButton("Show sessions") ?? topBarButton("Show remote devices");
  if (show) await pressElement(show);
  expect(topBarButton("Hide sessions") ?? topBarButton("Hide remote devices")).not.toBeNull();
}

it("leaves a narrow window's open drawer open when the rail's button inside it is pressed, now listing the devices, and closes it when one is chosen", async () => {
  saved = [KEYED, SECOND];
  await mountDesktop(390);
  await openDrawer();
  // The drawer lists the console's projects until the page is opened.
  expect(sidebarRows().some((one) => one.textContent?.includes("Website"))).toBe(true);
  await pressElement(remoteButton());
  expect(page()).not.toBeNull();
  expect(topBarButton("Hide remote devices")).not.toBeNull();
  expect(sidebarRows().map((one) => one.textContent)).toEqual(["Keyed", "Second box"]);
  // Back, Forward and the sidebar's toggle stay in the top bar.
  expect(topBarButton("Back")).not.toBeNull();
  expect(topBarButton("Forward")).not.toBeNull();
  await pressElement(deviceRow("Second box"));
  expect(heading()).toBe("Second box");
  expect(topBarButton("Show remote devices")).not.toBeNull();
  expect(topBarButton("Hide remote devices")).toBeNull();
});

it("closes a narrow window's drawer when a new device's form is asked for", async () => {
  await mountDesktop(390);
  await openDrawer();
  await pressElement(remoteButton());
  expect(topBarButton("Hide remote devices")).not.toBeNull();
  await pressElement(document.querySelector("[data-region=sidebar] [aria-label='Add remote daemon']") as HTMLElement);
  expect(heading()).toBe("Add remote daemon");
  expect(topBarButton("Show remote devices")).not.toBeNull();
  expect(topBarButton("Hide remote devices")).toBeNull();
});

const pressFocusKey = (code: "ArrowRight" | "ArrowLeft") => act(async () => void window.dispatchEvent(new KeyboardEvent("keydown", { code, key: code, altKey: true, metaKey: true, ctrlKey: false, bubbles: true, cancelable: true })));
const addButton = () => document.querySelector<HTMLElement>("[data-region=sidebar] [aria-label='Add remote daemon']");

it("reads the list of devices once at start, and raises one alert where that read fails", async () => {
  listRead.mockRejectedValueOnce(new Error("store unreadable"));
  await mountDesktop();
  expect(listRead).toHaveBeenCalledTimes(1);
  expect(errorAlerts.getSnapshot().map((alert) => alert.title)).toEqual(["Could not read the remote daemons"]);
});

it("makes saving a new device one step of Back and Forward: Back returns to the device before it", async () => {
  await mountDesktop();
  await showPage();
  await pressElement(addButton());
  await setValue(field("Host"), "box.example.com");
  await setValue(field("User"), "me");
  saved = [KEYED, { ...KEYED, id: "new", name: "box.example.com", host: "box.example.com", user: "me", auth: "password", keyPath: null }];
  await pressElement([...page()!.querySelectorAll<HTMLElement>("button")].find((one) => one.textContent?.trim() === "Save"));
  expect(heading()).toBe("box.example.com");
  expect(remoteDevicePreference.get()).toBe("new");
  await pressBack();
  expect(heading()).toBe("Keyed");
  await pressBack();
  expect(page()).toBeNull();
  expect(topBar().textContent).toContain(fix.title);
});

it("records no visit for the page while its list is not read yet, so Back leaves the page in one step", async () => {
  let answer!: (entries: RemoteDaemonEntry[]) => void;
  await mountDesktop();
  listRead.mockImplementationOnce(() => new Promise((resolve) => (answer = resolve)));
  await act(async () => changeListeners.forEach((listener) => listener()));
  await showPage();
  await act(async () => answer([KEYED]));
  expect(heading()).toBe("Keyed");
  await pressBack();
  expect(page()).toBeNull();
  expect(topBar().textContent).toContain(fix.title);
});

it("does nothing for the focus mode shortcuts on the page", async () => {
  await mountDesktop();
  // The same shortcuts enter and leave the focus mode of the selected session off the page.
  await pressFocusKey("ArrowRight");
  expect(document.querySelector("[data-focus-exit]")).not.toBeNull();
  await pressFocusKey("ArrowLeft");
  expect(document.querySelector("[data-focus-exit]")).toBeNull();
  await showPage();
  await pressFocusKey("ArrowRight");
  expect(document.querySelector("[data-focus-exit]")).toBeNull();
  expect(page()).not.toBeNull();
  await leavePage();
  expect(document.querySelector("[data-focus-exit]")).toBeNull();
});

it("lands the focus a closed dialog leaves behind in the page while it shows, not in the terminal under it", async () => {
  await mountDesktop();
  framesAsTimers();
  await showPage();
  await act(async () => handOver!([{ id: KEYED.id, outcome: { outcome: "no_daemon" } }]));
  expect(modal()).toBeDefined();
  await pressElement([...modal()!.querySelectorAll<HTMLElement>("button")].find((one) => one.textContent?.trim() === "Cancel"));
  expect(modal()).toBeUndefined();
  await settleFrames();
  expect(page()!.contains(document.activeElement)).toBe(true);
  expect(document.activeElement).not.toBe(agentInput());
});

it("names the drawer's scrim for the devices while they are listed, and for the sessions otherwise", async () => {
  await mountDesktop(390);
  const scrim = (name: string) => document.querySelector(`[aria-label="${name}"]`);
  await openDrawer();
  expect(scrim("Close sessions")).not.toBeNull();
  expect(scrim("Close remote devices")).toBeNull();
  await pressElement(remoteButton());
  expect(topBarButton("Hide remote devices")).not.toBeNull();
  expect(scrim("Close remote devices")).not.toBeNull();
  expect(scrim("Close sessions")).toBeNull();
});

it("leaves focus out of the page while the narrow window's drawer is open over it, and lands it once the drawer closes", async () => {
  saved = [KEYED, SECOND];
  await mountDesktop(390);
  await openDrawer();
  await pressElement(remoteButton());
  expect(page()).not.toBeNull();
  expect(page()!.contains(document.activeElement)).toBe(false);
  await pressElement(deviceRow("Second box"));
  expect(heading()).toBe("Second box");
  expect(page()!.contains(document.activeElement)).toBe(true);
});

// What the window shows while the local daemon cannot be used. A daemon that has not answered yet has no snapshot (no
// hosts), and the connection says why it has none.
const OFFLINE_STATES: [string, Partial<State>, string][] = [
  ["connecting", { connectionState: "connecting" }, "Connecting to the daemon…"],
  ["not answering", { connectionState: "closed" }, "The daemon is not answering."],
  ["of another protocol generation", { connectionState: "incompatible" }, "The app and its daemon are different versions"],
  ["refusing the page's key", { connectionState: "keyRefused" }, "This address no longer opens the daemon"],
];

function mountOffline(state: Partial<State>, { startupScreen, platform = DESKTOP, width = 1400 }: { startupScreen?: React.ReactNode; platform?: PlatformAdapter; width?: number } = {}) {
  stubWindow(width);
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  const daemon = fakeDaemon(state);
  mountApp(daemon, { platform, wrap: (app) => (startupScreen === undefined ? app : <App startupScreen={startupScreen} />) });
  return daemon;
}
const windowText = () => document.body.textContent ?? "";
const railButtons = () => [...document.querySelectorAll<HTMLElement>("[data-region=rail] button")].map((one) => one.getAttribute("aria-label"));

it.each(OFFLINE_STATES)("shows what a daemon %s says in the window, with the rail holding only the Remote devices button and no page", async (_, state, message) => {
  mountOffline(state);
  expect(windowText()).toContain(message);
  expect(railButtons()).toEqual(["Remote devices"]);
  expect(page()).toBeNull();
  expect(document.querySelector("[data-region=topbar]")).not.toBeNull();
});

it.each(OFFLINE_STATES)("shows the Remote devices page with its sidebar when asked for while a daemon is %s", async (_, state, message) => {
  saved = [KEYED, SECOND];
  mountOffline(state);
  await showPage();
  expect(page()).not.toBeNull();
  expect(heading()).toBe("Keyed");
  expect(sidebarRows().map((one) => one.textContent)).toEqual(["Keyed", "Second box"]);
  expect(windowText()).not.toContain(message);
  expect(isCurrent(remoteButton())).toBe(true);
});

it("asks for the page at the shell's request, and for a hand-over, though the daemon is not there", async () => {
  mountOffline({ connectionState: "closed" });
  saved = [KEYED, SECOND];
  await devicesChanged();
  await act(async () => askForPage!());
  expect(page()).not.toBeNull();
  await act(async () => handOver!([{ id: SECOND.id, outcome: { outcome: "no_daemon" } }]));
  expect(modal()!.textContent).toContain("No daemon is running on");
});

it("goes back from the page to what the window says of the daemon", async () => {
  mountOffline({ connectionState: "closed" });
  await showPage();
  expect(page()).not.toBeNull();
  await pressBack();
  expect(page()).toBeNull();
  expect(windowText()).toContain("The daemon is not answering.");
});

it("connects to a device from the page whatever the daemon's state", async () => {
  mountOffline({ connectionState: "incompatible" });
  await showPage();
  await pressElement([...page()!.querySelectorAll<HTMLButtonElement>("button")].find((one) => one.getAttribute("aria-label")?.replace(/[\u2068\u2069]/g, "") === "Connect to Keyed"));
  expect(connect).toHaveBeenCalledTimes(1);
});

it("keeps what a connection asked of the owner when the daemon's state turns to incompatible under it", async () => {
  const daemon = mountOffline({ connectionState: "closed" });
  connect.mockResolvedValueOnce({ outcome: "secret_needed", kind: "passphrase" });
  await showPage();
  await pressElement([...page()!.querySelectorAll<HTMLButtonElement>("button")].find((one) => one.getAttribute("aria-label")?.replace(/[\u2068\u2069]/g, "") === "Connect to Keyed"));
  const dialog = modal();
  expect(dialog!.textContent).toContain("Enter key passphrase");
  await act(async () => daemon.store.setState({ connectionState: "incompatible" }));
  expect(modal()).toBe(dialog);
  expect(page()).not.toBeNull();
});

it("shows the window's daemon alone, with no rail, where there is no Remote devices page", async () => {
  mountOffline({ connectionState: "incompatible" }, { platform: { kind: "browser", host: BROWSER_HOST } });
  expect(windowText()).toContain("The app and its daemon are different versions");
  expect(document.querySelector("[data-region=rail]")).toBeNull();
});

it("shows the screen it is given for a daemon that failed to start, as the other states, and the page when asked for", async () => {
  mountOffline({}, { startupScreen: <p>The daemon did not start.</p> });
  expect(windowText()).toContain("The daemon did not start.");
  expect(railButtons()).toEqual(["Remote devices"]);
  expect(page()).toBeNull();
  await showPage();
  expect(page()).not.toBeNull();
  expect(windowText()).not.toContain("The daemon did not start.");
});

it("goes back from the page to the screen of a daemon that failed to start", async () => {
  mountOffline({}, { startupScreen: <p>The daemon did not start.</p> });
  await showPage();
  await pressBack();
  expect(page()).toBeNull();
  expect(windowText()).toContain("The daemon did not start.");
});

it("shows no page while the daemon is first connecting, and the session UI, not the page, once it is there", async () => {
  const daemon = mountOffline({ connectionState: "connecting" });
  expect(page()).toBeNull();
  expect(heading()).toBeUndefined();
  await act(async () => daemon.store.setState(snapshotState({ consoles: [main], projects: [site], sessions: [fix], connectionState: "open" }) as Partial<State>));
  expect(page()).toBeNull();
  expect(railConsole(main.name)).toBeDefined();
  expect(railButtons()).toContain("Settings");
});

const mainCount = () => document.querySelectorAll("main").length;
const ONLINE = () => snapshotState({ consoles: [main, second], projects: [site], sessions: [fix], connectionState: "open" }) as Partial<State>;

it("keeps every project's persisted browser state when the daemon turns unusable after a snapshot", async () => {
  onTestFinished(() => forgetStoredBrowsersOtherThan(new Set()));
  setProjectBrowserMode(site.id, "git");
  const kept = () => Object.keys(JSON.parse(localStorage.getItem(PREFERENCE_KEYS.projectBrowsers) ?? "{}"));
  expect(kept()).toEqual([site.id]);
  const daemon = await mountDesktop();
  expect(kept()).toEqual([site.id]);
  await act(async () => daemon.store.setState({ connectionState: "incompatible" }));
  expect(kept()).toEqual([site.id]);
  await act(async () => daemon.store.setState({ connectionState: "keyRefused" }));
  expect(kept()).toEqual([site.id]);
});

it("still forgets the browser state of a project that is gone while the daemon is usable", async () => {
  onTestFinished(() => forgetStoredBrowsersOtherThan(new Set()));
  setProjectBrowserMode("p-gone", "git");
  await mountDesktop();
  expect(Object.keys(JSON.parse(localStorage.getItem(PREFERENCE_KEYS.projectBrowsers) ?? "{}"))).not.toContain("p-gone");
});

it("leaves a page that was opened while the daemon was unusable once it can be used, though there is no console or session to tell it by", async () => {
  const daemon = mountOffline({ connectionState: "connecting" });
  await showPage();
  expect(page()).not.toBeNull();
  await act(async () => daemon.store.setState(snapshotState({ connectionState: "open" }) as Partial<State>));
  expect(page()).toBeNull();
  expect(welcome()).not.toBeNull();
});

it.each(["incompatible", "keyRefused"] as const)("keeps a page that was opened while the daemon was usable when it turns %s under it, with the form as it was typed", async (state) => {
  const daemon = await mountDesktop();
  await showPage();
  await setValue(field("User"), "ops");
  await act(async () => daemon.store.setState({ connectionState: state }));
  expect(page()).not.toBeNull();
  expect(heading()).toBe("Keyed");
  expect(field("User").value).toBe("ops");
  expect(isCurrent(remoteButton())).toBe(true);
});

it("makes the status screen of an unusable daemon the window's welcome stop of F6, which reaches its controls", async () => {
  mountOffline({}, { startupScreen: <button type="button">Report the failure</button> });
  expect(document.querySelector("[data-region=welcome]")!.textContent).toContain("Report the failure");
  act(() => remoteButton()!.focus());
  expect(regionOfFocus()).toBe("rail");
  await pressF6();
  expect(regionOfFocus()).toBe("welcome");
  expect(document.activeElement!.textContent).toBe("Report the failure");
});

it("reaches the controls of the status of a daemon of another protocol generation by F6", async () => {
  mountOffline({ connectionState: "incompatible" });
  act(() => remoteButton()!.focus());
  await pressF6();
  expect(regionOfFocus()).toBe("welcome");
});

it.each([
  ["connecting", "connecting"],
  ["not answering", "closed"],
] as const)("holds exactly one main landmark while a daemon is %s, the page's included once shown", async (_, connectionState) => {
  mountOffline({ connectionState });
  expect(mainCount()).toBe(1);
  await showPage();
  expect(mainCount()).toBe(1);
});

it("keeps the focus in the first field of a new device's form added from the narrow window's drawer", async () => {
  await mountDesktop(390);
  await openDrawer();
  await pressElement(remoteButton());
  await pressElement(addButton());
  expect(heading()).toBe("Add remote daemon");
  // The drawer has closed, and the form's opening field, which the drawer kept its autofocus from, has the focus.
  expect(topBarButton("Show remote devices")).not.toBeNull();
  expect(document.activeElement).toBe(field("Connection details"));
});

it("makes the status a place Back returns to, and has no place before it", async () => {
  mountOffline({ connectionState: "closed" });
  expect((topBarButton("Back") as HTMLButtonElement).disabled).toBe(true);
  await showPage();
  expect((topBarButton("Back") as HTMLButtonElement).disabled).toBe(false);
  await pressBack();
  expect(windowText()).toContain("The daemon is not answering.");
  expect((topBarButton("Back") as HTMLButtonElement).disabled).toBe(true);
  await pressForward();
  expect(page()).not.toBeNull();
});

it("replaces the status with where the window is once the daemon can be used, rather than leaving it a step Back goes to", async () => {
  const daemon = mountOffline({ connectionState: "connecting" });
  await act(async () => daemon.store.setState(ONLINE()));
  expect((topBarButton("Back") as HTMLButtonElement).disabled).toBe(true);
  await pressElement(row(fix.title));
  expect((topBarButton("Back") as HTMLButtonElement).disabled).toBe(false);
  await pressBack();
  expect(windowText()).not.toContain("Connecting to the daemon");
  expect(railConsole(main.name)).toBeDefined();
  expect((topBarButton("Back") as HTMLButtonElement).disabled).toBe(true);
});

it("passes over the status, and the page opened over it, when moving Back after the daemon can be used", async () => {
  const daemon = mountOffline({ connectionState: "connecting" });
  await showPage();
  await act(async () => daemon.store.setState(ONLINE()));
  expect(page()).toBeNull();
  await pressElement(row(fix.title));
  expect((topBarButton("Back") as HTMLButtonElement).disabled).toBe(false);
  await pressBack();
  expect(page()).toBeNull();
  expect(windowText()).not.toContain("Connecting to the daemon");
});

it("keeps the devices sidebar mounted, its rows and the focus on one, when the daemon turns unusable under the page", async () => {
  saved = [KEYED, SECOND];
  const daemon = await mountDesktop();
  await showPage();
  const rowBefore = deviceRow("Second box")!;
  const sidebarBefore = document.querySelector("[data-region=sidebar]");
  act(() => rowBefore.focus());
  await act(async () => daemon.store.setState({ connectionState: "incompatible" }));
  expect(page()).not.toBeNull();
  expect(document.querySelector("[data-region=sidebar]")).toBe(sidebarBefore);
  expect(deviceRow("Second box")).toBe(rowBefore);
  expect(document.activeElement).toBe(rowBefore);
});

it("hands focus to the rail's Remote devices button when Back leaves the page for the status of an unusable daemon with focus in the sidebar", async () => {
  saved = [KEYED, SECOND];
  mountOffline({ connectionState: "closed" });
  await showPage();
  act(() => deviceRow("Keyed")!.focus());
  expect(regionOfFocus()).toBe("sidebar");
  await pressBack();
  expect(page()).toBeNull();
  expect(document.querySelector("[data-region=sidebar]")).toBeNull();
  expect(document.activeElement).toBe(remoteButton());
});

it("keeps a page opened while the daemon was usable, with its form, when the daemon comes back after an outage", async () => {
  const daemon = await mountDesktop();
  await showPage();
  await setValue(field("User"), "ops");
  await act(async () => daemon.store.setState({ connectionState: "incompatible" }));
  expect(page()).not.toBeNull();
  await act(async () => daemon.store.setState(ONLINE()));
  expect(page()).not.toBeNull();
  expect(heading()).toBe("Keyed");
  expect(field("User").value).toBe("ops");
  expect(isCurrent(remoteButton())).toBe(true);
});

it("still leaves a page opened while the daemon was unusable once it can be used, after an outage and a page kept through another", async () => {
  const daemon = mountOffline({ connectionState: "closed" });
  await showPage();
  await act(async () => daemon.store.setState(ONLINE()));
  expect(page()).toBeNull();
});
