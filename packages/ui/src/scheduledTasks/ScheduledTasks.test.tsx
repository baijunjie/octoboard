// @vitest-environment jsdom
// The Scheduled tasks page's sidebar and content area over a store the daemon's replies are applied to, as the window
// shows them: the list, the empty state, the form's validation, save, revert, the enabled switch, delete, Run now, the
// recent runs and the times in the daemon's zone.
import { act, useEffect } from "react";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { detailsButton, openDetails } from "../components/alerts/detailsTestSupport";
import { TICK_MS } from "../format/useNow";
import { useScheduledTasksPage } from "../app/useScheduledTasksPage";
import { accountOf, consoleOf, projectOf, sessionOf } from "../fixtures/builders";
import { closeOpenTooltip, CONTROL_TOOLTIP_DELAY_MS, enter, leave, openTooltip, tooltipsCooledDown } from "../components/hint/tooltipTestSupport";
import { translatorFor } from "../i18n/catalog";
import { setLanguage, t } from "../i18n/language";
import { UnsavedChangesProvider, useUnsavedChangesGuard } from "../components/dialog/UnsavedChanges";
import { LanguageProvider } from "../i18n/react";
import { errorAlerts } from "../components/alerts/errorAlerts";
import type { Account, AccountQuota, ClientRequestBody, Event, ScheduledTask, ScheduledTaskRun, Session } from "../protocol";
import { applyDaemonEvent, type Daemon } from "../store";
import { acknowledgeAllAlerts } from "../test/alertQueue";
import { daemonError } from "../test/daemonError";
import { fakeTimers } from "../test/clock";
import { clearSegment, optionElements, pickerNamed, segmentOf, type, typeSegment, withoutIsolates } from "../test/dom";
import { partSelector } from "../primitives";
import { mountHost } from "../test/mountHost";
import { defaultDaemonReply, testDaemon } from "../test/testDaemon";
import { ScheduledTaskPage } from "./ScheduledTaskPage";
import { scheduledTaskPreference } from "./scheduledTaskPreference";
import { ScheduledTasksProvider } from "./ScheduledTasks";
import { ScheduledTasksSidebar } from "./ScheduledTasksSidebar";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const main = consoleOf("c-main", "Main");
const site = projectOf("p-site", main.id, "Website");
const spare = consoleOf("c-spare", "Spare");
const garage = projectOf("p-garage", spare.id, "Garage");

function taskOf(id: string, extra: Partial<ScheduledTask> = {}): ScheduledTask {
  return {
    id,
    console_id: main.id,
    project_id: null,
    launch: { compatibility: {} },
    prompt: "Summarise yesterday's commits",
    schedule: { kind: "daily", time: { hour: 9, minute: 0 } },
    enabled: true,
    created_at: 1,
    updated_at: 1,
    next_run_at: null,
    last_run_at: null,
    run_sessions: [],
    ...extra,
  };
}

const REPORT = taskOf("t-report", { launch: { title: "Morning report", compatibility: {} } });

/** The tasks the daemon holds, which its replies are made from. */
let held: ScheduledTask[] = [];
/** The runs it keeps, by task. */
let heldRuns: Record<string, ScheduledTaskRun[]> = {};
/** The zone its listing names. */
let zone: string | null = null;
/** What answers a `run_scheduled_task`, which a test can hold back. */
let runAnswer: () => Promise<void> = async () => {};
/** What answers a `create_scheduled_task`, which a test can hold back. */
let createAnswer: () => Promise<void> = async () => {};
/** The accounts the daemon holds and its readings of their quotas. */
let heldAccounts: Account[] = [];
let heldQuotas: AccountQuota[] = [];
/** Whether the daemon holds no console, so that a task has nothing to start a session in. */
let noConsoles = false;
/** Fails the listing of the tasks, or of the runs, while set. */
let listFailing = false;
let runsFailing = false;
/** Refuses the saving of a task (create, update, enable or disable) while set, or only its enabling or disabling. */
let saveRefused = false;
let enableRefused = false;
/** The `updated_at` the daemon stamped its last change with. As the daemon does, every stored change is stamped past the
 * one before, and a request that changes nothing leaves the task's stamp alone. */
let stamp = 1;
/** Holds back the broadcast of every save until `deliverBroadcasts`, so that its reply arrives first. */
let broadcastsLate = false;
let lateBroadcasts: Event[] = [];
/** What answers an `update_scheduled_task`, which a test can hold back. */
let updateAnswer: () => Promise<void> = async () => {};
const sent = vi.fn<(body: ClientRequestBody) => void>();
const opened = vi.fn<(session: Session) => void>();
const newConsole = vi.fn<() => void>();
beforeEach(() => {
  setLanguage("en");
  acknowledgeAllAlerts();
  held = [REPORT];
  heldRuns = {};
  zone = null;
  runAnswer = async () => {};
  createAnswer = async () => {};
  heldAccounts = [];
  heldQuotas = [];
  noConsoles = false;
  listFailing = false;
  runsFailing = false;
  saveRefused = false;
  enableRefused = false;
  stamp = 1;
  broadcastsLate = false;
  lateBroadcasts = [];
  updateAnswer = async () => {};
  sent.mockClear();
  opened.mockClear();
  newConsole.mockClear();
});
// What a test remembers as the task last chosen is held in module memory, which nothing else resets.
afterEach(() => scheduledTaskPreference.set(undefined));

const fixSession = sessionOf("s-fix", main.id, site.id, "Fix header", "idle");
/** The console sessions a project task can report to: one running, one archived. */
const hub = sessionOf("s-hub", main.id, undefined, "Hub", "idle");
const oldHub = sessionOf("s-old", main.id, undefined, "Old hub", "archived");
/** One that has only just been asked for, which a task can report to all the same. */
const newHub = sessionOf("s-new", main.id, undefined, "New hub", "starting");

function runOf(id: string, outcome: ScheduledTaskRun["outcome"], extra: Partial<ScheduledTaskRun> = {}): ScheduledTaskRun {
  return { id, task_id: REPORT.id, ran_at: new Date(2026, 10, 18, 9, 0).getTime(), due_at: null, trigger: "run_now", outcome, ...extra };
}

/** A daemon whose replies change what it holds and land in the store, as the real one's do. */
function daemonOver(): Daemon {
  const daemon: Daemon = testDaemon({
    state: { hosts: new Map(), connectionState: "open", snapshotEpoch: 1, consoles: new Map((noConsoles ? [] : [main, spare]).map((one) => [one.id, one])), projects: new Map((noConsoles ? [] : [site, garage]).map((one) => [one.id, one])), sessions: new Map([fixSession, hub, oldHub, newHub].map((one) => [one.id, one])) },
    request: async (body) => {
      sent(body);
      let reply: Event;
      switch (body.type) {
        case "list_scheduled_tasks":
          if (listFailing) throw daemonError("internal_error", { detail: "database is locked" });
          reply = { type: "scheduled_tasks_listed", id: null, tasks: held, time_zone: zone };
          break;
        case "create_scheduled_task": {
          await createAnswer();
          if (saveRefused) throw daemonError("internal_error", { detail: "database is locked" });
          const task = taskOf(`t-${held.length + 1}`, { console_id: body.console_id, project_id: body.project_id ?? null, launch: body.launch ?? { compatibility: {} }, prompt: body.prompt, schedule: body.schedule, enabled: body.enabled ?? true });
          held = [...held, task];
          reply = { type: "scheduled_task_saved", id: null, task };
          break;
        }
        case "update_scheduled_task":
        case "set_scheduled_task_enabled": {
          if (body.type === "update_scheduled_task") await updateAnswer();
          if (saveRefused || (enableRefused && body.type === "set_scheduled_task_enabled")) throw daemonError("internal_error", { detail: "database is locked" });
          const current = held.find((one) => one.id === body.task)!;
          const { type: _type, task: _id, ...change } = body;
          // A task already in the state asked for is left as it is, its updated_at included.
          const unchanged = body.type === "set_scheduled_task_enabled" && current.enabled === body.enabled;
          const task = unchanged ? current : { ...current, ...change, updated_at: ++stamp };
          held = held.map((one) => (one.id === task.id ? task : one));
          reply = { type: "scheduled_task_saved", id: null, task };
          break;
        }
        case "list_agent_models":
          reply = { type: "agent_models", id: null, agent: body.agent, account: body.account ?? null, models: [{ id: "opus", name: "Opus", efforts: ["low", "high"] }, { id: "sonnet", name: "Sonnet", efforts: ["low", "high"] }], unavailable: null };
          break;
        case "list_scheduled_task_runs":
          if (runsFailing) throw daemonError("internal_error", { detail: "database is locked" });
          reply = { type: "scheduled_task_runs_listed", id: null, task: body.task, runs: heldRuns[body.task] ?? [] };
          break;
        case "run_scheduled_task":
          await runAnswer();
          return { type: "ack", id: null };
        case "delete_scheduled_task":
          held = held.filter((one) => one.id !== body.task);
          applyDaemonEvent(daemon.store, { type: "scheduled_task_deleted", task: body.task });
          return { type: "ack", id: null };
        default:
          // Whatever else the page's controls ask of the daemon is answered as the window's tests answer ambient reads.
          return defaultDaemonReply(body);
      }
      // The daemon broadcasts a save before it replies, though the two are not ordered on the socket; a reply does not
      // overwrite a task a window holds.
      if (reply.type === "scheduled_task_saved") {
        const broadcast: Event = { type: "scheduled_task_upserted", task: reply.task };
        if (broadcastsLate) lateBroadcasts.push(broadcast);
        else applyDaemonEvent(daemon.store, broadcast);
      }
      applyDaemonEvent(daemon.store, reply);
      return reply;
    },
  });
  if (heldAccounts.length > 0 || heldQuotas.length > 0) daemon.store.setState((state) => ({ settings: { ...state.settings, accounts: heldAccounts }, accountQuotas: heldQuotas }));
  return daemon;
}

/** The window's part of the page: the hook that says what it shows, opened once. */
function Harness() {
  const unsaved = useUnsavedChangesGuard();
  const page = useScheduledTasksPage({ available: true, consoleId: main.id, sessionId: undefined, held: unsaved.dirty });
  useEffect(() => page.show(), []);
  return (
    <UnsavedChangesProvider guard={unsaved.guard}>
      <ScheduledTasksProvider tasks={page.tasks} listing={page.listing} selectedId={page.task} adding={page.adding} onSelect={page.select} onAdd={page.startAdd} onCancelAdd={page.cancelAdd} onOpenSession={opened}>
        <nav><ScheduledTasksSidebar /></nav>
        {page.shown && <ScheduledTaskPage consoleId={main.id} onNewConsole={newConsole} />}
      </ScheduledTasksProvider>
      {unsaved.confirmation}
    </UnsavedChangesProvider>
  );
}

async function mount() {
  const daemon = daemonOver();
  let container!: HTMLElement;
  await act(async () => {
    container = mountHost(<LanguageProvider><Harness /></LanguageProvider>, { daemon });
  });
  return { container, daemon };
}

