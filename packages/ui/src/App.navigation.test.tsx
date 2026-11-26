// @vitest-environment jsdom
// What the window shows as it moves between consoles, sessions, focus modes and archive views, through the paths
// that cross `App`'s own state: the rail, the new-console dialog, the menu bar icon, Back and Forward, and daemon
// events about what the selection or the aside depends on.
import { defaultDaemonReply } from "./test/testDaemon";
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import { byLabel, deliver, fakeDaemon, mountApp, pressElement, pressMenuItem, railConsole, row, stubWindow, topBar } from "./appTestSupport";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./fixtures/builders";
import { scheduledTaskOf } from "./fixtures/scheduledTaskBuilders";
import { isolate } from "./i18n/isolate";
import type { PlatformAdapter, StatusItemCapability } from "./platform";
import type { Project, ClientRequestBody } from "./protocol";
import { applyDaemonEvent, type Daemon } from "./store";
import { BROWSER_HOST, IS_MAC } from "./test/hosts";
import { inputLabelled, type as typeInto } from "./test/dom";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
// Each test mounts the whole App, which a machine under load can take longer than the default 5s for.
vi.setConfig({ testTimeout: 20_000 });

vi.mock("./report/ReportPanel", () => ({ ReportPanel: () => null }));
// The menu bar icon's menu is drawn with a canvas, which jsdom has none of.
vi.mock("./statusItem/statusItemIcons", () => ({ withIcons: async (draft: object) => ({ ...draft, icons: {} }) }));
// The aside's Files panel says whose project it shows.
vi.mock("./project-browser/LazyProjectBrowser", () => ({ LazyProjectBrowser: ({ project }: { project: Project }) => <div data-browser={project.id} /> }));
// The agent terminal's input stands in for xterm's.
vi.mock("./terminal/TerminalController", () => import("./terminalControllerTestSupport"));

const main = consoleOf("c-main", "Main");
const second = consoleOf("c-second", "Second");
const site = projectOf("p-site", main.id, "Website");
const fix = sessionOf("s-fix", main.id, site.id, "Fix header", "idle");
const triage = sessionOf("s-triage", second.id, undefined, "Triage", "idle");

/** The two consoles, Main current, mounted at the docked width. */
function mountTwoConsoles(answer?: (daemon: () => Daemon, body: ClientRequestBody) => unknown, platform?: PlatformAdapter) {
  stubWindow();
  const daemon: Daemon = fakeDaemon(snapshotState({ consoles: [main, second], projects: [site], sessions: [fix, triage] }), answer && ((body) => answer(() => daemon, body)));
  mountApp(daemon, { platform });
  return daemon;
}

/** What the top bar says, which names the selected session. */
const topBarText = () => document.querySelector("[data-region=topbar]")?.textContent ?? "";
const isCurrent = (name: string) => railConsole(name)?.getAttribute("aria-current") === "true";
const tasksPage = () => document.querySelector("[data-region=tasks]");
const asideBrowser = () => document.querySelector("[data-browser]")?.getAttribute("data-browser");
const agentInput = () => document.querySelector("[data-region=terminal] textarea[aria-label='Agent terminal']");

it("shows a console's last session and its aside again when the rail switches back to it", async () => {
  mountTwoConsoles();
  await pressElement(row(fix.title));
  expect(asideBrowser()).toBe(site.id);

  await pressElement(railConsole(second.name));
  expect(isCurrent(second.name)).toBe(true);
  expect(topBarText()).not.toContain(fix.title);
  expect(asideBrowser()).toBeUndefined();

  await pressElement(railConsole(main.name));
  expect(isCurrent(main.name)).toBe(true);
  expect(topBarText()).toContain(fix.title);
  expect(asideBrowser()).toBe(site.id);
});

/** Types `value` into the open dialog's field labelled `label`. */
function type(label: string, value: string) {
  const input = inputLabelled(label, document.querySelector("[role=dialog]")!);
  expect(input, label).toBeDefined();
  typeInto(input, value);
}

const ops = consoleOf("c-ops", "Ops");

// `create_console` is answered with a bare ack, and its broadcast reaches the window on either side of it.
it.each(["before", "after"] as const)("makes the console this window creates current, its broadcast arriving %s the reply", async (order) => {
  const daemon = mountTwoConsoles((daemon, body) => {
    if (body.type === "create_console" && order === "before") applyDaemonEvent(daemon().store, { type: "console_upserted", console: ops });
    return defaultDaemonReply(body);
  });
  await pressElement(byLabel("New console"));
  type("Name", ops.name);
  await pressElement(Array.from(document.querySelectorAll<HTMLElement>("[role=dialog] button")).find((button) => button.textContent === "Create"));
  expect(document.querySelector("[role=dialog]")).toBeNull();
  if (order === "after") await deliver(daemon, { type: "console_upserted", console: ops });
  expect(isCurrent(ops.name)).toBe(true);
});

