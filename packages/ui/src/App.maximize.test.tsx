// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { App } from "./App";
import { stubWindow } from "./appTestSupport";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "./components/announce/StatusAnnouncer";
import { windowAttention } from "./components/announce/windowTestSupport";
import { PanelHeader } from "./components/panel/PanelHeader";
import { mountForTooltip } from "./components/hint/tooltipTestSupport";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./fixtures/builders";
import { isolate } from "./i18n/isolate";
import { defaultStack } from "./panels/stackState";
import { usePanelStacks } from "./panels/usePanelStacks";
import { PlatformProvider } from "./platform/react";
import type { Project } from "./protocol";
import { DaemonProvider, type Daemon } from "./store";
import { testDaemon } from "./test/testDaemon";
import { ThemeProvider } from "./preferences/theme";
import { BROWSER_HOST } from "./test/hosts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

vi.mock("./project-browser/LazyProjectBrowser", () => ({
  LazyProjectBrowser: ({ project }: { project: Project }) => (
    <>
      <PanelHeader title={project.name} />
      <button type="button">in files</button>
    </>
  ),
}));
// The shell's input stands in for a terminal view: where F6 lands in the panel.
vi.mock("./terminal/shell/ShellTerminalPanel", () => ({
  ShellTerminalPanel: () => (
    <>
      <PanelHeader title="Terminal" />
      <button type="button" data-region-landing>shell input</button>
    </>
  ),
}));
vi.mock("./terminal/shell/shellTerminals", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./terminal/shell/shellTerminals")>();
  return { ...actual, startShellTerminal: vi.fn(async () => undefined) };
});
vi.mock("./report/ReportPanel", () => ({ ReportPanel: () => <PanelHeader title="Report" /> }));
// Every method of the agent terminal's controller does nothing: the selected session attaches one.
vi.mock("./terminal/TerminalController", () => ({
  TerminalController: class {
    constructor() {
      return new Proxy(this, { get: (target, key) => (key === "then" ? undefined : key in target ? Reflect.get(target, key) : () => {}) });
    }
  },
}));

function mountApp() {
  // The panel stacks outlive a test's window: they go back as a fresh client has them.
  let stacks!: ReturnType<typeof usePanelStacks>;
  function StackReset() {
    stacks = usePanelStacks();
    return null;
  }
  stubWindow();
  onTestFinished(() => {
    act(() => {
      stacks.setStack("project", defaultStack("project"));
      stacks.setStack("report", defaultStack("report"));
    });
    try {
      localStorage.clear();
    } catch {
      // No storage: nothing was kept.
    }
  });
  const console_ = consoleOf("c-max", "Main");
  const project = projectOf("p-site", console_.id, "Website");
  const sessions = [sessionOf("s-site", console_.id, project.id, "Fix header", "idle"), sessionOf("s-console", console_.id, undefined, "Plan release", "idle"), sessionOf("s-old", console_.id, project.id, "Old work", "archived")];
  const daemon = testDaemon({
    state: snapshotState({ consoles: [console_], projects: [project], sessions }),
    request: vi.fn(async () => ({ type: "ack", id: null })) as Daemon["request"],
  });
  mountForTooltip(
    <PlatformProvider value={{ kind: "browser", host: BROWSER_HOST }}>
      <ThemeProvider><DaemonProvider value={daemon}><StackReset /><App /></DaemonProvider></ThemeProvider>
    </PlatformProvider>,
  );
  return daemon;
}

const pressElement = (element: HTMLElement | null | undefined) => {
  expect(element).toBeTruthy();
  return act(async () => element!.click());
};
const byLabel = (label: string, within: ParentNode = document) => within.querySelector<HTMLElement>(`[aria-label="${label}"]`);
const toggle = (label: string) => byLabel(label, document.querySelector("[data-region=topbar]")!);
/** The sidebar's row of the session titled `title`. */
const row = (title: string) =>
  Array.from(document.querySelectorAll<HTMLElement>("[data-region=sidebar] [role=button]")).filter((element) => element.textContent?.includes(title)).at(-1);
