// @vitest-environment jsdom
// The Scheduled tasks page, through the paths that cross `App`'s own state: the rail's button, what the page takes the
// place of, the top bar, how it is left, Back and Forward, where focus goes, and its place beside the Remote devices
// page.
import { act } from "react";
import { beforeEach, expect, it, vi } from "vitest";

import { errorAlerts } from "./components/alerts/errorAlerts";
import { framesAsTimers, settleFrames } from "./components/dialog/dialogTestSupport";
import { MODAL_CLOSE_SETTLE_MS } from "./components/focus/modalFocus";
import { fakeDaemon, mountApp, pressElement, pressMenuItem, railConsole, regionOfFocus, row, stubWindow } from "./appTestSupport";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./fixtures/builders";
import { t } from "./i18n/language";
import type { PlatformAdapter, RemoteDaemonEntry } from "./platform";
import type { ScheduledTask, ScheduledTaskRun, Session } from "./protocol";
import { scheduledTaskPreference } from "./scheduledTasks/scheduledTaskPreference";
import { acknowledgeAllAlerts } from "./test/alertQueue";
import { fakeTimersKeepingFrames } from "./test/clock";
import { daemonError } from "./test/daemonError";
import { defaultDaemonReply } from "./test/testDaemon";
import { BROWSER_HOST, MAC_HOST } from "./test/hosts";
import { applyDaemonEvent, type Daemon } from "./store";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
// Each test mounts the whole App, which a machine under load can take longer than the default 5s for.
vi.setConfig({ testTimeout: 20_000 });

vi.mock("./report/ReportPanel", () => ({ ReportPanel: () => null }));
// The menu bar icon's menu is drawn with a canvas, which jsdom has none of.
vi.mock("./statusItem/statusItemIcons", () => ({ withIcons: async (draft: object) => ({ ...draft, icons: {} }) }));
vi.mock("./project-browser/LazyProjectBrowser", () => ({ LazyProjectBrowser: () => null }));
// The agent terminal's input stands in for xterm's.
vi.mock("./terminal/TerminalController", () => import("./terminalControllerTestSupport"));
// The quote target's own hook, with the `uncoverTerminal` the window hands it kept: what a quote from the file viewer runs
// (inside a `flushSync`, with the viewer closing in the same commit) is run here by itself.
const quoting = vi.hoisted(() => ({ uncover: () => {} }));
vi.mock("./app/useTerminalQuoteTarget", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./app/useTerminalQuoteTarget")>();
  return {
    ...actual,
    useTerminalQuoteTarget: (...args: Parameters<typeof actual.useTerminalQuoteTarget>) => {
      quoting.uncover = args[2];
      return actual.useTerminalQuoteTarget(...args);
    },
  };
});

const main = consoleOf("c-main", "Main");
const second = consoleOf("c-second", "Second");
const site = projectOf("p-site", main.id, "Website");
const fix = sessionOf("s-fix", main.id, site.id, "Fix header", "idle");

function taskOf(id: string, title: string): ScheduledTask {
  return {
    id,
    console_id: main.id,
    project_id: null,
    launch: { title, compatibility: {} },
    prompt: "Summarise yesterday's commits",
    schedule: { kind: "daily", time: { hour: 9, minute: 0 } },
    enabled: true,
    created_at: 1,
    updated_at: 1,
    next_run_at: null,
    last_run_at: null,
    run_sessions: [],
  };
}

const REPORT = taskOf("t-report", "Morning report");
const CLEANUP = taskOf("t-cleanup", "Weekly cleanup");

const KEYED: RemoteDaemonEntry = { id: "keyed", name: "Keyed", host: "keyed.example.com", port: 22, user: "deploy", auth: "password", keyPath: null, fingerprint: null, octoboarddPath: null };

/** The desktop application's own window, which has the Remote devices page too. */
const DESKTOP: PlatformAdapter = {
  kind: "tauri",
  host: MAC_HOST,
  remoteDaemons: {
    list: async () => [KEYED],
    status: async () => [],
    onChanged: () => () => {},
    save: async (entry) => ({ ...entry, id: entry.id ?? "new" }),
    remove: async () => {},
    connect: async () => ({ outcome: "opened" }),
    disconnect: async () => {},
    onPageRequested: () => () => {},
    onHandedOver: () => () => {},
    showMainWindow: async () => {},
    showUpdates: async () => {},
  },
};

beforeEach(() => {
  acknowledgeAllAlerts();
});