it("leaves the current console as it is when another window creates one", async () => {
  const daemon = mountTwoConsoles();
  await deliver(daemon, { type: "console_upserted", console: ops });
  expect(railConsole(ops.name)).toBeDefined();
  expect(isCurrent(main.name)).toBe(true);
});

const pressFocusKey = (code: "ArrowRight" | "ArrowLeft") => act(async () => void window.dispatchEvent(new KeyboardEvent("keydown", { code, key: code, altKey: true, metaKey: IS_MAC, ctrlKey: !IS_MAC, bubbles: true, cancelable: true })));
const pressEnterFocusShortcut = () => pressFocusKey("ArrowRight");
const pressExitFocusShortcut = () => pressFocusKey("ArrowLeft");
const inFocusMode = () => document.querySelector("[data-focus-exit]") !== null;

// A console session, with a coordinator bound to it and a worker bound to the coordinator; and a session bound to
// neither.
const captain = sessionOf("s-captain", main.id, undefined, "Release captain", "idle");
const coordinator = sessionOf("s-coordinator", main.id, site.id, "Coordinator", "idle", { bound_to: captain.id });
const worker = sessionOf("s-worker", main.id, site.id, "Worker", "idle", { bound_to: coordinator.id });
const unrelated = sessionOf("s-unrelated", main.id, site.id, "Unrelated", "idle");
const api = projectOf("p-api", main.id, "API");

function mountTeam() {
  stubWindow();
  const daemon = fakeDaemon(snapshotState({ consoles: [main], projects: [site, api], sessions: [captain, coordinator, worker, unrelated] }));
  mountApp(daemon);
  return daemon;
}

it.each([
  ["its own binding", { ...worker, bound_to: null }],
  ["its owner's binding", { ...coordinator, bound_to: null }],
])("leaves a console session's focus mode when %s takes the selection out of it", async (_, change) => {
  const daemon = mountTeam();
  await pressElement(row(captain.title));
  await pressEnterFocusShortcut();
  expect(inFocusMode()).toBe(true);
  await pressElement(row(worker.title));
  expect(topBarText()).toContain(worker.title);
  expect(inFocusMode()).toBe(true);

  await deliver(daemon, { type: "session_upserted", session: change });
  expect(inFocusMode()).toBe(false);
});

it("keeps a focus mode entered beside the selection through another session's status change, until the shortcut leaves it", async () => {
  const daemon = mountTeam();
  await pressElement(row(unrelated.title));
  await pressElement(byLabel(`Actions for project ${isolate(api.name)}`));
  await pressMenuItem("Focus mode");
  expect(inFocusMode()).toBe(true);
  expect(topBarText()).toContain(unrelated.title);

  await deliver(daemon, { type: "session_upserted", session: { ...captain, status: "working" } });
  expect(inFocusMode()).toBe(true);
  await pressEnterFocusShortcut();
  expect(inFocusMode()).toBe(true);
  await pressExitFocusShortcut();
  expect(inFocusMode()).toBe(false);
});

it("selects the session chosen in the menu bar icon's menu, in its own console, and focuses its terminal", async () => {
  let choose!: (id: string) => void;
  const statusItem: StatusItemCapability = {
    setMenu: async () => {},
    setVisible: async () => {},
    onSessionChosen: (handler) => {
      choose = handler;
      return () => {};
    },
    onOpened: () => () => {},
  };
  mountTwoConsoles(undefined, { kind: "browser", host: BROWSER_HOST, statusItem });
  // Wherever focus was when the window came to the front; not lost, so nothing else moves it.
  act(() => topBar("Hide sessions")!.focus());
  await act(async () => choose(triage.id));
  expect(isCurrent(second.name)).toBe(true);
  expect(topBarText()).toContain(triage.title);
  expect(document.activeElement).toBe(agentInput());
});

/** A platform whose notifications can be chosen: `choose` hands the target to every listener, as the shell does for each
 * hook that raises notifications. */
function platformWithChosenNotifications(): { platform: PlatformAdapter; choose: (target: string) => void } {
  const listeners = new Set<(target: string) => void>();
  const platform: PlatformAdapter = {
    kind: "browser",
    host: BROWSER_HOST,
    notifications: {
      ensurePermission: async () => true,
      notify: async () => {},
      onChosen: (listener) => {
        listeners.add(listener);
        return () => listeners.delete(listener);
      },
    },
  };
  return { platform, choose: (target) => listeners.forEach((listener) => listener(target)) };
}

