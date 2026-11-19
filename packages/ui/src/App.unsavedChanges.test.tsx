// @vitest-environment jsdom
// The question before a page form's changes that are not saved are given up, through each way the window offers of
// leaving the Scheduled tasks page (the Remote devices page's own are in `App.remoteDevices.test.tsx`): what asks, what
// never does, what keeps the page for a change nobody asked about, and where focus goes. The guard's own rules are in
// `components/dialog/unsavedChangesGuard.test.ts` and `UnsavedChanges.test.tsx`.
import { act } from "react";
import { beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { fakeDaemon, mountApp, pressElement, railConsole, regionOfFocus, row, stubWindow } from "./appTestSupport";
import { framesAsTimers, settleFrames } from "./components/dialog/dialogTestSupport";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./fixtures/builders";
import type { PlatformAdapter, RemoteDaemonEntry, StatusItemCapability } from "./platform";
import { pressWithMouse } from "./primitives/testSupport";
import type { ScheduledTask, ScheduledTaskRun } from "./protocol";
import { acknowledgeAllAlerts } from "./test/alertQueue";
import { type, buttons } from "./test/dom";
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

beforeEach(() => acknowledgeAllAlerts());

const main = consoleOf("c-main", "Main");
const second = consoleOf("c-second", "Second");
const site = projectOf("p-site", main.id, "Website");
const fix = sessionOf("s-fix", main.id, site.id, "Fix header", "idle");
const waiting = sessionOf("s-wait", main.id, site.id, "Needs an answer", "waiting_user");
const triage = sessionOf("s-triage", second.id, undefined, "Triage", "idle");
const started = sessionOf("s-started", main.id, site.id, "Morning session", "idle");

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
const RUN: ScheduledTaskRun = { id: "r-1", task_id: REPORT.id, ran_at: 1, due_at: null, trigger: "run_now", outcome: { kind: "started", session: started.id, started_independent: false } };

const KEYED: RemoteDaemonEntry = { id: "keyed", name: "Keyed", host: "keyed.example.com", port: 22, user: "deploy", auth: "password", keyPath: null, fingerprint: null, octoboarddPath: null };
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

/** What the window's chosen notification and menu bar icon hand over, once the window has registered for them. */
const outside: { notification?: (target: string) => void; session?: (id: string) => void } = {};
const platformWithOutsideChoices = (): PlatformAdapter => {
  const statusItem: StatusItemCapability = {
    setMenu: async () => {},
    setVisible: async () => {},
    onSessionChosen: (handler) => {
      outside.session = handler;
      return () => {};
    },
    onOpened: () => () => {},
  };
  return {
    kind: "browser",
    host: BROWSER_HOST,
    statusItem,
    notifications: {
      ensurePermission: async () => true,
      notify: async () => {},
      onChosen: (listener) => {
        outside.notification = listener;
        return () => {};
      },
    },
  };
};

/** The window over two consoles, Main current with a session selected, holding `tasks` as the daemon listed them. */
async function mountWindow({ width = 1400, platform }: { width?: number; platform?: PlatformAdapter } = {}): Promise<Daemon> {
  stubWindow(width);
  const tasks = [REPORT, CLEANUP];
  const daemon = fakeDaemon({
    ...snapshotState({ consoles: [main, second], projects: [site], sessions: [fix, waiting, triage, started] }),
    scheduledTasks: new Map(tasks.map((task) => [task.id, task])),
    scheduledTaskRuns: new Map(tasks.map((task) => [task.id, task.id === REPORT.id ? [RUN] : []])),
  });
  mountApp(daemon, platform ? { platform } : undefined);
  if (width >= 768) await pressElement(row(fix.title));
  return daemon;
}

const tasksButton = () => document.querySelector<HTMLElement>("[data-region=rail] button[aria-label='Scheduled tasks']");
const page = () => document.querySelector<HTMLElement>("[data-region=tasks]");
const heading = () => page()?.querySelector("h1")?.textContent;
const prompt = () => page()!.querySelector<HTMLTextAreaElement>("textarea")!;
const topBar = () => document.querySelector<HTMLElement>("[data-region=topbar]")!;
const topBarButton = (label: string) => topBar().querySelector<HTMLElement>(`[aria-label="${label}"]`);
const isCurrent = (element: Element | null | undefined) => element?.getAttribute("aria-current") === "true";
const sidebarRow = (name: string) => [...document.querySelectorAll<HTMLElement>("[data-region=sidebar] [data-list-motion] [role=button]")].find((one) => one.getAttribute("aria-label") === name);
const asking = () => document.body.querySelector<HTMLElement>('[role="alertdialog"]');
const answer = (name: string) => pressElement(buttons().find((button) => asking()!.contains(button) && button.textContent === name));
const pageButton = (name: string) => buttons().find((button) => page()!.contains(button) && button.textContent === name);

/** Opens the page and gives its form a change that is not saved. */
async function openDirtyPage(): Promise<void> {
  await pressElement(tasksButton());
  type(prompt(), "Not saved");
  await act(async () => {});
}

/** The window's own ways of leaving the page, each taking the window to what `arrived` says. */
const WAYS: [string, () => Promise<void> | void, () => boolean][] = [
  ["another console's avatar", () => pressElement(railConsole(second.name)), () => isCurrent(railConsole(second.name))],
  ["the current console's own avatar", () => pressElement(railConsole(main.name)), () => isCurrent(railConsole(main.name)) && topBar().textContent!.includes(fix.title)],
  ["the rail's count of waiting sessions", () => pressElement(document.querySelector<HTMLElement>("[data-region=rail] button[aria-label*='waiting for you']")), () => topBar().textContent!.includes(waiting.title)],
];

it.each(WAYS)("asks before %s leaves the page, keeping the page and the form on Keep editing, and takes it on Discard", async (_, leave, arrived) => {
  await mountWindow();
  await openDirtyPage();
  await leave();
  expect(asking()).not.toBeNull();
  expect(page()).not.toBeNull();
  expect(arrived()).toBe(false);
  await answer("Keep editing");
  expect(asking()).toBeNull();
  expect(page()).not.toBeNull();
  expect(prompt().value).toBe("Not saved");

  await leave();
  await answer("Discard");
  expect(asking()).toBeNull();
  expect(page()).toBeNull();
  expect(arrived()).toBe(true);
});

it("asks before the menu bar icon's choice of a session leaves the page, and shows that session on Discard", async () => {
  await mountWindow({ platform: platformWithOutsideChoices() });
  await openDirtyPage();
  await act(async () => outside.session!(triage.id));
  expect(asking()).not.toBeNull();
  expect(page()).not.toBeNull();
  await answer("Keep editing");
  expect(prompt().value).toBe("Not saved");
  await act(async () => outside.session!(triage.id));
  await answer("Discard");
  expect(page()).toBeNull();
  expect(isCurrent(railConsole(second.name))).toBe(true);
  expect(topBar().textContent).toContain(triage.title);
});

it("asks before a failed run's notification shows another task, and not when it names the task already shown", async () => {
  await mountWindow({ platform: platformWithOutsideChoices() });
  await openDirtyPage();
  await act(async () => outside.notification!(`scheduled-task:${REPORT.id}`));
  expect(asking()).toBeNull();
  expect(prompt().value).toBe("Not saved");
  await act(async () => outside.notification!(`scheduled-task:${CLEANUP.id}`));
  expect(asking()).not.toBeNull();
  expect(heading()).toBe("Morning report");
  await answer("Discard");
  expect(heading()).toBe("Weekly cleanup");
});

it("asks before a run's session opened from Run history leaves the page, the draft still held while that tab shows", async () => {
  await mountWindow();
  await openDirtyPage();
  await pressElement([...page()!.querySelectorAll<HTMLElement>("[role=tab]")].find((tab) => tab.textContent === "Run history"));
  const link = () => [...page()!.querySelectorAll<HTMLButtonElement>("li button")].find((button) => button.textContent?.includes(started.title));
  await pressElement(link());
  expect(asking()).not.toBeNull();
  expect(page()).not.toBeNull();
  await answer("Keep editing");
  expect(page()).not.toBeNull();
  await pressElement(link());
  await answer("Discard");
  expect(page()).toBeNull();
  expect(topBar().textContent).toContain(started.title);
  expect(regionOfFocus()).toBe("terminal");
});

const shortcut = (key: "[" | "]") =>
  act(async () => void window.dispatchEvent(new KeyboardEvent("keydown", { key, code: key === "[" ? "BracketLeft" : "BracketRight", metaKey: true, bubbles: true, cancelable: true })));

/** The desktop app on macOS, where Back and Forward are the shortcuts ⌘[ and ⌘]. */
const macApp: PlatformAdapter = { kind: "tauri", host: MAC_HOST, windowChrome: { leftInset: () => 0, subscribe: () => () => {} } } as never;

it("asks for the Back and Forward shortcuts, and makes the move only once Discard is answered", async () => {
  await mountWindow({ platform: macApp });
  await pressElement(tasksButton());
  await pressElement(sidebarRow(CLEANUP.launch.title!));
  await pressElement(topBarButton("Back"));
  expect(heading()).toBe("Morning report");
  type(prompt(), "Not saved");
  await act(async () => {});
  await shortcut("]");
  expect(asking()).not.toBeNull();
  expect(heading()).toBe("Morning report");
  await answer("Keep editing");
  expect(heading()).toBe("Morning report");
  await shortcut("]");
  await answer("Discard");
  expect(heading()).toBe("Weekly cleanup");

  type(prompt(), "Not saved again");
  await act(async () => {});
  await shortcut("[");
  expect(asking()).not.toBeNull();
  await answer("Discard");
  expect(heading()).toBe("Morning report");
});

it("asks before the Remote devices page's rail button leaves the page, and shows that page on Discard", async () => {
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  onTestFinished(() => void vi.unstubAllGlobals());
  await mountWindow({ platform: DESKTOP });
  await openDirtyPage();
  const devices = () => document.querySelector<HTMLElement>("[data-region=rail] button[aria-label^='Remote devices']");
  await pressElement(devices());
  expect(asking()).not.toBeNull();
  expect(page()).not.toBeNull();
  await answer("Keep editing");
  expect(document.querySelector("[data-region=devices]")).toBeNull();
  await pressElement(devices());
  await answer("Discard");
  expect(page()).toBeNull();
  expect(document.querySelector("[data-region=devices]")).not.toBeNull();
});

it("asks nothing for the page's own rail button, the Settings and Run history tabs, Revert, or a form with nothing changed", async () => {
  await mountWindow();
  await pressElement(tasksButton());
  type(prompt(), "Not saved");
  await act(async () => {});
  await pressElement(tasksButton());
  expect(asking()).toBeNull();
  expect(page()).not.toBeNull();
  const tab = (name: string) => [...page()!.querySelectorAll<HTMLElement>("[role=tab]")].find((one) => one.textContent === name);
  await pressElement(tab("Run history"));
  await pressElement(tab("Settings"));
  expect(asking()).toBeNull();
  expect(prompt().value).toBe("Not saved");
  await pressElement(pageButton("Revert"));
  expect(asking()).toBeNull();
  expect(prompt().value).toBe(REPORT.prompt);
  // With nothing left changed, leaving asks nothing.
  await pressElement(railConsole(second.name));
  expect(asking()).toBeNull();
  expect(page()).toBeNull();
});

it("keeps the page and the form when the session it was opened over is deleted, and still asks about the user's own leave", async () => {
  const daemon = await mountWindow();
  await openDirtyPage();
  await act(async () => applyDaemonEvent(daemon.store, { type: "session_deleted", session: fix.id }));
  expect(page()).not.toBeNull();
  expect(prompt().value).toBe("Not saved");
  expect(asking()).toBeNull();
  await pressElement(railConsole(second.name));
  expect(asking()).not.toBeNull();
  await answer("Discard");
  expect(page()).toBeNull();
});

it("leaves the page when the session under it is deleted and nothing was changed", async () => {
  const daemon = await mountWindow();
  await pressElement(tasksButton());
  await act(async () => applyDaemonEvent(daemon.store, { type: "session_deleted", session: fix.id }));
  expect(page()).toBeNull();
});

it("returns focus to the drawer control that was pressed when Keep editing is answered in a narrow window", async () => {
  framesAsTimers();
  await mountWindow({ width: 390 });
  await pressElement(topBarButton("Show sessions"));
  await pressElement(tasksButton());
  expect(topBarButton("Hide scheduled tasks")).not.toBeNull();
  type(prompt(), "Not saved");
  await act(async () => {});
  const target = sidebarRow(CLEANUP.launch.title!)!;
  pressWithMouse(target);
  await settleFrames();
  expect(asking()).not.toBeNull();
  await answer("Keep editing");
  await settleFrames();
  expect(asking()).toBeNull();
  expect(document.activeElement).toBe(target);
  expect(heading()).toBe("Morning report");
  expect(prompt().value).toBe("Not saved");
});

it("opens a connection's question over the Scheduled tasks page without leaving it or dropping its form, and asks nothing", async () => {
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  onTestFinished(() => void vi.unstubAllGlobals());
  let handOver: ((items: { id: string; outcome: { outcome: "no_daemon" } }[]) => void) | undefined;
  const platform: PlatformAdapter = {
    ...DESKTOP,
    remoteDaemons: { ...DESKTOP.remoteDaemons!, onHandedOver: (listener) => ((handOver = listener as typeof handOver), () => {}) },
  };
  await mountWindow({ platform });
  await openDirtyPage();
  await act(async () => handOver!([{ id: KEYED.id, outcome: { outcome: "no_daemon" } }]));
  expect(page()).not.toBeNull();
  expect(document.querySelector("[data-region=devices]")).toBeNull();
  expect(prompt().value).toBe("Not saved");
  const question = [...document.body.querySelectorAll<HTMLElement>("[role=dialog], [role=alertdialog]")].at(-1);
  expect(question?.textContent).toContain("No daemon is running on");
  expect(question?.textContent).not.toContain("have not been saved");
});