/** The window over two consoles, Main current with a session selected, holding `tasks` as the daemon listed them, and
 * `runs` as it listed their runs. */
async function mountWindow(tasks: ScheduledTask[] = [REPORT, CLEANUP], platform?: PlatformAdapter, { runs = [], sessions = [] }: { runs?: ScheduledTaskRun[]; sessions?: Session[] } = {}) {
  stubWindow(1400);
  const daemon: Daemon = fakeDaemon({
    ...snapshotState({ consoles: [main, second], projects: [site], sessions: [fix, ...sessions] }),
    scheduledTasks: new Map(tasks.map((task) => [task.id, task])),
    scheduledTaskRuns: new Map(tasks.map((task) => [task.id, runs.filter((run) => run.task_id === task.id)])),
  }, (body) => {
    if (body.type !== "delete_scheduled_task") return defaultDaemonReply(body);
    // As the daemon answers a delete: the broadcast reaches the window as well.
    applyDaemonEvent(daemon.store, { type: "scheduled_task_deleted", task: body.task });
    return { type: "ack", id: null };
  });
  mountApp(daemon, platform ? { platform } : undefined);
  await pressElement(row(fix.title));
  return daemon;
}

const tasksButton = () => document.querySelector<HTMLElement>("[data-region=rail] button[aria-label^='Scheduled tasks']");
const page = () => document.querySelector<HTMLElement>("[data-region=tasks]");
const heading = () => page()?.querySelector("h1")?.textContent;
const topBar = () => document.querySelector<HTMLElement>("[data-region=topbar]")!;
const topBarButton = (label: string) => topBar().querySelector<HTMLElement>(`[aria-label="${label}"]`);
const isCurrent = (element: Element | null | undefined) => element?.getAttribute("aria-current") === "true";
/** Whether `element` is out of the layout, as what the page takes the place of is: kept mounted, `display: none`. */
const hidden = (element: Element | null | undefined) => element?.closest(".hidden") != null;
const sidebarRow = (name: string) => [...document.querySelectorAll<HTMLElement>("[data-region=sidebar] [data-list-motion] [role=button]")].find((one) => one.getAttribute("aria-label") === name);
const showPage = () => pressElement(tasksButton());
const pressF6 = () => act(async () => void document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "F6", bubbles: true, cancelable: true })));

it("shows the page in place of the sessions, the sidebar listing the tasks, the button marked current", async () => {
  await mountWindow();
  expect(page()).toBeNull();
  const terminal = document.querySelector("[data-region=terminal]");
  expect(hidden(terminal)).toBe(false);

  await showPage();
  expect(page()).not.toBeNull();
  expect(heading()).toBe("Morning report");
  expect(document.querySelector("[data-region=sidebar]")!.getAttribute("aria-label")).toBe("Scheduled tasks");
  expect(sidebarRow("Weekly cleanup")).toBeDefined();
  // Hidden, not unmounted.
  expect(hidden(terminal)).toBe(true);
  expect(isCurrent(tasksButton())).toBe(true);
  expect(isCurrent(railConsole(main.name))).toBe(false);
  // Opened from the rail, focus moves into the page.
  expect(page()!.contains(document.activeElement)).toBe(true);
});

it("keeps Back, Forward and the sidebar's toggle in the top bar, and names the page and the task in the breadcrumb", async () => {
  await mountWindow();
  await showPage();
  expect(topBarButton("Back")).not.toBeNull();
  expect(topBarButton("Hide scheduled tasks")).not.toBeNull();
  const trail = topBar().textContent!;
  expect(trail).toContain("Scheduled tasks");
  expect(trail).toContain("Morning report");
  expect(trail).not.toContain(fix.title);
  await pressElement(sidebarRow("Weekly cleanup"));
  expect(topBar().textContent).toContain("Weekly cleanup");
});

it("leaves the page for the console whose avatar is chosen", async () => {
  await mountWindow();
  await showPage();
  await pressElement(railConsole(second.name));
  expect(page()).toBeNull();
  expect(isCurrent(railConsole(second.name))).toBe(true);
  expect(isCurrent(tasksButton())).toBe(false);
});

it("is a step of Back and Forward, and so is choosing another task", async () => {
  await mountWindow();
  await showPage();
  await pressElement(sidebarRow("Weekly cleanup"));
  expect(heading()).toBe("Weekly cleanup");
  await pressElement(topBarButton("Back"));
  expect(heading()).toBe("Morning report");
  await pressElement(topBarButton("Back"));
  expect(page()).toBeNull();
  expect(topBar().textContent).toContain(fix.title);
  await pressElement(topBarButton("Forward"));
  expect(heading()).toBe("Morning report");
  expect(isCurrent(tasksButton())).toBe(true);
});