/** Delivers the broadcasts `broadcastsLate` held back, in the order they were sent. */
const deliverBroadcasts = (daemon: Daemon) => act(async () => {
  for (const event of lateBroadcasts.splice(0)) applyDaemonEvent(daemon.store, event);
});

const sidebar = (container: ParentNode) => container.querySelector("nav")!;
const page = (container: ParentNode) => container.querySelector<HTMLElement>("[data-region=tasks]");

/** What the form's date and time field shows, its segments read in the order the English locale draws them. */
const shownDateTime = (scope: ParentNode) => ["month", "day", "year", "hour", "minute", "dayPeriod"].map((part) => segmentOf(scope, part).textContent).join(" ");

/** Types a date and time into the form's date and time field, a segment at a time, in the 12-hour order of English. */
function enterDateTime(scope: ParentNode, [year, month, day, hour, minute]: [number, number, number, number, number]): void {
  const two = (value: number) => String(value).padStart(2, "0");
  typeSegment(segmentOf(scope, "month"), two(month));
  typeSegment(segmentOf(scope, "day"), two(day));
  typeSegment(segmentOf(scope, "year"), String(year));
  typeSegment(segmentOf(scope, "hour"), two(hour % 12 || 12));
  typeSegment(segmentOf(scope, "minute"), two(minute));
  typeSegment(segmentOf(scope, "dayPeriod"), hour < 12 ? "A" : "P");
}
const heading = (container: ParentNode) => page(container)?.querySelector("h1")?.textContent;
const rows = (container: ParentNode) => [...sidebar(container).querySelectorAll<HTMLElement>("[data-list-motion] [role=button]")];
/** The icons-and-words lines of a row, without its description. */
const meta = (row: HTMLElement) => withoutIsolates([...row.querySelectorAll("[data-task-meta]")].map((line) => line.textContent).join(""));
const when = (row: HTMLElement) => row.querySelector("[data-task-when]")?.textContent;
const named = (root: ParentNode, label: string) => [...root.querySelectorAll<HTMLElement>("[aria-label]")].find((one) => withoutIsolates(one.getAttribute("aria-label")) === label);
const buttonNamed = (root: ParentNode, name: string) => [...root.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent?.trim() === name);
const press = (element: HTMLElement | undefined) => act(async () => element!.click());
const modal = () => [...document.body.querySelectorAll<HTMLElement>("[role=dialog], [role=alertdialog]")].at(-1);
const menuItem = (name: string) => [...document.body.querySelectorAll<HTMLElement>("[role=menuitem]")].find((item) => item.textContent?.trim() === name);
const prompt = (container: ParentNode) => page(container)!.querySelector("textarea")!;
const lastSent = (kind: ClientRequestBody["type"]) => sent.mock.calls.map(([body]) => body).filter((body) => body.type === kind).at(-1);
const isDisabled = (element: HTMLElement | undefined) => (element as HTMLButtonElement).disabled || element!.getAttribute("aria-disabled") === "true";

async function choose(picker: string, option: string) {
  await press(pickerNamed(picker));
  await press(optionElements().find((one) => withoutIsolates(one.textContent)?.includes(option)));
}

/** The evening of Thursday, Nov 19, 2026 in the window's zone, which the tests below that show times read as now. */
const EVENING = new Date(2026, 10, 19, 23, 0);

it("lists each task with what it starts, its schedule and when it next runs, and shows the first", async () => {
  fakeTimers({ now: EVENING });
  const at = new Date(2026, 10, 20, 9, 0).getTime();
  held = [
    { ...REPORT, next_run_at: at },
    taskOf("t-deploy", { project_id: site.id, prompt: "Check the deploy\nand report", schedule: { kind: "weekly", time: { hour: 17, minute: 30 }, days: ["friday", "monday"] }, enabled: false }),
  ];
  const { container } = await mount();
  expect(sidebar(container).querySelector("h2")?.textContent).toBe("Scheduled tasks");
  const [report, deploy] = rows(container);
  expect(report.getAttribute("aria-label")).toBe("Morning report");
  // The line under the name is a few words, the schedule first, and the countdown at its end; the long wording is the
  // row's description.
  expect(meta(report)).toBe("Daily 9:00 AM · Main");
  expect(when(report)).toBe("in 10 hr");
  expect(withoutIsolates(report.textContent)).toContain("Console session in Main · Every day at 9:00 AM · Next run Fri, Nov 20, 2026, 9:00 AM");
  expect(report.getAttribute("aria-describedby")).toBe(report.querySelector(".sr-only")!.id);
  // Unnamed, a task goes by its prompt's first line; one that is off says so instead of a next run.
  expect(withoutIsolates(deploy.textContent)).toContain("Project session in Website · Mon and Fri at 5:30 PM · Off");
  expect(meta(deploy)).toBe("Mon, Fri 5:30 PM · Website");
  expect(when(deploy)).toBe("Off");
  expect(heading(container)).toBe("Morning report");
  expect(report.getAttribute("aria-current")).toBe("true");
  await press(deploy);
  expect(heading(container)).toBe("Check the deploy");
});

it("shows the empty state with none, whose action opens a new task's form, and a compact row in the sidebar", async () => {
  held = [];
  const { container } = await mount();
  expect(rows(container)).toEqual([]);
  expect(sidebar(container).textContent).toContain("No scheduled tasks yet");
  expect(heading(container)).toBe("No scheduled tasks yet");
  expect(page(container)!.textContent).toContain("A scheduled task starts a session at the times you choose");
  await press(buttonNamed(page(container)!, "New scheduled task"));
  expect(heading(container)).toBe("New scheduled task");
  expect(page(container)!.querySelector("form")).not.toBeNull();
});

it("says under the prompt that it is required, saves nothing, and moves focus there", async () => {
  const { container } = await mount();
  await press(named(sidebar(container), "New scheduled task"));
  await press(buttonNamed(page(container)!, "Save"));
  expect(page(container)!.textContent).toContain("Enter a prompt.");
  expect(document.activeElement).toBe(prompt(container));
  expect(lastSent("create_scheduled_task")).toBeUndefined();
});

it("asks for at least one day of a weekly schedule", async () => {
  const { container } = await mount();
  await press(named(sidebar(container), "New scheduled task"));
  type(prompt(container), "Tidy the backlog");
  await choose("Runs", "Every week");
  // A new weekly schedule starts on Monday, which is turned off here.
  await press([...page(container)!.querySelectorAll<HTMLInputElement>("input[type=checkbox]")].find((input) => input.closest("label")?.textContent?.trim() === "Monday"));
  await press(buttonNamed(page(container)!, "Save"));
  expect(page(container)!.textContent).toContain("Choose at least one day.");
  expect(lastSent("create_scheduled_task")).toBeUndefined();
});

it("creates a task from the form and then shows it", async () => {
  const { container } = await mount();
  await press(named(sidebar(container), "New scheduled task"));
  type(prompt(container), "  Tidy the backlog  ");
  await choose("Project", "Website");
  await choose("Runs", "Every hour");
  await press(buttonNamed(page(container)!, "Save"));
  const created = lastSent("create_scheduled_task") as Extract<ClientRequestBody, { type: "create_scheduled_task" }>;
  expect(created).toMatchObject({ console_id: main.id, project_id: site.id, prompt: "Tidy the backlog", schedule: { kind: "hourly", minute: 0 } });
  expect(created.launch?.agent).toBeDefined();
  expect(created.launch?.bound_to).toBeUndefined();
  expect(heading(container)).toBe("Tidy the backlog");
  expect(rows(container)).toHaveLength(2);
  // A task the form leaves enabled is created as the daemon creates one, with nothing more to send.
  expect(lastSent("set_scheduled_task_enabled")).toBeUndefined();
});

it("creates a monthly task on today's day of the month, the last-day note shown from the 29th", async () => {
  fakeTimers({ now: EVENING });
  const { container } = await mount();
  await press(named(sidebar(container), "New scheduled task"));
  type(prompt(container), "Close the books");
  await choose("Runs", "Every month");
  const day = () => page(container)!.querySelector<HTMLInputElement>("[data-slot=number-field-group] input")!;
  // The test's evening is Thursday, Nov 19.
  expect(day().value).toBe("19");
  expect(page(container)!.textContent).toContain("Day of month");
  expect(page(container)!.textContent).not.toContain("runs on its last day");
  for (let step = 0; step < 12; step++) await press(named(page(container)!, "Increase day"));
  expect(day().value).toBe("31");
  expect(page(container)!.textContent).toContain("A month without this day runs on its last day.");
  await press(buttonNamed(page(container)!, "Save"));
  expect(lastSent("create_scheduled_task")).toMatchObject({ prompt: "Close the books", schedule: { kind: "monthly", day: 31, time: { hour: 9, minute: 0 } } });
});

it("shows a saved monthly task's day and time, saves a changed day alone, and asks for its time", async () => {
  fakeTimers({ now: EVENING });
  held = [{ ...REPORT, schedule: { kind: "monthly", day: 15, time: { hour: 7, minute: 30 } } }];
  const { container } = await mount();
  expect(meta(rows(container)[0])).toBe("Monthly, day 15 7:30 AM · Main");
  const day = page(container)!.querySelector<HTMLInputElement>("[data-slot=number-field-group] input")!;
  expect(day.value).toBe("15");
  await press(named(page(container)!, "Decrease day"));
  await press(buttonNamed(page(container)!, "Save"));
  expect(lastSent("update_scheduled_task")).toEqual({ type: "update_scheduled_task", task: REPORT.id, schedule: { kind: "monthly", day: 14, time: { hour: 7, minute: 30 } } });

  clearSegment(segmentOf(page(container)!, "minute"));
  await act(async () => (document.activeElement as HTMLElement).blur());
  expect(page(container)!.textContent).toContain("Enter a time.");
  sent.mockClear();
  await press(buttonNamed(page(container)!, "Save"));
  expect(lastSent("update_scheduled_task")).toBeUndefined();
});

it("saves only what was changed of a saved task, and Revert drops a change", async () => {
  const { container } = await mount();
  const save = () => buttonNamed(page(container)!, "Save");
  const revert = () => buttonNamed(page(container)!, "Revert");
  expect(isDisabled(save())).toBe(true);
  expect(isDisabled(revert())).toBe(true);

  type(prompt(container), "Something else");
  await act(async () => {});
  expect(isDisabled(save())).toBe(false);
  await press(revert());
  expect(prompt(container).value).toBe(REPORT.prompt);
  expect(isDisabled(save())).toBe(true);

  type(prompt(container), "Summarise last week");
  await press(save());
  expect(lastSent("update_scheduled_task")).toEqual({ type: "update_scheduled_task", task: REPORT.id, prompt: "Summarise last week" });
  expect(isDisabled(save())).toBe(true);
});