it("selects the session whose waiting notification is chosen, in its own console, and focuses its terminal", async () => {
  const { platform, choose } = platformWithChosenNotifications();
  const daemon = mountTwoConsoles(undefined, platform);
  await deliver(daemon, { type: "session_upserted", session: { ...triage, status: "waiting_user" } });
  expect(isCurrent(main.name)).toBe(true);
  act(() => topBar("Hide sessions")!.focus());

  // A target that is none of a session's opens nothing.
  await act(async () => choose("something-else"));
  expect(isCurrent(main.name)).toBe(true);
  await act(async () => choose(`session:${triage.id}`));
  expect(isCurrent(second.name)).toBe(true);
  expect(topBarText()).toContain(triage.title);
  expect(document.activeElement).toBe(agentInput());
});

it.each([
  ["in the console shown", fix],
  ["in another console", triage],
])("leaves the Scheduled tasks page for the session, %s, whose waiting notification is chosen, and focuses its terminal", async (_, chosen) => {
  const { platform, choose } = platformWithChosenNotifications();
  const report = scheduledTaskOf("t-report", main.id, { launch: { title: "Morning report", compatibility: {} } });
  stubWindow();
  mountApp(
    fakeDaemon({
      ...snapshotState({ consoles: [main, second], projects: [site], sessions: [{ ...fix, status: "idle" }, { ...triage, status: "idle" }].map((session) => (session.id === chosen.id ? { ...session, status: "waiting_user" as const } : session)) }),
      scheduledTasks: new Map([[report.id, report]]),
      scheduledTaskRuns: new Map([[report.id, []]]),
    }),
    { platform },
  );
  await pressElement(document.querySelector<HTMLElement>("[data-region=rail] button[aria-label^='Scheduled tasks']"));
  expect(tasksPage()?.querySelector("h1")?.textContent).toBe("Morning report");
  // Opened from the rail, focus is in the page.
  expect(tasksPage()!.contains(document.activeElement)).toBe(true);

  // Focus handed to the page on its way out is lost when it unmounts, which a window that falls back on its own cannot
  // tell from the terminal taking it, so what focus is given to is recorded.
  const focus = vi.spyOn(HTMLElement.prototype, "focus");
  onTestFinished(() => focus.mockRestore());
  await act(async () => choose(`session:${chosen.id}`));
  expect((focus.mock.contexts as HTMLElement[]).filter((element) => element.closest("[data-region=tasks]"))).toEqual([]);
  expect(tasksPage()).toBeNull();
  expect(isCurrent(chosen.console_id === second.id ? second.name : main.name)).toBe(true);
  expect(topBarText()).toContain(chosen.title);
  // The page's own landing is gone with it, so focus is on the terminal rather than dropped to the body.
  expect(document.activeElement).toBe(agentInput());
});

it("clears the aside whose owner goes, and keeps it cleared when the owner comes back", async () => {
  const daemon = mountTwoConsoles();
  await pressElement(row(fix.title));
  expect(asideBrowser()).toBe(site.id);
  await deliver(daemon, { type: "project_deleted", project: site.id });
  expect(asideBrowser()).toBeUndefined();
  await deliver(daemon, { type: "project_upserted", project: site });
  expect(row(site.name)).toBeDefined();
  expect(asideBrowser()).toBeUndefined();
});

it("puts an archive view's scroll back when Forward returns to it, and opens it at the top when it is asked for afresh", async () => {
  stubWindow();
  const archived = Array.from({ length: 3 }, (_, index) => sessionOf(`s-old-${index}`, main.id, site.id, `Old work ${index}`, "archived"));
  mountApp(fakeDaemon(snapshotState({ consoles: [main], projects: [site], sessions: [fix, ...archived] })));
  const scroller = () => document.querySelector<HTMLElement>("section[data-region=archive] > div");
  const openArchive = async () => {
    await pressElement(byLabel(`Actions for project ${isolate(site.name)}`));
    await pressMenuItem("View archive");
    await pressMenuItem("View all");
    expect(scroller()).not.toBeNull();
  };
  await openArchive();
  act(() => {
    scroller()!.scrollTop = 40;
    scroller()!.dispatchEvent(new Event("scroll", { bubbles: true }));
  });

  await pressElement(topBar("Back"));
  expect(scroller()).toBeNull();
  await pressElement(topBar("Forward"));
  expect(scroller()?.scrollTop).toBe(40);

  await pressElement(topBar("Back"));
  await openArchive();
  expect(scroller()!.scrollTop).toBe(0);
});
