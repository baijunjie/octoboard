// @vitest-environment jsdom
// How often `App` and the children it lays out render. `App` is counted through `useProcessPanel`, which
// `usePaneLayout` calls once per `App` render; a hook it shares with its own hooks, `useT` among them, would count
// those hooks' calls too. Each child is counted through a stand-in for its module that calls the real component as a
// function, so every execution of its body counts, whether `App` or its own state brought it about.
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { fakeTimers } from "./test/clock";
import { App } from "./App";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "./components/announce/StatusAnnouncer";
import { byLabel, fakeDaemon, row, stubWindow } from "./appTestSupport";
import { consoleOf, gitStatusOf, markdownContent, pageOf, projectOf, sessionOf, shellTerminalOf, snapshotState } from "./fixtures/builders";
import { isolate } from "./i18n/isolate";
import { PlatformProvider } from "./platform/react";
import type { Event, ClientRequestBody } from "./protocol";
import { applyDaemonEvent, DaemonProvider, type Daemon } from "./store";
import { ThemeProvider } from "./preferences/theme";
import { BROWSER_HOST } from "./test/hosts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
// Each test mounts the whole App, which a machine under load can take longer than the default 5s for.
vi.setConfig({ testTimeout: 20_000 });

const CHILDREN = ["TitleBar", "Rail", "Sidebar", "TerminalPane", "RightPane", "ArchiveLayer", "Toasts", "UpdateNotice", "ConnectionBanner"] as const;
type Counted = "App" | (typeof CHILDREN)[number];
const renders = vi.hoisted(() => new Map<string, number>());
const counting = vi.hoisted(() => (name: string, component: (props: never) => unknown) =>
  function Counted(props: never) {
    // Called as a function, so its hooks are this one's and none of its executions are hidden by a memoized element.
    "use no memo";
    renders.set(name, (renders.get(name) ?? 0) + 1);
    return component(props);
  });
vi.mock("./process/useProcessPanel", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./process/useProcessPanel")>();
  return { ...actual, useProcessPanel: (...args: Parameters<typeof actual.useProcessPanel>) => {
    renders.set("App", (renders.get("App") ?? 0) + 1);
    return actual.useProcessPanel(...args);
  } };
});
vi.mock("./shell/TitleBar", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./shell/TitleBar")>();
  return { ...actual, TitleBar: counting("TitleBar", actual.TitleBar) };
});
vi.mock("./shell/Rail", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./shell/Rail")>();
  return { ...actual, Rail: counting("Rail", actual.Rail) };
});
vi.mock("./sidebar/Sidebar", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./sidebar/Sidebar")>();
  return { ...actual, Sidebar: counting("Sidebar", actual.Sidebar) };
});
vi.mock("./terminal/TerminalPane", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./terminal/TerminalPane")>();
  return { ...actual, TerminalPane: counting("TerminalPane", actual.TerminalPane) };
});
vi.mock("./panels/RightPane", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./panels/RightPane")>();
  return { ...actual, RightPane: counting("RightPane", actual.RightPane) };
});
vi.mock("./archive/ArchiveLayer", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./archive/ArchiveLayer")>();
  return { ...actual, ArchiveLayer: counting("ArchiveLayer", actual.ArchiveLayer) };
});
vi.mock("./shell/Toasts", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./shell/Toasts")>();
  return { ...actual, Toasts: counting("Toasts", actual.Toasts) };
});
vi.mock("./updates/UpdateNotice", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./updates/UpdateNotice")>();
  return { ...actual, UpdateNotice: counting("UpdateNotice", actual.UpdateNotice) };
});
vi.mock("./shell/ConnectionBanner", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./shell/ConnectionBanner")>();
  return { ...actual, ConnectionBanner: counting("ConnectionBanner", actual.ConnectionBanner) };
});
vi.mock("./project-browser/LazyProjectBrowser", () => ({ LazyProjectBrowser: () => null }));
vi.mock("./report/ReportPanel", () => ({ ReportPanel: () => null }));
vi.mock("./terminal/TerminalController", () => import("./terminalControllerTestSupport"));