it("turns a task off and on from its row's menu, and from the form's Enabled switch with Save", async () => {
  const { container } = await mount();
  await press(named(sidebar(container), "Actions for scheduled task Morning report"));
  await press(menuItem("Disable"));
  expect(lastSent("set_scheduled_task_enabled")).toEqual({ type: "set_scheduled_task_enabled", task: REPORT.id, enabled: false });
  expect(rows(container)[0].textContent).toContain("Off");

  const enabled = named(page(container)!, "Enabled") as HTMLInputElement;
  expect(enabled.checked).toBe(false);
  await press(enabled);
  await press(buttonNamed(page(container)!, "Save"));
  expect(lastSent("set_scheduled_task_enabled")).toEqual({ type: "set_scheduled_task_enabled", task: REPORT.id, enabled: true });
  expect(lastSent("update_scheduled_task")).toBeUndefined();
  expect(rows(container)[0].textContent).not.toContain("Off");
});

it("asks before deleting a task, naming it, and shows the next one after", async () => {
  held = [REPORT, taskOf("t-other", { launch: { title: "Other", compatibility: {} } })];
  const { container } = await mount();
  await press(named(page(container)!, "Delete Morning report"));
  expect(lastSent("delete_scheduled_task")).toBeUndefined();
  expect(withoutIsolates(modal()!.textContent)).toContain("Morning report will be deleted.");
  await press(buttonNamed(modal()!, "Delete"));
  expect(lastSent("delete_scheduled_task")).toEqual({ type: "delete_scheduled_task", task: REPORT.id });
  expect(rows(container).map((row) => row.getAttribute("aria-label"))).toEqual(["Other"]);
  expect(heading(container)).toBe("Other");
});

it("runs a task now from its header and its row's menu, whatever its schedule and whether it is on, but not twice at once", async () => {
  held = [{ ...REPORT, schedule: { kind: "manual" }, enabled: false }];
  let answer!: () => void;
  runAnswer = () => new Promise((resolve) => (answer = resolve));
  const { container } = await mount();
  const runNow = () => named(page(container)!, "Run now: Morning report");
  await press(runNow());
  expect(lastSent("run_scheduled_task")).toEqual({ type: "run_scheduled_task", task: REPORT.id });
  // Its own request is awaiting its answer: neither Run now can ask again.
  expect(isDisabled(runNow())).toBe(true);
  await press(named(sidebar(container), "Actions for scheduled task Morning report"));
  expect(document.body.querySelector("[role=menuitem]")?.textContent?.trim()).toBe("Run now");
  expect(isDisabled(menuItem("Run now"))).toBe(true);
  await act(async () => answer());
  expect(isDisabled(runNow())).toBe(false);

  sent.mockClear();
  runAnswer = async () => {};
  await press(named(sidebar(container), "Actions for scheduled task Morning report"));
  await press(menuItem("Run now"));
  expect(lastSent("run_scheduled_task")).toEqual({ type: "run_scheduled_task", task: REPORT.id });
});

it("shows the task's schedule, next run with a countdown and last run in its header", async () => {
  fakeTimers({ now: EVENING });
  held = [{ ...REPORT, next_run_at: new Date(2026, 10, 20, 9, 0).getTime(), last_run_at: new Date(2026, 10, 19, 9, 0).getTime() }];
  const { container } = await mount();
  const figures = (root: HTMLElement) => [...page(root)!.querySelectorAll("header dl > div")].map((one) => [...one.querySelectorAll("dt, dd > *")].map((part) => withoutIsolates(part.textContent)));
  // What it starts and its schedule in words, then the two figures: the countdown and the time ago over their times.
  expect(withoutIsolates(page(container)!.querySelector("header p")!.textContent)).toBe("MainConsole session in Main·Every day at 9:00 AM");
  expect(figures(container)).toEqual([
    ["Next run", "in 10 hr", "Fri, Nov 20, 2026, 9:00 AM"],
    ["Last run", "14 hours ago", "Thu, Nov 19, 2026, 9:00 AM"],
  ]);
  // What changes as the minutes pass is kept from assistive technology, which reads the times.
  expect([...page(container)!.querySelectorAll("header dd [aria-hidden=true]")].map((one) => one.textContent)).toEqual(["in 10 hr", "14 hours ago"]);
  // The countdown moves with the shared clock, a minute at a time.
  await act(async () => void vi.advanceTimersByTime(TICK_MS * 2));
  expect(figures(container)[0]![1]).toBe("in 9 hr 59 min");
  held = [taskOf("t-new", { schedule: { kind: "manual" } })];
  const second = await mount();
  expect(withoutIsolates(page(second.container)!.querySelector("header p")!.textContent)).toContain("Manual only");
  expect(figures(second.container)).toEqual([["Next run", "None"], ["Last run", "Never"]]);
});

it("pins Save and Revert below the form's scroll, and Save and Cancel for a new task, and gives a new task no Run history", async () => {
  const { container } = await mount();
  const bar = (): HTMLElement => buttonNamed(page(container)!, "Save")!.parentElement!.parentElement!;
  expect(bar().closest("[class*=overflow]")).toBeNull();
  expect(bar().previousElementSibling?.querySelector("form")).not.toBeNull();
  expect(buttonNamed(bar(), "Revert")).toBeDefined();
  await press(named(sidebar(container), "New scheduled task"));
  expect(buttonNamed(bar(), "Cancel")).toBeDefined();
  expect(tabNamed(container, "Run history")).toBeUndefined();
});

it("keeps what was changed while Run history shows, takes the bar away there, and keeps the view as another task is chosen", async () => {
  held = [REPORT, taskOf("t-other", { launch: { title: "Other", compatibility: {} } })];
  const { container } = await mount();
  type(prompt(container), "Changed prompt");
  await showRuns(container);
  expect(buttonNamed(page(container)!, "Save")).toBeUndefined();
  expect(runsCard(container)).toBeDefined();
  await press(tabNamed(container, "Settings"));
  expect(prompt(container).value).toBe("Changed prompt");
  expect(buttonNamed(page(container)!, "Save")).toBeDefined();
  await showRuns(container);
  // The draft is still held while Run history shows, so choosing another task asks.
  await press(rows(container)[1]);
  await answer("Discard");
  expect(heading(container)).toBe("Other");
  expect(runsCard(container)).toBeDefined();
  expect(lastSent("list_scheduled_task_runs")).toEqual({ type: "list_scheduled_task_runs", task: "t-other" });
});

it("says a task has no runs yet", async () => {
  const { container } = await mount();
  await showRuns(container);
  expect(lastSent("list_scheduled_task_runs")).toEqual({ type: "list_scheduled_task_runs", task: REPORT.id });
  expect(runsCard(container)?.textContent).toContain("No runs yet");
});

it("lists the runs most recent first: when, the trigger and the session started, which opens it", async () => {
  heldRuns = {
    [REPORT.id]: [
      runOf("r-2", { kind: "started", session: fixSession.id, started_independent: false }, { trigger: "scheduled", ran_at: new Date(2026, 10, 19, 9, 0).getTime() }),
      runOf("r-1", { kind: "started", session: "s-gone", started_independent: true }, { trigger: "catch_up" }),
    ],
  };
  const { container } = await mount();
  await showRuns(container);
  const [latest, earlier] = runEntries(container);
  expect(withoutIsolates(latest.textContent)).toBe("Thu, Nov 19, 2026, 9:00 AMScheduledStarted Fix header");
  // A session no longer held says so, without a link; one started independent says why.
  expect(withoutIsolates(earlier.textContent)).toBe("Wed, Nov 18, 2026, 9:00 AMCatch-upStarted a session that no longer existsThe session started without reporting to a console session, because the one it was to report to could not take it.");
  expect(earlier.querySelector("button")).toBeNull();
  await press(latest.querySelector("button")!);
  expect(opened).toHaveBeenCalledWith(fixSession);
});

it("words a failed run from its code, with its reason behind Details", async () => {
  heldRuns = { [REPORT.id]: [runOf("r-1", { kind: "failed", code: "some_future_failure", params: {}, message: "spawn failed: no such file" })] };
  const { container } = await mount();
  await showRuns(container);
  const [failed] = runEntries(container);
  expect(failed.textContent).toContain("Run now");
  expect(failed.textContent).toContain("Could not start a session");
  // Nothing of the reason is in the row: it is behind the Details button.
  expect(failed.querySelector("details")).toBeNull();
  expect(failed.textContent).not.toContain("spawn failed");
  expect(buttonNamed(failed, "Details")).toBeDefined();
});

it("adds a run the daemon records to the list", async () => {
  const { container, daemon } = await mount();
  await showRuns(container);
  await act(async () => applyDaemonEvent(daemon.store, { type: "scheduled_task_run_added", run: runOf("r-new", { kind: "started", session: fixSession.id, started_independent: false }) }));
  expect(runEntries(container)).toHaveLength(1);
  expect(runsCard(container)?.textContent).not.toContain("No runs yet");
});

it("shows and enters times in the daemon's zone, naming it where the window's own differs", async () => {
  // Chatham is 13:45 ahead of UTC in November, a zone no test machine runs in.
  zone = "Pacific/Chatham";
  fakeTimers({ now: EVENING });
  const at = Date.UTC(2027, 10, 20, 0, 0);
  held = [{ ...REPORT, schedule: { kind: "once", run_at: at }, next_run_at: at }];
  const { container } = await mount();
  expect(withoutIsolates(rows(container)[0].textContent)).toContain("Next run Sat, Nov 20, 2027, 1:45 PM GMT+13:45");
  expect(meta(rows(container)[0])).toContain("Nov 20, 1:45 PM");
  expect(withoutIsolates(page(container)!.querySelector("header")!.textContent)).toContain("Times are in Pacific/Chatham, the time zone of the computer that runs the tasks.");
  expect(shownDateTime(page(container)!)).toBe("11 20 2027 1 45 PM");
  enterDateTime(page(container)!, [2027, 11, 21, 8, 0]);
  await press(buttonNamed(page(container)!, "Save"));
  expect(lastSent("update_scheduled_task")).toMatchObject({ schedule: { kind: "once", run_at: Date.UTC(2027, 10, 20, 18, 15) } });
});

it("offers Report to for a project task only, none by default, among the console's live console sessions, and saves the one chosen", async () => {
  const { container } = await mount();
  await press(named(sidebar(container), "New scheduled task"));
  expect(pickerNamed("Report to")).toBeUndefined();
  await choose("Project", "Website");
  const picker = pickerNamed("Report to");
  expect(withoutIsolates(picker.textContent)).toContain("No console session");
  await press(picker);
  const options = optionElements().map((one) => withoutIsolates(one.textContent) ?? "");
  expect(options).toHaveLength(2);
  expect(options[0]).toContain("No console session");
  expect(options[1]).toContain("Hub");
  await press(optionElements()[1]);
  type(prompt(container), "Tidy the backlog");
  await press(buttonNamed(page(container)!, "Save"));
  expect(lastSent("create_scheduled_task")).toMatchObject({ project_id: site.id, launch: { bound_to: hub.id } });

  // Back at a console target, there is nothing to report to, so nothing is saved of it.
  await press(named(sidebar(container), "New scheduled task"));
  await choose("Project", "Website");
  await press(pickerNamed("Report to"));
  await press(optionElements()[1]);
  await choose("Project", "None (console session)");
  expect(pickerNamed("Report to")).toBeUndefined();
  type(prompt(container), "Check the logs");
  await press(buttonNamed(page(container)!, "Save"));
  const second = lastSent("create_scheduled_task") as Extract<ClientRequestBody, { type: "create_scheduled_task" }>;
  expect(second.project_id).toBeUndefined();
  expect(second.launch).not.toHaveProperty("bound_to");
});