const terminalCard = () => document.getElementById("panel-terminal");
const filesCard = () => document.getElementById("panel-files");
const isCovered = (selector: string) => document.querySelector(selector)?.closest("[inert]") != null;
const f6 = (shift = false) => act(async () => void window.dispatchEvent(new KeyboardEvent("keydown", { key: "F6", shiftKey: shift, bubbles: true, cancelable: true })));
const regionOfFocus = () => document.activeElement?.closest("[data-region]")?.getAttribute("data-region");

/** The project's session selected, with Files and Terminal open in its pane, and the Terminal
 * panel maximized. */
async function maximizeTerminal() {
  await pressElement(row("Fix header"));
  await pressElement(toggle("Show terminal panel"));
  expect(filesCard()).not.toBeNull();
  await pressElement(byLabel("Maximize panel", terminalCard()!));
}

it("offers maximize on the Terminal panel only, and maximized it covers the agent terminal alone", async () => {
  mountApp();
  await pressElement(row("Fix header"));
  await pressElement(toggle("Show terminal panel"));
  expect(byLabel("Maximize panel", filesCard()!)).toBeNull();
  await pressElement(byLabel("Maximize panel", terminalCard()!));

  expect(terminalCard()!.hasAttribute("data-panel-maximized")).toBe(true);
  expect(terminalCard()!.closest("[inert]")).toBeNull();
  expect(isCovered("[data-region=terminal]")).toBe(true);
  for (const region of ["rail", "sidebar", "panel-files", "topbar"]) expect(isCovered(`[data-region=${region}]`), region).toBe(false);
  // Files stays docked in the pane, with its width handle, and both toggles read open.
  expect(document.querySelector("[role=separator][data-pane=aside]")).not.toBeNull();
  expect(toggle("Hide files panel")).not.toBeNull();
  expect(toggle("Hide terminal panel")).not.toBeNull();
  expect(byLabel("Restore panel size", terminalCard()!)).not.toBeNull();

  await pressElement(byLabel("Restore panel size", terminalCard()!));
  expect(terminalCard()!.hasAttribute("data-panel-maximized")).toBe(false);
  expect(isCovered("[data-region=terminal]")).toBe(false);
});

it("skips the covered agent terminal in the F6 cycle, the maximized panel taking its place", async () => {
  mountApp();
  await maximizeTerminal();
  await act(async () => (row("Fix header")!).focus());
  expect(regionOfFocus()).toBe("sidebar");
  await f6();
  expect(regionOfFocus()).toBe("panel-terminal");
  expect(document.activeElement?.textContent).toBe("shell input");
  await f6();
  expect(regionOfFocus()).toBe("panel-files");
  await f6(true);
  await f6(true);
  expect(regionOfFocus()).toBe("sidebar");
});

it("keeps Files and the sidebar as they are while the Terminal panel is maximized", async () => {
  mountApp();
  await maximizeTerminal();
  await pressElement(toggle("Hide files panel"));
  expect(filesCard()).toBeNull();
  expect(terminalCard()!.hasAttribute("data-panel-maximized")).toBe(true);
  // No card is left in the pane's column: it folds away, with its width handle.
  expect(document.querySelector("[role=separator][data-pane=aside]")).toBeNull();
  await pressElement(toggle("Show files panel"));
  expect(filesCard()).not.toBeNull();
  expect(terminalCard()!.hasAttribute("data-panel-maximized")).toBe(true);
  await pressElement(toggle("Hide sessions"));
  expect(terminalCard()!.hasAttribute("data-panel-maximized")).toBe(true);
});