const main = consoleOf("c-main", "Main");
const second = consoleOf("c-second", "Second");
const site = projectOf("p-site", main.id, "Website");
const api = projectOf("p-api", main.id, "API");
const fix = sessionOf("s-fix", main.id, site.id, "Fix header", "idle");
const plan = sessionOf("s-plan", main.id, undefined, "Plan release", "idle");
const old = sessionOf("s-old", main.id, site.id, "Old work", "archived");
const elsewhere = sessionOf("s-elsewhere", second.id, undefined, "Triage", "idle");

/** The window at the docked width over the fixtures, on the fake clock from before it mounts, so that every timer
 * the window arms runs where the test settles it. Motion is reduced: jsdom has no Web Animations API for the
 * sidebar's view change. */
function mountApp() {
  renders.clear();
  fakeTimers({ now: Date.UTC(2026, 9, 28, 12) });
  stubWindow();
  const daemon: Daemon = fakeDaemon(snapshotState({ consoles: [main, second], projects: [site, api], sessions: [fix, plan, old, elsewhere] }), (body: ClientRequestBody) =>
    body.type === "list_processes" ? { type: "processes_listed", id: null, trees: [] } : { type: "ack", id: null });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<PlatformProvider value={{ kind: "browser", host: BROWSER_HOST }}><ThemeProvider><DaemonProvider value={daemon}><App /></DaemonProvider></ThemeProvider></PlatformProvider>));
  onTestFinished(async () => {
    act(() => { root.unmount(); });
    container.remove();
  });
  return daemon;
}

/** Runs `step` and everything it sets going, and returns how often each counted component rendered meanwhile. */
async function rendersOf(step: () => unknown): Promise<Record<Counted, number>> {
  const before = new Map(renders);
  await act(async () => { await step(); });
  await act(async () => { await vi.advanceTimersByTimeAsync(AFTER_FOCUS_MOVE_DELAY_MS); });
  return Object.fromEntries((["App", ...CHILDREN] as const).map((name) => [name, (renders.get(name) ?? 0) - (before.get(name) ?? 0)])) as Record<Counted, number>;
}

const found = (element: HTMLElement | null | undefined) => {
  expect(element).toBeTruthy();
  return element!;
};
const menuItem = (label: string) => found(Array.from(document.querySelectorAll<HTMLElement>("[role=menuitem]")).find((item) => (item.querySelector('[data-slot="label"]')?.textContent ?? item.textContent) === label));
const openSubmenu = (item: HTMLElement) => {
  item.focus();
  item.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
};
const hover = (element: HTMLElement, type: "pointerover" | "pointerout") => {
  const event = new MouseEvent(type, { bubbles: true });
  Object.defineProperty(event, "pointerType", { value: "mouse" });
  element.dispatchEvent(event);
};

// App reads the one session, count or derived value it needs rather than a whole map, since every event of a kind
// replaces its map: an event about something it does not read renders nothing of it. Each event here changes the
// store, so a selector over its map would see it.
it.each<[string, Event]>([
  ["a status change of a session that is not selected", { type: "session_upserted", session: { ...plan, status: "working" } }],
  ["a project's git status", { type: "project_git_status", status: gitStatusOf(site.id, { ahead: 2 }) }],
  ["a page of a session that is not selected", { type: "page_created", page: pageOf("pg-1", plan.id, markdownContent("Done"), 1) }],
  ["a shell terminal of a project that does not own the aside", { type: "shell_terminal_upserted", shell_terminal: shellTerminalOf("t-api", api, "zsh", 1) }],
])("does not render App for %s", async (_, event) => {
  const daemon = mountApp();
  const selected = await rendersOf(() => found(row(fix.title)).click());
  expect(selected.App).toBeGreaterThan(0);
  const state = daemon.store.getState();
  const counts = await rendersOf(() => applyDaemonEvent(daemon.store, event));
  expect(daemon.store.getState()).not.toBe(state);
  expect(counts.App).toBe(0);
});