it("leaves a new task's form with Cancel, saving nothing and showing the task that was selected", async () => {
  const { container } = await mount();
  await press(named(sidebar(container), "New scheduled task"));
  expect(heading(container)).toBe("New scheduled task");
  expect(buttonNamed(page(container)!, "Save")).toBeDefined();
  expect(buttonNamed(page(container)!, "Revert")).toBeUndefined();
  type(prompt(container), "Something typed");
  await press(buttonNamed(page(container)!, "Cancel"));
  expect(lastSent("create_scheduled_task")).toBeUndefined();
  expect(heading(container)).toBe("Morning report");
});

it("shows what is wrong with the prompt only once it was edited and left, and a blank one counts as none", async () => {
  const { container } = await mount();
  type(prompt(container), "   ");
  await act(async () => {});
  expect(page(container)!.textContent).not.toContain("Enter a prompt.");
  act(() => prompt(container).focus());
  act(() => prompt(container).blur());
  expect(page(container)!.textContent).toContain("Enter a prompt.");
  await press(buttonNamed(page(container)!, "Save"));
  expect(lastSent("update_scheduled_task")).toBeUndefined();
  expect(document.activeElement).toBe(prompt(container));
  type(prompt(container), "Back again");
  await act(async () => {});
  expect(page(container)!.textContent).not.toContain("Enter a prompt.");
});

it("refuses a one-off in the past when it is made, and focuses its field", async () => {
  const { container } = await mount();
  await press(named(sidebar(container), "New scheduled task"));
  type(prompt(container), "Remind me");
  await choose("Runs", "Once");
  enterDateTime(page(container)!, [2020, 1, 1, 10, 0]);
  await press(buttonNamed(page(container)!, "Save"));
  expect(page(container)!.textContent).toContain("Choose a time in the future.");
  expect(document.activeElement).toBe(segmentOf(page(container)!, "month"));
  expect(lastSent("create_scheduled_task")).toBeUndefined();

  enterDateTime(page(container)!, [2999, 1, 1, 10, 0]);
  await press(buttonNamed(page(container)!, "Save"));
  expect(lastSent("create_scheduled_task")).toMatchObject({ schedule: { kind: "once", run_at: new Date(2999, 0, 1, 10, 0).getTime() } });
});

it("saves a one-off whose time has passed with its other fields changed, leaving its schedule out", async () => {
  held = [taskOf("t-ran", { launch: { title: "Reminder", compatibility: {} }, schedule: { kind: "once", run_at: new Date(2020, 0, 1, 10, 0).getTime() } })];
  const { container } = await mount();
  expect(page(container)!.textContent).not.toContain("Choose a time in the future.");
  type(prompt(container), "Another reminder");
  await press(buttonNamed(page(container)!, "Save"));
  expect(lastSent("update_scheduled_task")).toEqual({ type: "update_scheduled_task", task: "t-ran", prompt: "Another reminder" });
});

it("changes a saved task's schedule: the kind, and the time of day", async () => {
  const { container } = await mount();
  await choose("Runs", "Weekdays");
  await press(buttonNamed(page(container)!, "Save"));
  expect(lastSent("update_scheduled_task")).toEqual({ type: "update_scheduled_task", task: REPORT.id, schedule: { kind: "weekdays", time: { hour: 9, minute: 0 } } });
  typeSegment(segmentOf(page(container)!, "hour"), "07");
  typeSegment(segmentOf(page(container)!, "minute"), "30");
  await press(buttonNamed(page(container)!, "Save"));
  expect(lastSent("update_scheduled_task")).toEqual({ type: "update_scheduled_task", task: REPORT.id, schedule: { kind: "weekdays", time: { hour: 7, minute: 30 } } });
});

it("counts a time of day with a segment cleared as empty, says so once the field is left, and refuses to save it", async () => {
  const { container } = await mount();
  clearSegment(segmentOf(page(container)!, "minute"));
  expect(page(container)!.textContent).not.toContain("Enter a time.");
  await act(async () => (document.activeElement as HTMLElement).blur());
  expect(page(container)!.textContent).toContain("Enter a time.");
  await press(buttonNamed(page(container)!, "Save"));
  expect(lastSent("update_scheduled_task")).toBeUndefined();
  expect(document.activeElement).toBe(segmentOf(page(container)!, "hour"));
});

it("lists the selected console's projects after None, and puts Project back to none as the console changes", async () => {
  const { container } = await mount();
  await press(named(sidebar(container), "New scheduled task"));
  expect(withoutIsolates(pickerNamed("Console").textContent)).toContain("Main");
  expect(withoutIsolates(pickerNamed("Project").textContent)).toContain("None (console session)");
  await press(pickerNamed("Project"));
  expect(optionElements().map((one) => withoutIsolates(one.textContent))).toEqual(["None (console session)", "Website"]);
  await press(optionElements()[1]);
  expect(pickerNamed("Report to")).toBeDefined();
  await choose("Console", "Spare");
  expect(withoutIsolates(pickerNamed("Project").textContent)).toContain("None (console session)");
  expect(pickerNamed("Report to")).toBeUndefined();
  await press(pickerNamed("Project"));
  expect(optionElements().map((one) => withoutIsolates(one.textContent))).toEqual(["None (console session)", "Garage"]);
  await press(optionElements()[1]);
  type(prompt(container), "Tidy the garage");
  await press(buttonNamed(page(container)!, "Save"));
  expect(lastSent("create_scheduled_task")).toMatchObject({ console_id: spare.id, project_id: garage.id });
});

it("moves a saved task to a project, naming the console and the project", async () => {
  const { container } = await mount();
  await choose("Project", "Website");
  await press(buttonNamed(page(container)!, "Save"));
  // The console did not change, so only the project is sent.
  expect(lastSent("update_scheduled_task")).toMatchObject({ task: REPORT.id, project_id: site.id });
  expect(lastSent("update_scheduled_task")).not.toHaveProperty("console_id");
});

const asking = () => document.body.querySelector<HTMLElement>('[role="alertdialog"]');
const answer = (name: string) => press(buttonNamed(asking()!, name));

it("asks before what was changed and not saved is dropped when another task is chosen, and keeps it on Keep editing", async () => {
  held = [REPORT, taskOf("t-other", { launch: { title: "Other", compatibility: {} } })];
  const { container } = await mount();
  type(prompt(container), "Not saved");
  await press(rows(container)[1]);
  expect(asking()!.textContent).toContain("The changes in this form have not been saved. They will be lost.");
  expect(heading(container)).toBe("Morning report");
  await answer("Keep editing");
  expect(asking()).toBeNull();
  expect(prompt(container).value).toBe("Not saved");
});

it("shows the other task once Discard is answered, with nothing sent", async () => {
  held = [REPORT, taskOf("t-other", { launch: { title: "Other", compatibility: {} } })];
  const { container } = await mount();
  type(prompt(container), "Not saved");
  await press(rows(container)[1]);
  await answer("Discard");
  expect(asking()).toBeNull();
  expect(heading(container)).toBe("Other");
  await press(rows(container)[0]);
  expect(asking()).toBeNull();
  expect(prompt(container).value).toBe(REPORT.prompt);
  expect(lastSent("update_scheduled_task")).toBeUndefined();
});

it("asks nothing for an untouched form, the task already shown, Revert, or Cancel of a new task", async () => {
  held = [REPORT, taskOf("t-other", { launch: { title: "Other", compatibility: {} } })];
  const { container } = await mount();
  await press(rows(container)[1]);
  expect(asking()).toBeNull();
  await press(rows(container)[0]);
  type(prompt(container), "Not saved");
  await press(rows(container)[0]);
  expect(asking()).toBeNull();
  await press(buttonNamed(page(container)!, "Revert"));
  expect(prompt(container).value).toBe(REPORT.prompt);

  await press(named(sidebar(container), "New scheduled task"));
  expect(asking()).toBeNull();
  // Asking for the form already open changes nothing, and Cancel is the user's own discard, whatever was typed.
  await press(named(sidebar(container), "New scheduled task"));
  type(prompt(container), "Draft");
  await press(buttonNamed(page(container)!, "Cancel"));
  expect(asking()).toBeNull();
  expect(heading(container)).toBe("Morning report");
});

it("asks before a new task's form with something typed is replaced by another task, and keeps the draft on Keep editing", async () => {
  held = [REPORT, taskOf("t-other", { launch: { title: "Other", compatibility: {} } })];
  const { container } = await mount();
  await press(named(sidebar(container), "New scheduled task"));
  type(prompt(container), "Draft");
  await press(rows(container)[1]);
  expect(asking()).not.toBeNull();
  await answer("Keep editing");
  expect(prompt(container).value).toBe("Draft");
  await press(rows(container)[1]);
  await answer("Discard");
  expect(heading(container)).toBe("Other");
});

it("creates a task that is off with it off from the start", async () => {
  const { container } = await mount();
  await press(named(sidebar(container), "New scheduled task"));
  type(prompt(container), "Tidy the backlog");
  await press(named(page(container)!, "Enabled"));
  await press(buttonNamed(page(container)!, "Save"));
  expect(lastSent("create_scheduled_task")).toMatchObject({ enabled: false });
  expect(lastSent("set_scheduled_task_enabled")).toBeUndefined();
  expect(rows(container)[1].textContent).toContain("Off");
});

it("keeps a task when its deletion is cancelled", async () => {
  const { container } = await mount();
  await press(named(page(container)!, "Delete Morning report"));
  await press(buttonNamed(modal()!, "Cancel"));
  expect(modal()).toBeUndefined();
  expect(lastSent("delete_scheduled_task")).toBeUndefined();
  expect(rows(container)).toHaveLength(1);
});

it("raises an error alert when Run now is refused, and lets it be asked again", async () => {
  runAnswer = async () => {
    throw daemonError("daemon_shutting_down", {});
  };
  const { container } = await mount();
  const runNow = () => named(page(container)!, "Run now: Morning report");
  await press(runNow());
  expect(errorAlerts.getSnapshot().map((alert) => alert.title)).toEqual(["Could not run the scheduled task"]);
  expect(isDisabled(runNow())).toBe(false);
});

it("words a failed run of a known code in the application's own words", async () => {
  heldRuns = { [REPORT.id]: [runOf("r-1", { kind: "failed", code: "account_disabled", params: { agent: "Codex", account: "Work" }, message: "raw daemon text" })] };
  const { container } = await mount();
  await showRuns(container);
  const [failed] = runEntries(container);
  expect(failed.textContent).toContain("Could not start a session");
  // The reason and its remedy are behind Details, not written into the row.
  expect(withoutIsolates(failed.textContent)).not.toContain("is disabled, so a session cannot be opened on it.");
  expect(buttonNamed(failed, "Details")).toBeDefined();
});