it("opens on the task last chosen", async () => {
  scheduledTaskPreference.set(CLEANUP.id);
  await mountWindow();
  await showPage();
  expect(heading()).toBe("Weekly cleanup");
});

it("is a stop of F6 after the tasks sidebar, and the terminal is not one while the page shows", async () => {
  await mountWindow();
  await showPage();
  act(() => tasksButton()!.focus());
  expect(regionOfFocus()).toBe("rail");
  await pressF6();
  expect(regionOfFocus()).toBe("sidebar");
  await pressF6();
  expect(regionOfFocus()).toBe("tasks");
  for (let stop = 0; stop < 4; stop++) {
    await pressF6();
    expect(["topbar", "rail", "sidebar", "tasks"]).toContain(regionOfFocus());
  }
});

it("shows one page at a time: opening the Remote devices page leaves this one, and the other way round", async () => {
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  await mountWindow(undefined, DESKTOP);
  const remoteButton = document.querySelector<HTMLElement>("[data-region=rail] button[aria-label^='Remote devices']")!;
  // The button stands directly above Remote devices.
  const railButtons = [...document.querySelectorAll<HTMLElement>("[data-region=rail] button")];
  expect(railButtons.indexOf(remoteButton)).toBe(railButtons.indexOf(tasksButton()!) + 1);
  await showPage();
  await pressElement(remoteButton);
  expect(page()).toBeNull();
  expect(document.querySelector("[data-region=devices]")).not.toBeNull();
  expect(isCurrent(tasksButton())).toBe(false);
  await showPage();
  expect(page()).not.toBeNull();
  expect(document.querySelector("[data-region=devices]")).toBeNull();
});

it.each(["tasks", "devices"] as const)("leaves the %s page where a quote uncovers the terminal, and the terminal takes the focus once the page is gone", async (shown) => {
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  await mountWindow(undefined, DESKTOP);
  if (shown === "tasks") await showPage();
  else await pressElement(document.querySelector<HTMLElement>("[data-region=rail] button[aria-label^='Remote devices']"));
  const pageRegion = () => document.querySelector(`[data-region=${shown}]`);
  expect(pageRegion()).not.toBeNull();
  act(() => quoting.uncover());
  expect(pageRegion()).toBeNull();
  expect(regionOfFocus()).toBe("terminal");
});

it("has no button while the daemon has not been read from, where the rail holds only Remote devices", async () => {
  stubWindow(1400);
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  mountApp(fakeDaemon({ connectionState: "connecting" }), { platform: DESKTOP });
  await act(async () => {});
  expect(document.querySelector("[data-region=rail]")).not.toBeNull();
  expect(tasksButton()).toBeNull();
});

it("shows a session a run started in place of the page, though it is a session of the console the page was opened over", async () => {
  const started = sessionOf("s-started", main.id, site.id, "Morning session", "idle");
  const run: ScheduledTaskRun = { id: "r-1", task_id: REPORT.id, ran_at: 1, due_at: null, trigger: "run_now", outcome: { kind: "started", session: started.id, started_independent: false } };
  await mountWindow([REPORT, CLEANUP], undefined, { runs: [run], sessions: [started] });
  await showPage();
  expect(topBar().textContent).not.toContain(started.title);
  await pressElement([...page()!.querySelectorAll<HTMLElement>("[role=tab]")].find((tab) => tab.textContent === "Run history"));
  const link = [...page()!.querySelectorAll<HTMLButtonElement>("li button")].find((button) => button.textContent?.includes(started.title));
  await pressElement(link);
  expect(page()).toBeNull();
  expect(topBar().textContent).toContain(started.title);
  expect(isCurrent(tasksButton())).toBe(false);
  // The page that held focus is gone, and the terminal of the session shown takes it.
  expect(regionOfFocus()).toBe("terminal");
});

it("opens the page on the task whose failed run's notification is chosen", async () => {
  let choose!: (target: string) => void;
  const platform: PlatformAdapter = {
    kind: "browser",
    host: BROWSER_HOST,
    notifications: {
      ensurePermission: async () => true,
      notify: async () => {},
      onChosen: (listener) => {
        choose = listener;
        return () => {};
      },
    },
  };
  await mountWindow([REPORT, CLEANUP], platform);
  // A target that is none of this page's opens nothing.
  await act(async () => choose("something-else"));
  expect(page()).toBeNull();
  await act(async () => choose("scheduled-task:t-cleanup"));
  expect(page()).not.toBeNull();
  expect(heading()).toBe("Weekly cleanup");
  expect(isCurrent(tasksButton())).toBe(true);
});