/** The bound of one step: `app` for App, `children` for every child but those `others` names. */
const bound = (app: number, children: number, others: Partial<Record<Counted, number>> = {}): Record<Counted, number> => ({
  App: app, ...Object.fromEntries(CHILDREN.map((name) => [name, children])), ...others,
}) as Record<Counted, number>;

// The toasts, the update notice and the connection banner take nothing from App that changes in this sequence, so
// none of its renders reach them.
const UNREACHED = { Toasts: 0, UpdateNotice: 0, ConnectionBanner: 0 } as const;

// The renders of each step as the code stood when this was written: a bound no change may exceed. A change that
// lowers a count lowers its bound with it.
it("renders App and its children no more often than the baseline over a scripted sequence", async () => {
  const daemon = mountApp();
  const atMount = new Map(renders);
  // The toasts, the update notice and the connection banner are not reached by any step, so they must have rendered
  // here for their bound of 0 to mean anything.
  for (const name of Object.keys(UNREACHED)) expect(atMount.get(name) ?? 0, `${name} at mount`).toBeGreaterThan(0);
  const rail = (name: string) => Array.from(document.querySelectorAll<HTMLElement>("[data-region=rail] button[aria-label]"))
    .find((button) => button.getAttribute("aria-label")!.includes(name) && !button.getAttribute("aria-label")!.startsWith("Actions"));
  const steps: [string, () => unknown, Record<Counted, number>][] = [
    ["a status change of a session that is not selected", () => applyDaemonEvent(daemon.store, { type: "session_upserted", session: { ...plan, status: "working" } }), bound(0, 0)],
    ["a project renamed", () => applyDaemonEvent(daemon.store, { type: "project_upserted", project: { ...api, name: "Backend" } }), bound(1, 1, { ...UNREACHED, TitleBar: 0, RightPane: 0 })],
    ["selecting a session", () => found(row(fix.title)).click(), bound(3, 2, UNREACHED)],
    ["hiding a panel", () => found(byLabel("Hide files panel")).click(), bound(2, 1, UNREACHED)],
    ["hovering a closed docked panel's toggle", () => hover(found(byLabel("Show files panel")), "pointerover"), bound(0, 0)],
    ["leaving a closed docked panel's toggle", () => hover(found(byLabel("Show files panel")), "pointerout"), bound(0, 0)],
    ["opening a dialog", () => found(byLabel("New console")).click(), bound(2, 2, UNREACHED)],
    ["closing the dialog", () => found(Array.from(document.querySelectorAll<HTMLElement>("[role=dialog] button")).find((button) => button.textContent === "Cancel")).click(), bound(2, 2, UNREACHED)],
    ["opening a project's menu", () => found(byLabel(`Actions for project ${isolate(site.name)}`)).click(), bound(1, 1, UNREACHED)],
    ["opening its archive submenu", () => openSubmenu(menuItem("View archive")), bound(0, 0)],
    ["opening the archive", () => menuItem("View all").click(), bound(3, 3, { ...UNREACHED, ArchiveLayer: 5 })],
    ["closing the archive", () => found(document.querySelector<HTMLElement>("[data-region=archive] [aria-label=Close]")).click(), bound(2, 2, UNREACHED)],
    ["switching the console from the rail", () => found(rail(second.name)).click(), bound(3, 2, { ...UNREACHED, TitleBar: 1, RightPane: 0, Sidebar: 1, ArchiveLayer: 1 })],
  ];
  for (const [name, step, limit] of steps) {
    const counts = await rendersOf(step);
    for (const component of ["App", ...CHILDREN] as const) expect(counts[component], `${component} on ${name}`).toBeLessThanOrEqual(limit[component]);
  }
  // Every bound above could have held vacuously for a component that never rendered; none did.
  for (const component of ["App", ...CHILDREN] as const) expect(renders.get(component) ?? 0, `${component} rendered`).toBeGreaterThan(0);
  // The sequence went where it was meant to: the second console's own session is the one now listed.
  expect(row(elsewhere.title)).toBeDefined();
});