it("names no zone where the daemon's is the window's own", async () => {
  zone = Intl.DateTimeFormat().resolvedOptions().timeZone;
  fakeTimers({ now: EVENING });
  held = [{ ...REPORT, next_run_at: new Date(2026, 10, 20, 9, 0).getTime() }];
  const { container } = await mount();
  expect(page(container)!.querySelector("header")!.textContent).not.toContain("Times are in");
  expect(withoutIsolates(rows(container)[0].textContent)).toContain("Next run Fri, Nov 20, 2026, 9:00 AM");
  expect(withoutIsolates(rows(container)[0].textContent)).not.toContain("GMT");
});

it("saves the agent picked, and Revert puts the pickers back to what the task holds", async () => {
  held = [taskOf("t-agent", { launch: { agent: "claude", account: null, compatibility: {} } })];
  const { container } = await mount();
  const save = () => buttonNamed(page(container)!, "Save");
  const agentPicker = () => pickerNamed("Agent");
  const before = withoutIsolates(agentPicker().textContent);
  expect(isDisabled(save())).toBe(true);

  await choose("Agent", "Codex");
  expect(withoutIsolates(agentPicker().textContent)).not.toBe(before);
  expect(isDisabled(save())).toBe(false);
  await press(buttonNamed(page(container)!, "Revert"));
  expect(withoutIsolates(agentPicker().textContent)).toBe(before);
  expect(isDisabled(save())).toBe(true);

  await choose("Agent", "Codex");
  await press(save());
  expect(lastSent("update_scheduled_task")).toMatchObject({ task: "t-agent", launch: { agent: "codex" } });
});

/** The header card's figures: each one's label, value and note, without the bidi isolates. */
const figureRows = (container: ParentNode) => [...page(container)!.querySelectorAll("header dl > div")].map((one) => [...one.querySelectorAll("dt, dd > *")].map((part) => withoutIsolates(part.textContent)));
const minuteField = (container: ParentNode) => page(container)!.querySelector<HTMLInputElement>("[data-slot=number-field-group] input")!;
const menuItems = () => [...document.body.querySelectorAll<HTMLElement>("[role=menuitem]")].map((item) => item.textContent?.trim());

it("shows the exact time of the next run in the countdown's tooltip, and opens none on an Off task's mark", async () => {
  await tooltipsCooledDown();
  fakeTimers({ now: EVENING });
  held = [
    { ...REPORT, next_run_at: new Date(2026, 10, 20, 9, 0).getTime() },
    taskOf("t-off", { launch: { title: "Dormant", compatibility: {} }, enabled: false }),
  ];
  const { container } = await mount();
  onTestFinished(closeOpenTooltip);
  const [report, dormant] = rows(container);
  const mark = (row: HTMLElement) => row.querySelector<HTMLElement>("[data-task-when]")!;
  enter(mark(dormant));
  act(() => void vi.advanceTimersByTime(CONTROL_TOOLTIP_DELAY_MS));
  expect(mark(dormant).textContent).toBe("Off");
  expect(openTooltip()).toBeNull();
  leave(mark(dormant));
  enter(mark(report));
  act(() => void vi.advanceTimersByTime(CONTROL_TOOLTIP_DELAY_MS));
  expect(withoutIsolates(openTooltip()?.textContent)).toBe("Fri, Nov 20, 2026, 9:00 AM");
});

it("offers Run now, Disable and Delete in an enabled task's row menu and Enable in a disabled one's, and asks before deleting from it", async () => {
  held = [REPORT, taskOf("t-off", { launch: { title: "Dormant", compatibility: {} }, enabled: false })];
  const { container } = await mount();
  await press(named(sidebar(container), "Actions for scheduled task Morning report"));
  expect(menuItems()).toEqual(["Run now", "Disable", "Delete"]);
  await press(menuItem("Delete"));
  expect(lastSent("delete_scheduled_task")).toBeUndefined();
  expect(withoutIsolates(modal()!.textContent)).toContain("Morning report will be deleted.");
  await press(buttonNamed(modal()!, "Cancel"));
  expect(lastSent("delete_scheduled_task")).toBeUndefined();

  await press(named(sidebar(container), "Actions for scheduled task Dormant"));
  expect(menuItems()).toEqual(["Run now", "Enable", "Delete"]);
  await press(menuItem("Enable"));
  expect(lastSent("set_scheduled_task_enabled")).toEqual({ type: "set_scheduled_task_enabled", task: "t-off", enabled: true });
});

it("leaves the end of an enabled task's line empty with no next run, marks an Off task by words, and hides the line from assistive technology", async () => {
  fakeTimers({ now: EVENING });
  held = [
    { ...REPORT, schedule: { kind: "manual" } },
    taskOf("t-ran", { launch: { title: "Reminder", compatibility: {} }, schedule: { kind: "once", run_at: new Date(2020, 0, 1, 10, 0).getTime() } }),
    taskOf("t-off", { launch: { title: "Dormant", compatibility: {} }, enabled: false }),
    taskOf("t-next", { launch: { title: "Upcoming", compatibility: {} }, next_run_at: new Date(2026, 10, 20, 9, 0).getTime() }),
  ];
  const { container } = await mount();
  const [manual, ran, dormant, upcoming] = rows(container);
  expect(meta(manual)).toBe("Manual · Main");
  for (const row of [manual, ran]) {
    expect(when(row)).toBeUndefined();
    const description = withoutIsolates(row.querySelector(".sr-only")!.textContent)!;
    expect(description).not.toContain("Next run");
    expect(description).not.toContain("Off");
  }
  expect(when(dormant)).toBe("Off");
  expect(when(upcoming)).toBe("in 10 hr");
  const line = (row: HTMLElement) => row.querySelector("[data-task-meta]")!.parentElement!;
  // No row's line is faded: its muted name and the word Off mark a task that is off.
  for (const row of [manual, ran, dormant, upcoming]) expect(line(row).className).not.toContain("disabled-opacity");
  for (const row of [manual, ran, dormant, upcoming]) {
    // The line is for the eye; the row is read by its description, which is not hidden.
    expect(row.querySelector("[data-task-meta]")!.getAttribute("aria-hidden")).toBe("true");
    expect(row.querySelector("[data-task-when]")?.getAttribute("aria-hidden") ?? "true").toBe("true");
    expect(row.querySelector(".sr-only")!.closest("[aria-hidden=true]")).toBeNull();
    // No leading icon: the only icons in a row belong to its menu's button.
    expect([...row.querySelectorAll("svg")].every((icon) => icon.closest("button") !== null)).toBe(true);
  }
});

it.each([
  ["a console task", undefined, "Main", "Console session in Main"],
  ["a project task", site.id, "Website", "Project session in Website"],
])("names %s's target in the header by %s to the eye and by the full sentence to assistive technology", async (_, project, shortName, sentence) => {
  held = [{ ...REPORT, project_id: project ?? null }];
  const { container } = await mount();
  const line = page(container)!.querySelector("header p")!;
  const shown = [...line.querySelectorAll("span")].find((one) => one.textContent === shortName)!;
  expect(shown.getAttribute("aria-hidden")).toBe("true");
  const spoken = line.querySelector(".sr-only")!;
  expect(withoutIsolates(spoken.textContent)).toBe(sentence);
  expect(spoken.closest("[aria-hidden=true]")).toBeNull();
});

it("says Off in the header's Next run for a task that is off, though it has a time stored", async () => {
  fakeTimers({ now: EVENING });
  held = [{ ...REPORT, enabled: false, next_run_at: new Date(2026, 10, 20, 9, 0).getTime() }];
  const { container } = await mount();
  expect(figureRows(container)[0]).toEqual(["Next run", "Off"]);
});

it("marks the last run in the header by what came of it, and none while the runs say nothing", async () => {
  fakeTimers({ now: EVENING });
  const last = new Date(2026, 10, 19, 9, 0).getTime();
  const started = t("scheduledTasks.header.lastRun.started");
  const failed = t("scheduledTasks.runs.failed");
  held = [{ ...REPORT, last_run_at: last }];

  heldRuns = { [REPORT.id]: [runOf("r-1", { kind: "started", session: fixSession.id, started_independent: false })] };
  const ok = await mount();
  expect(named(page(ok.container)!, started)).toBeDefined();
  expect(named(page(ok.container)!, failed)).toBeUndefined();

  heldRuns = { [REPORT.id]: [runOf("r-2", { kind: "failed", code: "some_future_failure", params: {}, message: "no" }), runOf("r-1", { kind: "started", session: fixSession.id, started_independent: false })] };
  const bad = await mount();
  expect(named(page(bad.container)!, failed)).toBeDefined();
  expect(named(page(bad.container)!, started)).toBeUndefined();

  heldRuns = {};
  const none = await mount();
  expect(named(page(none.container)!, started)).toBeUndefined();
  expect(named(page(none.container)!, failed)).toBeUndefined();
  expect(figureRows(none.container)[1]![0]).toBe("Last run");
});

it("leads each run in the history with a mark of what came of it", async () => {
  heldRuns = {
    [REPORT.id]: [
      runOf("r-2", { kind: "failed", code: "some_future_failure", params: {}, message: "no" }),
      runOf("r-1", { kind: "started", session: fixSession.id, started_independent: false }),
    ],
  };
  const { container } = await mount();
  await showRuns(container);
  const [failed, started] = runEntries(container);
  expect(failed.querySelector("svg")?.classList.contains("lucide-circle-alert")).toBe(true);
  expect(started.querySelector("svg")?.classList.contains("lucide-circle-check")).toBe(true);
  expect(failed.querySelector("svg")?.closest("[aria-hidden=true]")).not.toBeNull();
});

it.each(["en", "zh-CN"] as const)("gives the prompt an example placeholder behind the catalog's own \"Ex.\" prefix in %s", async (language) => {
  setLanguage(language);
  const { container } = await mount();
  const words = translatorFor(language);
  const expected = words("common.example", { example: words("scheduledTasks.form.prompt.example") });
  expect(prompt(container).placeholder).toBe(expected);
  expect(expected).not.toBe(words("scheduledTasks.form.prompt.example"));
});

it.each<[string, string[]]>([
  ["Once", ["Date and time"]],
  ["Every hour", ["Minute past the hour"]],
  ["Every day", ["Time"]],
  ["Weekdays", ["Time"]],
  ["Every week", ["Time", "Days"]],
  ["Every month", ["Time", "Day of month"]],
  ["Manual only", []],
])("offers %s these fields beside its kind and no others", async (kind, offered) => {
  const { container } = await mount();
  await choose("Runs", kind);
  const everyField = ["Date and time", "Minute past the hour", "Time", "Days", "Day of month"];
  const shown = (name: string) => name === "Days"
    ? ["Monday", "Sunday"].every((day) => [...page(container)!.querySelectorAll<HTMLInputElement>("input[type=checkbox]")].some((input) => input.closest("label")?.textContent?.trim() === day))
    : [...page(container)!.querySelectorAll("[data-slot=label]")].some((label) => label.textContent === name);
  expect(everyField.filter(shown)).toEqual(everyField.filter((name) => offered.includes(name)));
});