it("shows nothing of a failed listing of the tasks, and raises no alert, in a window where the page is not open", async () => {
  stubWindow(1400);
  const daemon = fakeDaemon(snapshotState({ consoles: [main], projects: [site], sessions: [fix] }), (body) => {
    if (body.type === "list_scheduled_tasks") throw daemonError("internal_error", { detail: "database is locked" });
    return defaultDaemonReply(body);
  });
  mountApp(daemon);
  await act(async () => {});
  expect(page()).toBeNull();
  expect(errorAlerts.getSnapshot()).toEqual([]);
  expect(document.body.textContent).not.toContain(t("scheduledTasks.listFailedTitle"));
  // Opened, the page says so in its place.
  await showPage();
  expect(page()!.textContent).toContain(t("scheduledTasks.listFailedTitle"));
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

const newTask = () => document.querySelector<HTMLElement>("[data-region=sidebar] [aria-label='New scheduled task']");

it("ends a new task's form, with nothing typed in it, when a failed run's notification shows a task", async () => {
  let choose!: (target: string) => void;
  const platform: PlatformAdapter = {
    kind: "browser",
    host: BROWSER_HOST,
    notifications: { ensurePermission: async () => true, notify: async () => {}, onChosen: (listener) => ((choose = listener), () => {}) },
  };
  await mountWindow([REPORT, CLEANUP], platform);
  await showPage();
  await pressElement(newTask());
  expect(heading()).toBe("New scheduled task");
  await act(async () => choose("scheduled-task:t-cleanup"));
  expect(heading()).toBe("Weekly cleanup");
  expect(page()!.querySelector("form")).not.toBeNull();
});

it("ends a new task's form, with nothing typed in it, when Back or Forward moves to a task", async () => {
  await mountWindow();
  await showPage();
  await pressElement(sidebarRow("Weekly cleanup"));
  await pressElement(newTask());
  expect(heading()).toBe("New scheduled task");
  await pressElement(topBarButton("Back"));
  expect(heading()).not.toBe("New scheduled task");
  expect(["Morning report", "Weekly cleanup"]).toContain(heading());
  await pressElement(newTask());
  expect(heading()).toBe("New scheduled task");
  expect((topBarButton("Forward") as HTMLButtonElement).disabled).toBe(false);
  await pressElement(topBarButton("Forward"));
  expect(["Morning report", "Weekly cleanup"]).toContain(heading());
});

/** The page and the sidebar beside it, which focus lands in once what held it is gone. */
const focusOnPage = () => document.activeElement?.closest("[data-region=tasks], [data-region=sidebar]") != null;

it.each([
  ["the Delete card", async () => pressElement([...page()!.querySelectorAll<HTMLElement>("button")].find((button) => button.textContent === "Delete"))],
  ["a row's menu", async () => {
    await pressElement([...document.querySelectorAll<HTMLElement>("[data-region=sidebar] button[aria-label]")].find((button) => button.getAttribute("aria-label")!.includes("Actions for scheduled task")));
    await pressMenuItem("Delete");
  }],
])("leaves focus on the page, never on <body>, once a deletion from %s is confirmed", async (_, begin) => {
  framesAsTimers();
  fakeTimersKeepingFrames();
  await mountWindow();
  await showPage();
  await begin();
  await settleFrames();
  const confirm = [...document.body.querySelectorAll<HTMLElement>("[role=alertdialog] button, [role=dialog] button")].find((button) => button.textContent === "Delete");
  // The last input is a key, which `element.click()` does not replace (react-aria reads that as virtual).
  act(() => void document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true })));
  await pressElement(confirm);
  await settleFrames();
  act(() => void vi.advanceTimersByTime(MODAL_CLOSE_SETTLE_MS));
  await settleFrames();
  expect(document.querySelector("[role=alertdialog]")).toBeNull();
  expect(heading()).toBe("Weekly cleanup");
  await act(async () => {});
  expect(document.activeElement).not.toBe(document.body);
  expect(focusOnPage()).toBe(true);
  // A move the user did not make: the ring follows the last input, a key here.
  expect(document.activeElement!.hasAttribute("data-focus-visible")).toBe(true);
});
