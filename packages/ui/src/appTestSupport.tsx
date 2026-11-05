// Shared by the App tests that mount the whole window over a store of fixtures: the window's width, the daemon
// stand-in, the mount, and finding and pressing what the window shows.
import type React from "react";
import { act } from "react";
import { expect, onTestFinished, vi } from "vitest";

import { App } from "./App";
import { mountForTooltip } from "./components/hint/tooltipTestSupport";
import { consolePreference } from "./store";
import { isolate } from "./i18n/isolate";
import { defaultStack } from "./panels/stackState";
import { usePanelStacks } from "./panels/usePanelStacks";
import type { PlatformAdapter } from "./platform";
import { PlatformProvider } from "./platform/react";
import type { Event, ClientRequestBody } from "./protocol";
import { applyDaemonEvent, DaemonHostAnnouncement, DaemonProvider, type Daemon, type State } from "./store";
import { ThemeProvider } from "./preferences/theme";
import { testDaemon } from "./test/testDaemon";
import { matchesWidthQuery } from "./test/viewport";
import { partSelector } from "./primitives";
import { BROWSER_HOST } from "./test/hosts";

const CSS_VARS = { "--rail-width": "48px", "--breakpoint-tablet": "768px", "--breakpoint-phone": "390px" };

/** A window `width` px wide for one test: the media queries answer width queries by it, and the breakpoints the layout
 * reads are set on the document. Motion is reduced, as jsdom has no Web Animations API for the sidebar's view change,
 * and what leaves with an exit (the archive view) leaves at once. */
export function stubWindow(width = 1400): void {
  const previousWidth = Object.getOwnPropertyDescriptor(document.documentElement, "clientWidth");
  Object.defineProperty(document.documentElement, "clientWidth", { configurable: true, value: width });
  window.dispatchEvent(new Event("resize"));
  vi.stubGlobal("matchMedia", (media: string) => {
    const matches = media === "(prefers-reduced-motion: reduce)" || matchesWidthQuery(media, width);
    return { media, matches, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} };
  });
  for (const [key, value] of Object.entries(CSS_VARS)) document.documentElement.style.setProperty(key, value);
  onTestFinished(() => {
    if (previousWidth) Object.defineProperty(document.documentElement, "clientWidth", previousWidth);
    else Reflect.deleteProperty(document.documentElement, "clientWidth");
    vi.unstubAllGlobals();
    for (const key of Object.keys(CSS_VARS)) document.documentElement.style.removeProperty(key);
    try {
      localStorage.clear();
    } catch {
      // No storage: nothing was kept.
    }
  });
}

/** A daemon whose store starts at `state` and whose requests `answer` replies to: an ack by default. */
export function fakeDaemon(state: Partial<State>, answer: (body: ClientRequestBody) => unknown = () => ({ type: "ack", id: null })): Daemon {
  return testDaemon({
    state,
    request: vi.fn(async (body: ClientRequestBody) => answer(body)) as Daemon["request"],
    showNotice: vi.fn(), reconnect: vi.fn(),
  });
}

/** Mounts the whole window over `daemon` until the test ends, inside `wrap` where given (a provider the window needs
 * above it). The panel stacks and the current console it leaves are put back, as they outlive the mount. */
export function mountApp(daemon: Daemon, { platform = { kind: "browser", host: BROWSER_HOST }, wrap = (app) => app }: { platform?: PlatformAdapter; wrap?: (app: React.ReactElement) => React.ReactElement } = {}): HTMLElement {
  let stacks!: ReturnType<typeof usePanelStacks>;
  function StackReset() {
    stacks = usePanelStacks();
    return null;
  }
  onTestFinished(() => act(() => {
    stacks.setStack("project", defaultStack("project"));
    stacks.setStack("report", defaultStack("report"));
    consolePreference.set(undefined);
  }));
  return mountForTooltip(
    <PlatformProvider value={platform}>
      <ThemeProvider><DaemonProvider value={daemon}><DaemonHostAnnouncement /><StackReset />{wrap(<App />)}</DaemonProvider></ThemeProvider>
    </PlatformProvider>,
  );
}

/** Hands `event` to the store as the daemon client would. */
export const deliver = (daemon: Daemon, event: Event) => act(async () => applyDaemonEvent(daemon.store, event));

export const byLabel = (label: string, within: ParentNode = document) => within.querySelector<HTMLElement>(`[aria-label="${label}"]`);
/** The top bar's control labelled `label`. */
export const topBar = (label: string) => byLabel(label, document.querySelector("[data-region=topbar]")!);
/** The sidebar's row of the session titled `title`. */
export const row = (title: string) =>
  Array.from(document.querySelectorAll<HTMLElement>("[data-region=sidebar] [role=button]")).filter((element) => element.textContent?.includes(title)).at(-1);
/** The rail's avatar of the console named `name`, whatever activity its label adds. */
export const railConsole = (name: string) =>
  Array.from(document.querySelectorAll<HTMLElement>("[data-region=rail] button[aria-label]")).find((button) => {
    const label = button.getAttribute("aria-label")!;
    return label === name || label.startsWith(`${isolate(name)},`);
  });
export const regionOfFocus = () => document.activeElement?.closest("[data-region]")?.getAttribute("data-region");

export function pressElement(element: HTMLElement | null | undefined): Promise<void> {
  expect(element).toBeTruthy();
  return act(async () => element!.click());
}

/** Presses the open menu's item `label`, opening its submenu where it has one. */
export async function pressMenuItem(label: string): Promise<void> {
  const item = Array.from(document.querySelectorAll<HTMLElement>("[role=menuitem], [role=menuitemradio]")).find((element) => (element.querySelector(partSelector.itemLabel)?.textContent ?? element.textContent) === label);
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