it("takes an hourly schedule's minute from 0 to 59, clamping what is typed, and saves it", async () => {
  const { container } = await mount();
  await choose("Runs", "Every hour");
  expect(minuteField(container).value).toBe("0");
  expect((named(page(container)!, "Decrease minute") as HTMLButtonElement).disabled).toBe(true);
  act(() => minuteField(container).focus());
  type(minuteField(container), "75");
  await act(async () => minuteField(container).blur());
  expect(minuteField(container).value).toBe("59");
  expect((named(page(container)!, "Increase minute") as HTMLButtonElement).disabled).toBe(true);
  await press(buttonNamed(page(container)!, "Save"));
  expect(lastSent("update_scheduled_task")).toEqual({ type: "update_scheduled_task", task: REPORT.id, schedule: { kind: "hourly", minute: 59 } });
});

it("shows the last-day note for days 29 to 31 of a monthly schedule only", async () => {
  fakeTimers({ now: EVENING });
  held = [{ ...REPORT, schedule: { kind: "monthly", day: 28, time: { hour: 9, minute: 0 } } }];
  const { container } = await mount();
  const note = "A month without this day runs on its last day.";
  expect(page(container)!.textContent).not.toContain(note);
  await press(named(page(container)!, "Increase day"));
  expect(minuteField(container).value).toBe("29");
  expect(page(container)!.textContent).toContain(note);
  await press(named(page(container)!, "Decrease day"));
  await press(named(page(container)!, "Decrease day"));
  expect(minuteField(container).value).toBe("27");
  expect(page(container)!.textContent).not.toContain(note);
});

it("saves a weekly schedule's chosen days in week order", async () => {
  held = [{ ...REPORT, schedule: { kind: "weekly", time: { hour: 9, minute: 0 }, days: ["monday"] } }];
  const { container } = await mount();
  const day = (name: string) => [...page(container)!.querySelectorAll<HTMLInputElement>("input[type=checkbox]")].find((input) => input.closest("label")?.textContent?.trim() === name);
  await press(day("Friday"));
  await press(day("Wednesday"));
  await press(buttonNamed(page(container)!, "Save"));
  expect(lastSent("update_scheduled_task")).toEqual({ type: "update_scheduled_task", task: REPORT.id, schedule: { kind: "weekly", time: { hour: 9, minute: 0 }, days: ["monday", "wednesday", "friday"] } });
});

/** The Run history view's panel, once its tab is chosen. */
const runsCard = (container: ParentNode) => page(container)!.querySelector<HTMLElement>("[role=tabpanel][id$=runs]:not([hidden])");
const runEntries = (container: ParentNode) => [...(runsCard(container)?.querySelectorAll<HTMLElement>("li") ?? [])];
const tabNamed = (container: ParentNode, name: string) => [...page(container)!.querySelectorAll<HTMLElement>("[role=tab]")].find((tab) => tab.textContent === name);
const showRuns = (container: ParentNode) => press(tabNamed(container, "Run history"));

const nameField = (container: ParentNode) => page(container)!.querySelector<HTMLInputElement>("[data-task-name] input")!;
const saveButton = (container: ParentNode) => buttonNamed(page(container)!, "Save");
const AGENT_TASK = taskOf("t-agent", { launch: { title: "Morning report", agent: "claude", account: null, compatibility: {} } });

it("follows the agent another client gives a task while the user has changed no launch setting, leaving the form clean", async () => {
  held = [AGENT_TASK];
  const { container, daemon } = await mount();
  const before = withoutIsolates(pickerNamed("Agent").textContent);
  expect(before).not.toContain("Codex");
  await act(async () => applyDaemonEvent(daemon.store, { type: "scheduled_task_upserted", task: { ...AGENT_TASK, launch: { ...AGENT_TASK.launch, agent: "codex" } } }));
  expect(withoutIsolates(pickerNamed("Agent").textContent)).toContain("Codex");
  expect(isDisabled(saveButton(container))).toBe(true);
  // A save of something else sends nothing of the launch, which the user did not change.
  type(prompt(container), "Another prompt");
  await press(saveButton(container));
  expect(lastSent("update_scheduled_task")).toEqual({ type: "update_scheduled_task", task: AGENT_TASK.id, prompt: "Another prompt" });
});

it("keeps the agent the user picked and the fields they edited when the task changes elsewhere, and follows the fields they did not", async () => {
  held = [AGENT_TASK];
  const { container, daemon } = await mount();
  await choose("Agent", "Codex");
  type(prompt(container), "My prompt");
  await act(async () => applyDaemonEvent(daemon.store, { type: "scheduled_task_upserted", task: { ...AGENT_TASK, prompt: "Their prompt", launch: { ...AGENT_TASK.launch, title: "Renamed elsewhere" } } }));
  expect(withoutIsolates(pickerNamed("Agent").textContent)).toContain("Codex");
  expect(prompt(container).value).toBe("My prompt");
  expect(nameField(container).value).toBe("Renamed elsewhere");
  expect(isDisabled(saveButton(container))).toBe(false);
});

it("sends project_id null for None (console session) and the console only when it changed", async () => {
  held = [{ ...REPORT, project_id: site.id }];
  const { container } = await mount();
  await choose("Project", "None (console session)");
  await press(saveButton(container));
  expect(lastSent("update_scheduled_task")).toEqual({ type: "update_scheduled_task", task: REPORT.id, project_id: null });
  await choose("Console", "Spare");
  await press(saveButton(container));
  expect(lastSent("update_scheduled_task")).toEqual({ type: "update_scheduled_task", task: REPORT.id, console_id: spare.id, project_id: null });
});

const WORK = accountOf("work", "codex", "Work", "/tmp/codex-work");
const ACCOUNT_TASK = taskOf("t-account", { launch: { title: "Morning report", agent: "codex", account: "work", compatibility: {} } });
const reading = (extra: Partial<AccountQuota>): AccountQuota => ({ agent: "codex", account: "work", fetched_at: 1, plan: null, email: null, windows: [], unavailable: null, ...extra });

it("blocks a save that names a turned-off account with the account field's message, and sends nothing", async () => {
  held = [ACCOUNT_TASK];
  heldAccounts = [{ ...WORK, disabled: true }];
  const { container } = await mount();
  // Untouched, the form saves nothing of the launch, so the account is not named.
  expect(page(container)!.textContent).not.toContain("is disabled");
  type(nameField(container), "Renamed");
  await act(async () => {});
  expect(withoutIsolates(page(container)!.textContent)).toContain("Account Work is disabled, so a session cannot use it.");
  await press(saveButton(container));
  expect(lastSent("update_scheduled_task")).toBeUndefined();
});

it.each([
  ["signed out", reading({ unavailable: { code: "quota_not_signed_in", params: {}, message: "Not signed in" } })],
  ["used up", reading({ windows: [{ kind: "weekly", label: null, used_percent: 100, resets_at: Date.now() + 3_600_000, window_minutes: null }] })],
])("saves a task that names an account that is %s, which is left to the run", async (_, quota) => {
  held = [ACCOUNT_TASK];
  heldAccounts = [WORK];
  heldQuotas = [quota];
  const { container } = await mount();
  type(nameField(container), "Renamed");
  await press(saveButton(container));
  expect(page(container)!.textContent).not.toContain("so a session cannot use it");
  expect(lastSent("update_scheduled_task")).toMatchObject({ task: ACCOUNT_TASK.id, launch: { title: "Renamed", agent: "codex", account: "work" } });
});

it("never blocks a task that leaves the account to its target, whatever accounts are turned off", async () => {
  heldAccounts = [{ ...WORK, disabled: true }];
  const { container } = await mount();
  type(nameField(container), "Renamed");
  await press(saveButton(container));
  expect(page(container)!.textContent).not.toContain("so a session cannot use it");
  const update = lastSent("update_scheduled_task") as Extract<ClientRequestBody, { type: "update_scheduled_task" }>;
  expect(update.launch).toMatchObject({ title: "Renamed" });
  expect(update.launch).not.toHaveProperty("account");
});

it("shows a console session to report to that no longer exists as none, leaves the form clean, and does not send it with a change of the name", async () => {
  held = [{ ...REPORT, project_id: site.id, launch: { title: "Morning report", bound_to: "s-gone", compatibility: {} } }];
  const { container } = await mount();
  expect(withoutIsolates(pickerNamed("Report to").textContent)).toContain("No console session");
  expect(isDisabled(saveButton(container))).toBe(true);
  type(nameField(container), "Renamed");
  await press(saveButton(container));
  const update = lastSent("update_scheduled_task") as Extract<ClientRequestBody, { type: "update_scheduled_task" }>;
  expect(update.launch).toMatchObject({ title: "Renamed" });
  expect(update.launch).not.toHaveProperty("bound_to");
});

const FUTURE = taskOf("t-future", { launch: { title: "From the future", compatibility: {} }, schedule: { kind: "yearly", month: 3 } as unknown as ScheduledTask["schedule"] });
const KIND_LABELS = ["Once", "Every hour", "Every day", "Weekdays", "Every week", "Every month", "Manual only"];

it("shows a task of a schedule kind this client does not know by its target alone, with an empty Runs select and no further field", async () => {
  held = [FUTURE];
  const { container } = await mount();
  const [row] = rows(container);
  expect(meta(row)).toBe("Main");
  expect(withoutIsolates(row.querySelector(".sr-only")!.textContent)).toBe("Console session in Main");
  expect(withoutIsolates(page(container)!.querySelector("header p")!.textContent)).toBe("MainConsole session in Main");
  expect(row.textContent).not.toContain("{");
  expect(page(container)!.textContent).not.toContain("{");
  const picker = withoutIsolates(pickerNamed("Runs").textContent)!;
  for (const label of KIND_LABELS) expect(picker).not.toContain(label);
  const labels = [...page(container)!.querySelectorAll("[data-slot=label]")].map((label) => label.textContent);
  for (const field of ["Date and time", "Minute past the hour", "Time", "Days", "Day of month"]) expect(labels).not.toContain(field);
  expect(isDisabled(saveButton(container))).toBe(true);
});

it("saves a change of the prompt of a task of a schedule kind it does not know without a schedule, and a kind picked with one", async () => {
  held = [FUTURE];
  const { container } = await mount();
  type(prompt(container), "Another prompt");
  await press(saveButton(container));
  expect(lastSent("update_scheduled_task")).toEqual({ type: "update_scheduled_task", task: FUTURE.id, prompt: "Another prompt" });
  await choose("Runs", "Every day");
  await press(saveButton(container));
  expect(lastSent("update_scheduled_task")).toEqual({ type: "update_scheduled_task", task: FUTURE.id, schedule: { kind: "daily", time: { hour: 9, minute: 0 } } });
});