it("restores the stack when the pane passes to a console session, so the project's comes back as it was", async () => {
  mountApp();
  await maximizeTerminal();
  await pressElement(row("Plan release"));
  expect(terminalCard()).toBeNull();
  expect(document.getElementById("panel-report")).not.toBeNull();
  expect(isCovered("[data-region=terminal]")).toBe(false);
  await pressElement(row("Fix header"));
  expect(terminalCard()!.hasAttribute("data-panel-maximized")).toBe(false);
  expect(filesCard()).not.toBeNull();
});

it("restores the layout and uncovers the agent terminal when the maximized panel is closed", async () => {
  mountApp();
  await maximizeTerminal();
  await pressElement(byLabel("Close panel", terminalCard()!));
  expect(terminalCard()).toBeNull();
  expect(isCovered("[data-region=terminal]")).toBe(false);
});

it("floats Files in from its toggle beside the maximized Terminal panel, and docks it on a press", async () => {
  mountApp();
  await maximizeTerminal();
  await pressElement(toggle("Hide files panel"));
  vi.useFakeTimers();
  document.documentElement.style.setProperty("--peek-gap", "8px");
  onTestFinished(() => {
    vi.useRealTimers();
    document.documentElement.style.removeProperty("--peek-gap");
  });
  const button = toggle("Show files panel")!;
  const pointer = (type: string) => new PointerEvent(type, { pointerType: "mouse", bubbles: type !== "pointerenter" });
  await act(async () => {
    button.dispatchEvent(pointer("pointerover"));
    button.dispatchEvent(pointer("pointerenter"));
    await vi.advanceTimersByTimeAsync(300);
  });
  // The card floats in a pane of its own, the maximized panel staying where it is.
  const floating = document.querySelector<HTMLElement>("[data-pane=aside]");
  expect(floating?.classList.contains("fixed")).toBe(true);
  expect(floating?.querySelector("#panel-files")).not.toBeNull();
  expect(terminalCard()!.hasAttribute("data-panel-maximized")).toBe(true);
  expect(floating?.contains(terminalCard())).toBe(false);

  await pressElement(toggle("Show files panel"));
  expect(document.querySelector("[data-pane=aside]")?.classList.contains("fixed")).toBe(false);
  expect(filesCard()?.closest("[data-pane=aside]")).not.toBeNull();
  expect(toggle("Hide files panel")).not.toBeNull();
  expect(terminalCard()!.hasAttribute("data-panel-maximized")).toBe(true);
});

it("orders the top bar's panel toggles as the panels stack, Terminal above Files", async () => {
  mountApp();
  await pressElement(row("Fix header"));
  await pressElement(toggle("Show terminal panel"));
  const labels = Array.from(document.querySelectorAll("[data-region=topbar] [aria-label$='panel']"), (element) => element.getAttribute("aria-label"));
  expect(labels).toEqual(["Hide terminal panel", "Hide files panel", "Show processes panel"]);
  const cards = Array.from(document.querySelectorAll("[data-panel-card]"), (card) => card.id);
  expect(cards).toEqual(["panel-terminal", "panel-files"]);
});

/** Whether the archive view is open, not just still on screen for its exit. */
const archiveShown = () => document.querySelector(".archive-layer[data-state=open]") !== null;

/** Presses a menu item of the open menu by its text; a submenu opens on ArrowRight. */
async function pressMenuItem(label: string) {
  const item = Array.from(document.querySelectorAll<HTMLElement>("[role=menuitem]")).find((element) => element.textContent === label);
  expect(item, label).toBeDefined();
  await act(async () => {
    if (item!.hasAttribute("aria-haspopup")) {
      item!.focus();
      item!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
    } else {
      item!.click();
    }
  });
}

it("restores the maximized Terminal panel when a project's archive view is opened, and keeps it for another session", async () => {
  mountApp();
  await maximizeTerminal();
  await pressElement(row("Fix header"));
  expect(terminalCard()!.hasAttribute("data-panel-maximized")).toBe(true);

  await pressElement(byLabel(`Actions for project ${isolate("Website")}`));
  await pressMenuItem("View archive");
  await pressMenuItem("View all");
  expect(document.querySelector("[data-region=archive]")).not.toBeNull();
  expect(terminalCard()!.hasAttribute("data-panel-maximized")).toBe(false);
  expect(filesCard()).not.toBeNull();
});

