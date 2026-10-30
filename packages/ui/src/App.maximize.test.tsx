// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { App } from "./App";
import { PanelHeader } from "./components/PanelHeader";
import { mountForTooltip } from "./components/tooltipTestSupport";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./gallery/fixtures/builders";
import { isolate } from "./i18n/isolate";
import { defaultStack } from "./panels/stackState";
import { usePanelStacks } from "./panels/usePanelStacks";
import { PlatformProvider } from "./platform/react";
import type { Project } from "./protocol";
import { createStateStore, DaemonProvider, type Daemon } from "./store";
import { ThemeProvider } from "./theme";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

vi.mock("./browser/LazyProjectBrowser", () => ({
  LazyProjectBrowser: ({ project }: { project: Project }) => (
    <>
      <PanelHeader title={project.name} />
      <button type="button">in files</button>
    </>
  ),
}));
// The shell's input stands in for a terminal view: where F6 lands in the panel.
vi.mock("./terminal/ShellTerminalPanel", () => ({
  ShellTerminalPanel: () => (
    <>
      <PanelHeader title="Terminal" />
      <button type="button" data-region-landing>shell input</button>
    </>
  ),
}));
vi.mock("./terminal/shellTerminals", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./terminal/shellTerminals")>();
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
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: media.startsWith("(min-width:"), addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  document.documentElement.style.setProperty("--rail-width", "48px");
  document.documentElement.style.setProperty("--breakpoint-docked", "1168px");
  document.documentElement.style.setProperty("--breakpoint-phone", "782px");
  onTestFinished(() => {
    act(() => {
      stacks.setStack("project", defaultStack("project"));
      stacks.setStack("report", defaultStack("report"));
    });
    vi.unstubAllGlobals();
    document.documentElement.style.removeProperty("--rail-width");
    document.documentElement.style.removeProperty("--breakpoint-docked");
    document.documentElement.style.removeProperty("--breakpoint-phone");
    try {
      localStorage.clear();
    } catch {
      // No storage: nothing was kept.
    }
  });
  const console_ = consoleOf("c-max", "Main");
  const project = projectOf("p-site", console_.id, "Website");
  const sessions = [sessionOf("s-site", console_.id, project.id, "Fix header", "idle"), sessionOf("s-console", console_.id, undefined, "Plan release", "idle"), sessionOf("s-old", console_.id, project.id, "Old work", "archived")];
  const daemon: Daemon = {
    store: createStateStore(snapshotState({ consoles: [console_], projects: [project], sessions })),
    request: vi.fn(async () => ({ type: "ack", id: null })) as Daemon["request"],
    toastError: vi.fn(), toastNotice: vi.fn(), onToast: () => () => {},
    dismissTrustPrompt: () => {}, reconnect: () => {}, terminalUrl: () => "",
  };
  return mountForTooltip(
    <PlatformProvider value={{ kind: "browser" }}>
      <ThemeProvider><DaemonProvider value={daemon}><StackReset /><App /></DaemonProvider></ThemeProvider>
    </PlatformProvider>,
  );
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

it("orders the top bar's panel toggles as the panels stack, Files above Terminal", async () => {
  mountApp();
  await pressElement(row("Fix header"));
  await pressElement(toggle("Show terminal panel"));
  const labels = Array.from(document.querySelectorAll("[data-region=topbar] [aria-label$='panel']"), (element) => element.getAttribute("aria-label"));
  expect(labels).toEqual(["Hide files panel", "Show processes panel", "Hide terminal panel"]);
  const cards = Array.from(document.querySelectorAll("[data-panel-card]"), (card) => card.id);
  expect(cards).toEqual(["panel-files", "panel-terminal"]);
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