it("shows a failed listing of the tasks in the content area and as a row of the sidebar, each with its own visible Try again, raising no alert, and lists again on either", async () => {
  listFailing = true;
  const { container } = await mount();
  expect(errorAlerts.getSnapshot()).toEqual([]);
  expect(rows(container)).toEqual([]);
  const title = t("scheduledTasks.listFailedTitle");
  expect(sidebar(container).textContent).toContain(title);
  expect(page(container)!.textContent).toContain(title);
  const sidebarRetry = buttonNamed(sidebar(container), "Try again");
  expect(sidebarRetry).toBeDefined();
  expect(sidebarRetry!.closest("[hidden], [aria-hidden=true]")).toBeNull();
  expect(buttonNamed(page(container)!, "Try again")).toBeDefined();
  listFailing = false;
  sent.mockClear();
  await press(sidebarRetry);
  expect(lastSent("list_scheduled_tasks")).toBeDefined();
  expect(rows(container)).toHaveLength(1);
  expect(sidebar(container).textContent).not.toContain(title);
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("lists the tasks again from the content area's Try again", async () => {
  listFailing = true;
  const { container } = await mount();
  listFailing = false;
  await press(buttonNamed(page(container)!, "Try again"));
  expect(rows(container)).toHaveLength(1);
  expect(heading(container)).toBe("Morning report");
});

it("shows a failed listing of a task's runs inside Run history with Try again, raising no alert, and lists them again", async () => {
  runsFailing = true;
  const { container } = await mount();
  await showRuns(container);
  expect(runsCard(container)!.textContent).toContain(t("scheduledTasks.runs.listFailedTitle"));
  expect(errorAlerts.getSnapshot()).toEqual([]);
  runsFailing = false;
  await press(buttonNamed(runsCard(container)!, "Try again"));
  expect(runsCard(container)!.textContent).toContain("No runs yet");
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it.each([
  ["a code it words", { kind: "failed", code: "account_disabled", params: { agent: "Codex", account: "Work" }, message: "raw daemon text" }, "is disabled, so a session cannot be opened on it."],
  ["a code it does not", { kind: "failed", code: "some_future_failure", params: {}, message: "spawn failed: no such file" }, "spawn failed: no such file"],
] as const)("opens a failed run's reason in the popover of its Details button, for %s", async (_, outcome, reason) => {
  heldRuns = { [REPORT.id]: [runOf("r-1", { ...outcome, params: { ...outcome.params } })] };
  const { container } = await mount();
  await showRuns(container);
  const [failed] = runEntries(container);
  expect(detailsButton(failed)).toBeDefined();
  expect(withoutIsolates(failed.textContent)).not.toContain(reason);
  const popover = openDetails(failed);
  expect(failed.contains(popover)).toBe(false);
  expect(withoutIsolates(popover.textContent)).toContain(reason);
});

it("offers New console where there is no console, and makes the sidebar's New scheduled task unavailable with its reason", async () => {
  noConsoles = true;
  held = [];
  const { container } = await mount();
  expect(page(container)!.textContent).toContain(t("scheduledTasks.noConsoles.description"));
  await press(buttonNamed(page(container)!, "New console"));
  expect(newConsole).toHaveBeenCalledTimes(1);
  const add = named(sidebar(container), "New scheduled task")!;
  expect(add.getAttribute("aria-disabled")).toBe("true");
  expect(document.getElementById(add.getAttribute("aria-describedby")!)?.textContent).toBe(t("scheduledTasks.noConsoles"));
  await press(add);
  expect(page(container)!.querySelector("form")).toBeNull();
});

it("does nothing for another task chosen while a new task's save is in flight, with Cancel unavailable then", async () => {
  held = [REPORT, taskOf("t-other", { launch: { title: "Other", compatibility: {} } })];
  let answerCreate!: () => void;
  createAnswer = () => new Promise<void>((resolve) => (answerCreate = resolve));
  const { container } = await mount();
  await press(named(sidebar(container), "New scheduled task"));
  type(prompt(container), "Tidy the backlog");
  await press(saveButton(container));
  expect(isDisabled(buttonNamed(page(container)!, "Cancel"))).toBe(true);
  await press(rows(container)[1]);
  expect(asking()).toBeNull();
  expect(heading(container)).toBe("New scheduled task");
  await act(async () => answerCreate());
  expect(heading(container)).toBe("Tidy the backlog");
});

it("hands focus to the Name field when Save or Revert disables itself under it, and to the page when Cancel ends a new task's form", async () => {
  const { container } = await mount();
  for (const name of ["Save", "Revert"]) {
    type(prompt(container), `Edited before ${name}`);
    act(() => void document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true })));
    const button = buttonNamed(page(container)!, name)!;
    act(() => button.focus());
    expect(document.activeElement).toBe(button);
    await press(button);
    await act(async () => {});
    expect(document.activeElement, name).toBe(nameField(container));
    // A move the user did not make: the ring follows the last input, a key here.
    expect(document.activeElement!.hasAttribute("data-focus-visible"), name).toBe(true);
  }
  await press(named(sidebar(container), "New scheduled task"));
  const cancel = buttonNamed(page(container)!, "Cancel")!;
  act(() => void document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true })));
  act(() => cancel.focus());
  await press(cancel);
  await act(async () => {});
  expect(document.activeElement).not.toBe(document.body);
  expect(page(container)!.contains(document.activeElement)).toBe(true);
  expect(document.activeElement!.hasAttribute("data-focus-visible")).toBe(true);
});

it("counts a time of day with a segment cleared as empty when Enter is pressed in a segment, so a save cannot read the time the field no longer shows", async () => {
  const { container } = await mount();
  clearSegment(segmentOf(page(container)!, "minute"));
  await act(async () => void segmentOf(page(container)!, "minute").dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true })));
  expect(segmentOf(page(container)!, "hour").matches(partSelector.blankSegment)).toBe(true);
  await press(saveButton(container));
  expect(lastSent("update_scheduled_task")).toBeUndefined();
  expect(page(container)!.textContent).toContain("Enter a time.");
});

it.each([
  ["update_scheduled_task", async (container: ParentNode) => type(prompt(container), "Changed prompt")],
  ["set_scheduled_task_enabled", async (container: ParentNode) => press(named(page(container)!, "Enabled"))],
] as const)("raises the error alert for a %s the daemon refuses, and keeps the draft to save again", async (kind, change) => {
  saveRefused = true;
  const { container } = await mount();
  await change(container);
  await press(saveButton(container));
  expect(lastSent(kind)).toBeDefined();
  expect(errorAlerts.getSnapshot().map((alert) => alert.title)).toEqual([t("scheduledTasks.form.saveFailedTitle")]);
  expect(isDisabled(saveButton(container))).toBe(false);
  if (kind === "update_scheduled_task") expect(prompt(container).value).toBe("Changed prompt");
  else expect((named(page(container)!, "Enabled") as HTMLInputElement).checked).toBe(false);
  acknowledgeAllAlerts();
  saveRefused = false;
  await press(saveButton(container));
  expect(isDisabled(saveButton(container))).toBe(true);
});

it("follows a change made elsewhere to a field the user edited back to its saved value, leaving the form clean", async () => {
  const { container, daemon } = await mount();
  const enabled = () => named(page(container)!, "Enabled") as HTMLInputElement;
  await press(enabled());
  await press(enabled());
  expect(isDisabled(saveButton(container))).toBe(true);
  await act(async () => applyDaemonEvent(daemon.store, { type: "scheduled_task_upserted", task: { ...REPORT, enabled: false } }));
  expect(enabled().checked).toBe(false);
  expect(isDisabled(saveButton(container))).toBe(true);
});

it("follows the launch another client changes when the user built it back to what the task held", async () => {
  held = [AGENT_TASK];
  const { container, daemon } = await mount();
  await choose("Agent", "Codex");
  await choose("Agent", "Claude");
  expect(isDisabled(saveButton(container))).toBe(true);
  await act(async () => applyDaemonEvent(daemon.store, { type: "scheduled_task_upserted", task: { ...AGENT_TASK, launch: { ...AGENT_TASK.launch, agent: "codex" } } }));
  expect(withoutIsolates(pickerNamed("Agent").textContent)).toContain("Codex");
  expect(isDisabled(saveButton(container))).toBe(true);
});

it("opens clean a one-off whose time is not on a whole minute, and one whose console or project the window does not hold yet", async () => {
  fakeTimers({ now: EVENING });
  held = [
    taskOf("t-once", { launch: { title: "Seconds", compatibility: {} }, schedule: { kind: "once", run_at: new Date(2026, 10, 25, 9, 0, 30).getTime() } }),
    taskOf("t-new", { console_id: "c-new", project_id: "p-new", launch: { title: "Elsewhere", compatibility: {} } }),
  ];
  const { container } = await mount();
  expect(isDisabled(saveButton(container))).toBe(true);
  await press(rows(container)[1]);
  expect(heading(container)).toBe("Elsewhere");
  expect(isDisabled(saveButton(container))).toBe(true);
  type(nameField(container), "Renamed");
  await press(saveButton(container));
  // Only what was changed is sent: the target the window cannot show is not moved.
  const update = lastSent("update_scheduled_task") as Extract<ClientRequestBody, { type: "update_scheduled_task" }>;
  expect(update.console_id).toBeUndefined();
  expect(update.project_id).toBeUndefined();
});

it("shows both pickers empty for a task whose account can no longer be picked, and saves it as it is until one is picked", async () => {
  held = [taskOf("t-gone", { launch: { title: "Gone", agent: "claude", account: "a-deleted", compatibility: {} } })];
  const { container } = await mount();
  expect(withoutIsolates(pickerNamed("Agent").textContent)).toContain("Choose an agent");
  expect(page(container)!.textContent).toContain("Choose an account");
  expect(page(container)!.textContent).toContain(t("scheduledTasks.form.agent.gone"));
  expect(isDisabled(saveButton(container))).toBe(true);
  type(prompt(container), "Another prompt");
  await press(saveButton(container));
  expect(lastSent("update_scheduled_task")).toEqual({ type: "update_scheduled_task", task: "t-gone", prompt: "Another prompt" });
  await choose("Agent", "Codex");
  expect(withoutIsolates(pickerNamed("Agent").textContent)).toContain("Codex");
  expect(isDisabled(saveButton(container))).toBe(false);
});

it("keeps a task that leaves its account to its target leaving it when another launch setting is changed", async () => {
  held = [taskOf("t-target", { launch: { title: "Target's account", agent: "claude", compatibility: {} } })];
  const { container } = await mount();
  await press(buttonNamed(page(container)!, "Advanced"));
  await choose("Model", "Sonnet");
  expect(isDisabled(saveButton(container))).toBe(false);
  await press(saveButton(container));
  const launch = (lastSent("update_scheduled_task") as Extract<ClientRequestBody, { type: "update_scheduled_task" }>).launch!;
  expect(launch.agent).toBe("claude");
  expect(launch).not.toHaveProperty("account");
  expect(launch.model).toBe("sonnet");
});