it("restores the maximized Terminal panel when Back or Forward arrives at an archive view, and keeps it for any other view", async () => {
  mountApp();
  await maximizeTerminal();
  await pressElement(byLabel(`Actions for project ${isolate("Website")}`));
  await pressMenuItem("View archive");
  await pressMenuItem("View all");
  expect(terminalCard()!.hasAttribute("data-panel-maximized")).toBe(false);

  await pressElement(byLabel("Maximize panel", terminalCard()!));
  await pressElement(byLabel("Back"));
  expect(archiveShown()).toBe(false);
  expect(terminalCard()!.hasAttribute("data-panel-maximized")).toBe(true);

  await pressElement(byLabel("Forward"));
  expect(archiveShown()).toBe(true);
  expect(terminalCard()!.hasAttribute("data-panel-maximized")).toBe(false);
});

const RECOVERED = "Recovered terminal display";
const PENDING_SENTENCE = "Reconnecting to the session. The session process is still running. Input is unavailable until the connection returns. Reconnection is automatic.";
/** The status regions in the window holding exactly `text`. */
const said = (text: string) => Array.from(document.querySelectorAll("[role=status]")).filter((region) => region.textContent === text);

/** The App mounted with its timers faked before anything arms one, the window attended, and the project's session
 * selected with the Terminal panel maximized over the agent area. Returns what changes the selected session. */
async function maximizedWithOpenSession() {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  onTestFinished(() => {
    act(() => void vi.runOnlyPendingTimers());
    vi.useRealTimers();
  });
  windowAttention();
  const daemon = mountApp();
  await maximizeTerminal();
  const update = (change: Partial<ReturnType<typeof sessionOf>>) =>
    act(() => {
      const sessions = new Map(daemon.store.getState().sessions);
      sessions.set("s-site", { ...sessions.get("s-site")!, ...change });
      daemon.store.setState({ sessions });
    });
  const advance = (ms: number) => act(async () => void (await vi.advanceTimersByTimeAsync(ms)));
  // Past the status regions' first write delay, so what comes next is written as it is said.
  await advance(AFTER_FOCUS_MOVE_DELAY_MS);
  return { update, advance };
}

it("holds the recovered display's sentence raised under a maximized panel until the panel is restored, then says it once", async () => {
  const { update, advance } = await maximizedWithOpenSession();
  expect(isCovered("[data-region=terminal]")).toBe(true);
  await update({ screen_inexact: true });
  await advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
  expect(said(RECOVERED)).toEqual([]);

  await pressElement(byLabel("Restore panel size", terminalCard()!));
  expect(isCovered("[data-region=terminal]")).toBe(false);
  await advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  expect(said(RECOVERED)).toEqual([]);
  await advance(1);
  expect(said(RECOVERED)).toHaveLength(1);
});

it("holds the pending sentence of a session that turns pending under a maximized panel until the panel is restored, then says it once", async () => {
  const { update, advance } = await maximizedWithOpenSession();
  await update({ holder_link: "pending" });
  await advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
  expect(said(PENDING_SENTENCE)).toEqual([]);

  await pressElement(byLabel("Restore panel size", terminalCard()!));
  await advance(AFTER_FOCUS_MOVE_DELAY_MS - 1);
  expect(said(PENDING_SENTENCE)).toEqual([]);
  await advance(1);
  expect(said(PENDING_SENTENCE)).toHaveLength(1);
  // Maximized and restored again: said no more.
  await pressElement(byLabel("Maximize panel", terminalCard()!));
  await pressElement(byLabel("Restore panel size", terminalCard()!));
  await advance(AFTER_FOCUS_MOVE_DELAY_MS * 3);
  expect(said(PENDING_SENTENCE)).toHaveLength(1);
});