it.each([
  ["archived", oldHub],
  ["still starting", newHub],
])("shows a saved console session to report to that is %s as that session, clean, and saves it with the launch", async (_, owner) => {
  held = [taskOf("t-owner", { project_id: site.id, launch: { title: "Owner", bound_to: owner.id, compatibility: {} } })];
  const { container } = await mount();
  expect(withoutIsolates(pickerNamed("Report to").textContent)).toContain(owner.title);
  // An archived session cannot take a run, which then starts without reporting: the option says so before Run history does.
  expect(withoutIsolates(pickerNamed("Report to").textContent)?.includes("Archived")).toBe(owner.status === "archived");
  expect(isDisabled(saveButton(container))).toBe(true);
  type(nameField(container), "Renamed");
  await press(saveButton(container));
  expect(lastSent("update_scheduled_task")).toMatchObject({ launch: { title: "Renamed", bound_to: owner.id } });
});

it("shows a session to report to that is not a console session of the task's console as none, and does not save it", async () => {
  held = [taskOf("t-other-owner", { project_id: site.id, launch: { title: "Owner", bound_to: fixSession.id, compatibility: {} } })];
  const { container } = await mount();
  expect(withoutIsolates(pickerNamed("Report to").textContent)).toContain("No console session");
  expect(isDisabled(saveButton(container))).toBe(true);
  type(nameField(container), "Renamed");
  await press(saveButton(container));
  expect((lastSent("update_scheduled_task") as Extract<ClientRequestBody, { type: "update_scheduled_task" }>).launch).not.toHaveProperty("bound_to");
});

it("hands focus from the pressed Try again of Run history, once the runs are listed, to the Run history tab", async () => {
  runsFailing = true;
  const { container } = await mount();
  await showRuns(container);
  runsFailing = false;
  const retry = buttonNamed(runsCard(container)!, "Try again")!;
  act(() => void document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true })));
  act(() => retry.focus());
  await press(retry);
  await act(async () => {});
  expect(runsCard(container)!.textContent).toContain("No runs yet");
  const tab = page(container)!.querySelector<HTMLElement>("[role=tab][aria-selected=true]")!;
  expect(tab.textContent).toBe("Run history");
  expect(document.activeElement).toBe(tab);
  // The ring follows the last input, a key here.
  expect(tab.hasAttribute("data-focus-visible")).toBe(true);
});

it("keeps focus in the sidebar, on its first task, when the Try again of its failed listing succeeds", async () => {
  listFailing = true;
  const { container } = await mount();
  listFailing = false;
  const retry = buttonNamed(sidebar(container), "Try again")!;
  act(() => void document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true })));
  act(() => retry.focus());
  await press(retry);
  await act(async () => {});
  expect(document.activeElement).toBe(rows(container)[0]);
  expect(document.activeElement!.hasAttribute("data-focus-visible")).toBe(true);
});

it("words a task of a schedule kind it does not know in the row's description, with Off and the next run", async () => {
  fakeTimers({ now: EVENING });
  const at = new Date(2026, 10, 20, 9, 0).getTime();
  held = [{ ...FUTURE, next_run_at: at }, { ...FUTURE, id: "t-off", enabled: false }];
  const { container } = await mount();
  const [on, off] = rows(container);
  expect(withoutIsolates(on.querySelector(".sr-only")!.textContent)).toBe("Console session in Main · Next run Fri, Nov 20, 2026, 9:00 AM");
  expect(withoutIsolates(off.querySelector(".sr-only")!.textContent)).toBe("Console session in Main · Off");
});

it("keeps the error mark of a field edited and left while a save is in flight once the save is done", async () => {
  let answerUpdate!: () => void;
  updateAnswer = () => new Promise<void>((resolve) => (answerUpdate = resolve));
  const { container } = await mount();
  type(nameField(container), "Renamed");
  await press(saveButton(container));
  // The prompt is emptied and left while the save is out: the save's own clearing of the marks must not drop that.
  act(() => prompt(container).focus());
  type(prompt(container), "");
  act(() => prompt(container).blur());
  await act(async () => answerUpdate());
  expect(page(container)!.textContent).toContain("Enter a prompt.");
});

it("keeps a field the user typed back to its old value while this form's save was out, and the form dirty, once the save lands", async () => {
  let answerUpdate!: () => void;
  updateAnswer = () => new Promise<void>((resolve) => (answerUpdate = resolve));
  const { container } = await mount();
  type(prompt(container), "Saved prompt");
  await press(saveButton(container));
  type(prompt(container), REPORT.prompt);
  await act(async () => answerUpdate());
  expect(prompt(container).value).toBe(REPORT.prompt);
  expect(isDisabled(saveButton(container))).toBe(false);
});

it("keeps a launch setting the user changed back to its old value while this form's save was out, and the form dirty, once the save lands", async () => {
  held = [AGENT_TASK];
  let answerUpdate!: () => void;
  updateAnswer = () => new Promise<void>((resolve) => (answerUpdate = resolve));
  const { container } = await mount();
  const before = withoutIsolates(pickerNamed("Agent").textContent);
  await choose("Agent", "Codex");
  await press(saveButton(container));
  await choose("Agent", "Claude");
  await act(async () => answerUpdate());
  expect(withoutIsolates(pickerNamed("Agent").textContent)).toBe(before);
  expect(isDisabled(saveButton(container))).toBe(false);
});

it("keeps focus in the sidebar, on the header's unavailable New scheduled task, when the Try again of its failed listing succeeds with no task and no console", async () => {
  noConsoles = true;
  held = [];
  listFailing = true;
  const { container } = await mount();
  listFailing = false;
  const retry = buttonNamed(sidebar(container), "Try again")!;
  act(() => retry.focus());
  await press(retry);
  await act(async () => {});
  expect(document.activeElement).toBe(named(sidebar(container), "New scheduled task"));
});

it("keeps the prompt typed back and the agent picked back while a save of both was out, its reply arriving before its broadcast, and the form dirty", async () => {
  held = [AGENT_TASK];
  broadcastsLate = true;
  let answerUpdate!: () => void;
  updateAnswer = () => new Promise<void>((resolve) => (answerUpdate = resolve));
  const { container, daemon } = await mount();
  const agent = () => withoutIsolates(pickerNamed("Agent").textContent);
  const before = agent();
  type(prompt(container), "Saved prompt");
  await choose("Agent", "Codex");
  await press(saveButton(container));
  type(prompt(container), AGENT_TASK.prompt);
  await choose("Agent", "Claude");
  await act(async () => answerUpdate());
  // Answered while the store does not hold the change yet: what the user did stands.
  expect(prompt(container).value).toBe(AGENT_TASK.prompt);
  expect(agent()).toBe(before);
  await deliverBroadcasts(daemon);
  expect(prompt(container).value).toBe(AGENT_TASK.prompt);
  expect(agent()).toBe(before);
  expect(isDisabled(saveButton(container))).toBe(false);
  updateAnswer = async () => {};
  await press(saveButton(container));
  expect(lastSent("update_scheduled_task")).toMatchObject({ task: AGENT_TASK.id, prompt: AGENT_TASK.prompt, launch: { agent: "claude" } });
});

it("holds a form whose save is answered but not yet in the store, showing what was saved, and lets it go once it is", async () => {
  held = [REPORT, taskOf("t-other", { launch: { title: "Other", compatibility: {} } })];
  broadcastsLate = true;
  const { container, daemon } = await mount();
  type(prompt(container), "Saved prompt");
  await press(saveButton(container));
  expect(prompt(container).value).toBe("Saved prompt");
  await press(rows(container)[1]);
  expect(asking()).not.toBeNull();
  await answer("Keep editing");
  await deliverBroadcasts(daemon);
  expect(prompt(container).value).toBe("Saved prompt");
  expect(isDisabled(saveButton(container))).toBe(true);
  await press(rows(container)[1]);
  expect(asking()).toBeNull();
  expect(heading(container)).toBe("Other");
});

it("keeps what the first request of a save stored as saved and the refused Enabled as the user's, the first's broadcast arriving after the refusal", async () => {
  broadcastsLate = true;
  enableRefused = true;
  const { container, daemon } = await mount();
  const enabled = () => named(page(container)!, "Enabled") as HTMLInputElement;
  type(prompt(container), "Saved prompt");
  await press(enabled());
  await press(saveButton(container));
  expect(lastSent("update_scheduled_task")).toEqual({ type: "update_scheduled_task", task: REPORT.id, prompt: "Saved prompt" });
  expect(lastSent("set_scheduled_task_enabled")).toEqual({ type: "set_scheduled_task_enabled", task: REPORT.id, enabled: false });
  expect(errorAlerts.getSnapshot().map((alert) => alert.title)).toEqual([t("scheduledTasks.form.saveFailedTitle")]);
  acknowledgeAllAlerts();
  expect(prompt(container).value).toBe("Saved prompt");
  expect(enabled().checked).toBe(false);
  await deliverBroadcasts(daemon);
  expect(prompt(container).value).toBe("Saved prompt");
  expect(enabled().checked).toBe(false);
  // The prompt is saved; Enabled is all that is left to save.
  enableRefused = false;
  broadcastsLate = false;
  sent.mockClear();
  await press(saveButton(container));
  expect(lastSent("update_scheduled_task")).toBeUndefined();
  expect(lastSent("set_scheduled_task_enabled")).toEqual({ type: "set_scheduled_task_enabled", task: REPORT.id, enabled: false });
  expect(isDisabled(saveButton(container))).toBe(true);
});

it("sends the prompt typed back to the task's value with a second Save pressed before the first landed, and ends on it, clean", async () => {
  broadcastsLate = true;
  const { container, daemon } = await mount();
  type(prompt(container), "Saved prompt");
  await press(saveButton(container));
  // The first save is answered; its broadcast has not arrived, so the store still holds the old prompt.
  type(prompt(container), REPORT.prompt);
  await act(async () => {});
  expect(isDisabled(saveButton(container))).toBe(false);
  sent.mockClear();
  await press(saveButton(container));
  expect(lastSent("update_scheduled_task")).toEqual({ type: "update_scheduled_task", task: REPORT.id, prompt: REPORT.prompt });
  await deliverBroadcasts(daemon);
  expect(prompt(container).value).toBe(REPORT.prompt);
  expect(isDisabled(saveButton(container))).toBe(true);
});

it("keeps the agent picked when the save of it is refused, with Save still available", async () => {
  held = [AGENT_TASK];
  saveRefused = true;
  const { container } = await mount();
  await choose("Agent", "Codex");
  await press(saveButton(container));
  expect(lastSent("update_scheduled_task")).toMatchObject({ task: AGENT_TASK.id, launch: { agent: "codex" } });
  expect(errorAlerts.getSnapshot().map((alert) => alert.title)).toEqual([t("scheduledTasks.form.saveFailedTitle")]);
  acknowledgeAllAlerts();
  expect(withoutIsolates(pickerNamed("Agent").textContent)).toContain("Codex");
  expect(isDisabled(saveButton(container))).toBe(false);
});
