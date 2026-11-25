import { SAMPLE, minutesAgo, sessionOf, snapshotState } from "../../fixtures/builders";
import { describing, type Ui } from "../../fixtures/interact";
import { lastOccurrence, scheduledRunOf, scheduledTaskOf } from "../../fixtures/scheduledTaskBuilders";
import type { Schedule, ScheduledTask, ScheduledTaskRun } from "../../protocol";
import { weekdayName } from "../../scheduledTasks/scheduleText";
import type { Scenario } from "../scenario";
import { production, staging } from "./remoteDaemons";

const GROUP = "Scheduled tasks";

const { console: console_, web, api, consoleSession, sessions } = SAMPLE;

/** A console with two projects and its console session, which a project task reports to. */
const state = snapshotState({ consoles: [console_], projects: [web, api], sessions });

/** A window with no console at all, which a task could not start a session in. */
const noConsoleState = snapshotState({ consoles: [], projects: [], sessions: [] });

const LONG_NAME = "Quarterly cross-team dependency, licence and security audit for every service in the monorepo and its mirrors";
const LONG_PROMPT = [
  "Walk every service in the monorepo, list each dependency that has a newer major version or an advisory against it, and group the findings by team.",
  "Open one pull request per team for the upgrades that need no code change, and leave a comment on the audit issue for the ones that do, naming the breaking change and a suggested owner.",
].join("\n");

/** When the first task's latest run was, which is also its last run in the header, so the two agree. */
const LATEST_DEPS_RUN = minutesAgo(50);

/** The tasks the page lists, oldest first, one of every schedule kind: the first, which the page shows, is a project
 * task that reports to the console session; one is off and one has a name and prompt too long for a row. Their next runs
 * are the daemon's own for their schedules, in `zone` (the window's own where `undefined`). */
function tasksIn(zone?: string): ScheduledTask[] {
  return [
    scheduledTaskOf("t-deps", console_.id, {
      project_id: web.id,
      launch: { title: "Weekly dependency review", agent: "claude", compatibility: {}, bound_to: consoleSession.id },
      prompt: "Review the open dependency updates and open a pull request for the safe ones.\nList the ones that need a decision.",
      schedule: { kind: "weekly", time: { hour: 9, minute: 30 }, days: ["monday", "wednesday", "friday"] },
      last_run_at: LATEST_DEPS_RUN,
    }, zone),
    scheduledTaskOf("t-report", console_.id, {
      launch: { title: "Morning report", compatibility: {} },
      prompt: "Summarise yesterday's commits across every project and post the summary as a page.",
      schedule: { kind: "daily", time: { hour: 8, minute: 0 } },
      last_run_at: minutesAgo(60 * 9),
    }, zone),
    scheduledTaskOf("t-index", console_.id, {
      project_id: api.id,
      prompt: "Check the search index health and report anything that looks stale.",
      schedule: { kind: "hourly", minute: 15 },
      last_run_at: minutesAgo(40),
    }, zone),
    scheduledTaskOf("t-wrap", console_.id, {
      launch: { title: "Wrap-up notes", compatibility: {} },
      prompt: "Write today's wrap-up notes from the sessions that ran.",
      schedule: { kind: "weekdays", time: { hour: 17, minute: 30 } },
      last_run_at: minutesAgo(60 * 22),
    }, zone),
    scheduledTaskOf("t-ledger", console_.id, {
      project_id: api.id,
      launch: { title: "Month-end ledger close", compatibility: {} },
      prompt: "Reconcile the month's invoices against the ledger and list the ones that do not match.",
      schedule: { kind: "monthly", day: 31, time: { hour: 9, minute: 0 } },
      last_run_at: minutesAgo(60 * 24 * 12),
    }, zone),
    scheduledTaskOf("t-release", console_.id, {
      project_id: web.id,
      launch: { title: "Release announcement draft", compatibility: {} },
      prompt: "Draft the announcement for the release that goes out tomorrow.",
      // To the minute, as the form holds it: a task with seconds in its time would show the form as already changed.
      schedule: { kind: "once", run_at: Math.ceil((Date.now() + 30 * 3_600_000) / 60_000) * 60_000 },
    }, zone),
    scheduledTaskOf("t-branches", console_.id, {
      launch: { title: "Clean up stale branches", compatibility: {} },
      prompt: "List the branches that were merged more than a month ago.",
      schedule: { kind: "manual" },
    }, zone),
    scheduledTaskOf("t-backup", console_.id, {
      launch: { title: "Nightly backup check", compatibility: {} },
      prompt: "Confirm last night's backup finished and report its size.",
      schedule: { kind: "daily", time: { hour: 6, minute: 0 } },
      enabled: false,
      last_run_at: minutesAgo(60 * 80),
    }, zone),
    scheduledTaskOf("t-long", console_.id, {
      project_id: api.id,
      launch: { title: LONG_NAME, compatibility: {} },
      prompt: LONG_PROMPT,
      schedule: { kind: "weekly", time: { hour: 10, minute: 0 }, days: ["tuesday", "thursday"] },
    }, zone),
  ];
}

const TASKS = tasksIn();

/** A task whose schedule is of a kind a newer daemon sends that this client has no words for. It is the only task of its
 * scenario, since it is a state of the task's own, not of the list. The tag is outside the contract the types hold, so it
 * is written through `unknown`; a newer daemon would send a next run for it. */
const NEWER_KIND_TASK = scheduledTaskOf("t-newer", console_.id, {
  project_id: web.id,
  launch: { title: "Quarterly licence review", compatibility: {} },
  prompt: "Review the licences of every dependency added this quarter.",
  schedule: { kind: "quarterly", month: 1, day: 5, time: { hour: 9, minute: 0 } } as unknown as Schedule,
  next_run_at: Date.now() + 40 * 24 * 3_600_000,
  last_run_at: minutesAgo(60 * 24 * 80),
});

/** What the daemon refuses a listing with, as a database that cannot be read is told. */
const LISTING_FAILURE = { code: "internal_error", params: { detail: "database is locked" }, message: "database is locked" };

/** What the first task's runs hold: a session it started, one it started independent because the console session it was
 * to report to was gone, one whose session is gone since, an account that was not signed in, and a launch that failed unforeseen. */
const RUNS: ScheduledTaskRun[] = [
  scheduledRunOf("r-started", "t-deps", 50, { kind: "started", session: "s-web-1", started_independent: false }, { trigger: "scheduled", ran_at: LATEST_DEPS_RUN, due_at: LATEST_DEPS_RUN }),
  scheduledRunOf("r-independent", "t-deps", 60 * 28, { kind: "started", session: "s-web-2", started_independent: true }, { trigger: "catch_up", due_at: minutesAgo(60 * 30) }),
  scheduledRunOf("r-gone", "t-deps", 60 * 52, { kind: "started", session: "s-removed", started_independent: false }, { trigger: "run_now" }),
  scheduledRunOf(
    "r-signed-out",
    "t-deps",
    60 * 76,
    {
      kind: "failed",
      code: "account_not_signed_in",
      params: { account: "Work", agent: "Claude Code" },
      message: "Work (Claude Code) is not logged in, so a session cannot be opened on it.",
    },
    { trigger: "scheduled", due_at: minutesAgo(60 * 76) },
  ),
  scheduledRunOf(
    "r-unforeseen",
    "t-deps",
    60 * 100,
    {
      kind: "failed",
      code: "internal_error",
      params: { detail: "failed to start the terminal: no free pseudo-terminal could be opened (os error 24)" },
      message: "failed to start the terminal: no free pseudo-terminal could be opened (os error 24)",
    },
    { trigger: "scheduled", due_at: minutesAgo(60 * 100) },
  ),
];

/** A launch Grok Build's sandbox refused, as the daemon records it: the code is the provider's own, with the file that sets
 * the sandbox and its profile, and the run stands in a Grok task's history (the one a launch of that agent could refuse). */
const SANDBOX_RUN = scheduledRunOf(
  "r-sandbox",
  "t-deps",
  60 * 76,
  {
    kind: "failed",
    code: "provider.grok.sandbox_unsupported",
    params: { source: "/Users/dev/Accounts/grok-work/.grok/config.toml", profile: "strict" },
    message:
      "Octoboard's per-session Grok home is incompatible with Grok's sandbox (profile `strict`, set by /Users/dev/Accounts/grok-work/.grok/config.toml): to launch this session through Octoboard, turn the sandbox off where it is set, or ask whoever manages that setting to. A conversation started under the sandbox keeps it, so continue that one in Grok itself",
  },
  { trigger: "scheduled", due_at: minutesAgo(60 * 76) },
);

/** `TASKS` with the first, whose runs the page shows, on Grok Build. */
const GROK_TASKS: ScheduledTask[] = TASKS.map((task) => (task.id === "t-deps" ? { ...task, launch: { ...task.launch, agent: "grok" } } : task));
/** The plain task's schedule is Monday, Wednesday and Friday at 9:30: when it last came due and the time before, so its
 * runs agree with it. */
const onPlainDay = (weekday: number) => weekday === 1 || weekday === 3 || weekday === 5;
const LATEST_PLAIN_RUN = lastOccurrence(Date.now(), 9, 30, onPlainDay);
const EARLIER_PLAIN_RUN = lastOccurrence(Date.now(), 9, 30, onPlainDay, 1);
/** A Run now on the day of the earlier occurrence, in the afternoon, so it falls between the two scheduled runs. */
const MANUAL_PLAIN_RUN = EARLIER_PLAIN_RUN + 5 * 3_600_000;

/** The window the plain scenarios open on: the same console and projects, with the two sessions the plain task's runs
 * started (idle). */
const plainSessions = [
  consoleSession,
  sessionOf("s-deps-1", console_.id, web.id, "Dependency review", "idle", { bound_to: consoleSession.id, started_at: LATEST_PLAIN_RUN }),
  sessionOf("s-deps-2", console_.id, web.id, "Bump the Astro and Vite versions", "idle", { bound_to: consoleSession.id, started_at: MANUAL_PLAIN_RUN }),
];
const plainState = snapshotState({ consoles: [console_], projects: [web, api], sessions: plainSessions });

/** Tomorrow at `hour`:00 in the window's zone, to the minute as the form holds it. */
function tomorrowAt(hour: number): number {
  const day = new Date();
  day.setDate(day.getDate() + 1);
  day.setHours(hour, 0, 0, 0);
  return day.getTime();
}

/** Six tasks with names and second lines that fit a row whole: a project task that reports to the console session (the
 * first, which the page shows), console tasks, a weekdays, a daily, an hourly and a one-off schedule, and one that is
 * off. */
const PLAIN_TASKS: ScheduledTask[] = [
  scheduledTaskOf("t-plain-deps", console_.id, {
    project_id: web.id,
    launch: { title: "Dependency review", agent: "claude", compatibility: {}, bound_to: consoleSession.id },
    prompt: "Review the open dependency updates and open a pull request for the safe ones.\nList the ones that need a decision.",
    schedule: { kind: "weekly", time: { hour: 9, minute: 30 }, days: ["monday", "wednesday", "friday"] },
    last_run_at: LATEST_PLAIN_RUN,
  }),
  scheduledTaskOf("t-plain-morning", console_.id, {
    launch: { title: "Morning summary", compatibility: {} },
    prompt: "Summarise yesterday's commits across every project and post the summary as a page.",
    schedule: { kind: "daily", time: { hour: 8, minute: 0 } },
    last_run_at: minutesAgo(60 * 9),
  }),
  scheduledTaskOf("t-plain-index", console_.id, {
    project_id: api.id,
    launch: { title: "Index health check", compatibility: {} },
    prompt: "Check the search index health and report anything that looks stale.",
    schedule: { kind: "hourly", minute: 15 },
    last_run_at: minutesAgo(40),
  }),
  scheduledTaskOf("t-plain-wrap", console_.id, {
    launch: { title: "Wrap-up notes", compatibility: {} },
    prompt: "Write today's wrap-up notes from the sessions that ran.",
    schedule: { kind: "weekdays", time: { hour: 17, minute: 30 } },
    last_run_at: minutesAgo(60 * 22),
  }),
  scheduledTaskOf("t-plain-release", console_.id, {
    project_id: web.id,
    launch: { title: "Release notes draft", compatibility: {} },
    prompt: "Draft the release notes for the version that goes out tomorrow.",
    schedule: { kind: "once", run_at: tomorrowAt(10) },
  }),
  scheduledTaskOf("t-plain-backup", console_.id, {
    launch: { title: "Nightly backup check", compatibility: {} },
    prompt: "Confirm last night's backup finished and report its size.",
    schedule: { kind: "daily", time: { hour: 6, minute: 0 } },
    enabled: false,
    last_run_at: minutesAgo(60 * 80),
  }),
];

/** What the plain task's runs hold, most recent first: a scheduled run that started a session, a Run now that started
 * one, and a scheduled run that failed because the account was not signed in. */
const PLAIN_RUNS: ScheduledTaskRun[] = [
  scheduledRunOf("r-plain-latest", "t-plain-deps", 0, { kind: "started", session: "s-deps-1", started_independent: false }, { trigger: "scheduled", ran_at: LATEST_PLAIN_RUN, due_at: LATEST_PLAIN_RUN }),
  scheduledRunOf("r-plain-manual", "t-plain-deps", 0, { kind: "started", session: "s-deps-2", started_independent: false }, { trigger: "run_now", ran_at: MANUAL_PLAIN_RUN }),
  scheduledRunOf(
    "r-plain-signed-out",
    "t-plain-deps",
    0,
    {
      kind: "failed",
      code: "account_not_signed_in",
      params: { account: "Work", agent: "Claude Code" },
      message: "Work (Claude Code) is not logged in, so a session cannot be opened on it.",
    },
    { trigger: "scheduled", ran_at: EARLIER_PLAIN_RUN, due_at: EARLIER_PLAIN_RUN },
  ),
];

/** Closes the sidebar's drawer that pressing the rail button leaves up in a narrow window, which would hide the page the
 * scenario shows; a docked sidebar is left alone. */
const closeDrawer = (ui: Ui) => ui.closeDrawer();

/** One of `RUNS` by its id. */
const runOf = (id: string): ScheduledTaskRun => RUNS.find((run) => run.id === id)!;

/** A zone the daemon reads schedules in that is not the window's own, so the page names it. */
const OTHER_ZONE = Intl.DateTimeFormat().resolvedOptions().timeZone === "Asia/Tokyo" ? "America/New_York" : "Asia/Tokyo";

/** Presses the rail's Scheduled tasks button, which shows the page; its name counts the tasks enabled, when there are any. */
const showPage = (ui: Ui) => ui.press(describing("the rail's Scheduled tasks button", (name) => name.startsWith(ui.t("scheduledTasks.label"))));

/** Scrolls the task's page, a scroll region named by the task, to its end, where the Schedule card stands above Delete; the
 * offset is past the end, which the step clamps, so it holds whatever the window's height. */
const scrollToSchedule = (id: string) => (ui: Ui) => ui.scrollRegion(rowOf(id), 100_000);

/** The name a task's row in the sidebar goes by. */
const rowOf = (id: string): string => ([...TASKS, NEWER_KIND_TASK].find((task) => task.id === id)!.launch.title ?? "");

export const scheduledTasksScenarios: Scenario[] = [
  {
    id: "scheduled-tasks-list",
    group: GROUP,
    category: "normal",
    title: "Scheduled tasks page with several tasks",
    description:
      "The rail's Scheduled tasks button, directly above Remote devices, is drawn as the current item while the page shows, and no console avatar button is drawn as the current one (no filled circle behind an avatar); it carries a green badge reading 8, the tasks that are enabled (nine are listed and Nightly backup check is off). The sidebar lists the tasks, oldest first, one row of two lines each, with no icons: the name (the first line of the prompt for the one with no name), then the schedule in short form and the name of what the task starts (\"Mon, Wed, Fri 9:30 AM · Web app\", \"Daily 8:00 AM\", \"Hourly :15\", \"Monthly, day 31 9:00 AM\", a date for the one-off, \"Manual\"), fading at its end, and at the line's end a one-unit countdown to the next run (\"in 2 hr\", whose tooltip is the exact next run time) or Off for the one that is off. The long name is cut at the row's end. The first, a project task that reports to the console session, is selected: the header is its name and Run now, which stays beside a long name, then a quiet line of the target's icon and name and the schedule in words; below it one card holds two figures, Next run (the countdown over its time) and Last run (the time ago with an outcome mark, over its time); then two tabs, Settings (selected) and Run history: the Session card (Console and Project side by side, Name, Agent and account, Report to, Prompt, which shows an example as its placeholder, and the Advanced settings), the Schedule card (the kind and the fields it needs in one row where the area is wide, and stacked where it is narrow; Enabled below) and the Delete card, with Save and Revert (disabled until something changes) pinned in a bar at the bottom of the area that stays in view while the form scrolls. Choose a narrow width to see the sidebar become a drawer.",
    state,
    desktopApp: true,
    remoteDaemons: { entries: [staging, production] },
    scheduledTasks: { tasks: TASKS, runs: { "t-deps": RUNS } },
    steps: [showPage],
  },

  {
    id: "scheduled-tasks-list-plain",
    group: GROUP,
    category: "normal",
    title: "Scheduled tasks page, six tasks that fit their rows",
    description:
      "The page with six tasks whose names and second lines each fit their row whole: a weekly, a daily, an hourly, a weekdays and a one-off schedule, one task that is off, and both kinds of target (the project's name, or the console's own). The first, Dependency review, is selected on its Settings tab: its header shows Next run and a Last run with a success mark. Unlike Scheduled tasks page with several tasks, no name is cut and no task has a long prompt.",
    state: plainState,
    scheduledTasks: { tasks: PLAIN_TASKS, runs: { "t-plain-deps": PLAIN_RUNS } },
    steps: [showPage, closeDrawer],
  },

  {
    id: "scheduled-tasks-disabled",
    group: GROUP,
    category: "normal",
    title: "Disabled scheduled task",
    description:
      "Nightly backup check selected, a task that is turned off: its row's name is muted, with Off in place of a countdown; the header has an Off badge after the name, and Next run says Off while Last run still shows when it last ran. Run now is still there. At the end of the Settings view's Schedule card (scroll down), Enabled is off and its line says the task runs once it is turned on.",
    state,
    scheduledTasks: { tasks: TASKS },
    steps: [showPage, (ui) => ui.press(rowOf("t-backup"))],
  },

  {
    id: "scheduled-tasks-monthly",
    group: GROUP,
    category: "normal",
    title: "Monthly scheduled task",
    description:
      "Month-end ledger close selected: its Schedule card has Every month beside the Time, as every kind has, and Day of month 31 in a row of its own below, with a line under it saying that a month without that day runs on its last day (the line shows only from day 29 on). Where the card is narrow the three stack.",
    state,
    scheduledTasks: { tasks: TASKS },
    steps: [showPage, (ui) => ui.press(rowOf("t-ledger")), scrollToSchedule("t-ledger")],
  },

  {
    id: "scheduled-tasks-once",
    group: GROUP,
    category: "normal",
    title: "One-off scheduled task with its calendar open",
    description:
      "Release announcement draft selected, a one-off task: its Schedule card is the kind beside Date and time, the application's segmented date and time field, and the field's calendar button was pressed, so the calendar opens in a popover on the floating surface over the form, on the month of the chosen day, which is marked as selected (today is lightly marked too where it falls in that month, which it does not near a month's end). Any day may be picked, none is greyed out. Hover either month arrow for its tooltip; the calendar button's own tooltip shows once Escape has closed the calendar.",
    state,
    scheduledTasks: { tasks: TASKS },
    steps: [showPage, (ui) => ui.press(rowOf("t-release")), scrollToSchedule("t-release"), (ui) => ui.press(ui.t("calendar.open"))],
  },

  {
    id: "scheduled-tasks-runs",
    group: GROUP,
    category: "normal",
    title: "Scheduled task with its run history",
    description:
      "The Run history view of the selected task (the second tab under the header, after Settings; the pinned Save and Revert bar is not there), most recent first, each run leading with a check or an alert mark and separated from the next by a hairline, with when it ran and what started it (its schedule, a catch-up or Run now): a session it started, as a link that shows that session; one it started independent, with a line saying it started without reporting to a console session, because the one it was to report to could not take it; one whose session no longer exists, which says so and has no link; and two launches that failed, each showing only Could not start a session and a Details button, whose popover holds the reason (see the scenario with it open).",
    state,
    scheduledTasks: { tasks: TASKS, runs: { "t-deps": RUNS } },
    steps: [showPage, closeDrawer, (ui) => ui.press(ui.t("scheduledTasks.view.runs"))],
  },

  {
    id: "scheduled-tasks-runs-plain",
    group: GROUP,
    category: "normal",
    title: "Scheduled task run history, three ordinary runs",
    description:
      "The Run history view of Dependency review, most recent first: a scheduled run that started a session, a Run now that started one, and a run that failed because the account is not logged in, which shows Could not start a session and a Details button. The sidebar beside it lists six tasks whose names fit. Unlike Scheduled task with its run history, no run is independent, none refers to a session that is gone and none failed for an unforeseen reason.",
    state: plainState,
    scheduledTasks: { tasks: PLAIN_TASKS, runs: { "t-plain-deps": PLAIN_RUNS } },
    steps: [showPage, closeDrawer, (ui) => ui.press(ui.t("scheduledTasks.view.runs"))],
  },

  {
    id: "scheduled-tasks-runs-empty",
    group: GROUP,
    category: "normal",
    title: "Run history of a task that has not run",
    description: "The Run history view of Clean up stale branches, a manual task that has never run: the history card holds one compact line, No runs yet, with its glyph, and nothing else. The header's Last run says Never.",
    state,
    scheduledTasks: { tasks: TASKS },
    steps: [showPage, (ui) => ui.press(rowOf("t-branches")), (ui) => ui.press(ui.t("scheduledTasks.view.runs"))],
  },

  {
    id: "scheduled-tasks-zone",
    group: GROUP,
    category: "normal",
    title: "Scheduled tasks in the daemon's time zone",
    description:
      `The daemon reads schedules in ${OTHER_ZONE}, which is not this window's zone (the zone is picked to differ, so it is never the viewer's own): a quiet line under the task's header names the zone, each time carries the zone's short name, and the form's Time field says its time is in that zone.`,
    state,
    scheduledTasks: { tasks: tasksIn(OTHER_ZONE), zone: OTHER_ZONE },
    steps: [showPage, closeDrawer],
  },

  {
    id: "scheduled-tasks-newer-schedule",
    group: GROUP,
    category: "normal",
    title: "Scheduled task with a schedule this version does not know",
    description:
      "Quarterly licence review, whose schedule is of a kind a newer daemon sends: nothing words it, so its row's second line and the header's quiet line show the target alone (Website), with no schedule and no separator, and the countdown and Next run are the daemon's own (the row's spoken description keeps the next run time too). In the Settings view the Schedule card's Runs select is empty and no further schedule field follows it, only Enabled.",
    state,
    scheduledTasks: { tasks: [NEWER_KIND_TASK] },
    steps: [showPage, closeDrawer, scrollToSchedule("t-newer")],
  },

  {
    id: "scheduled-tasks-new",
    group: GROUP,
    category: "normal",
    title: "New scheduled task form",
    description:
      "New scheduled task in the sidebar's header opens the blank form under its own title, with no task selected in the sidebar: the Name field has focus, Console is the current console and Project is None (console session), the schedule is every day at 9:00 (the Time field is the application's own segmented field) and Enabled is on. Changing the schedule to a one-off shows the application's date and time field, whose calendar opens in a popover. Save and Cancel are pinned in a bar at the bottom, with no tabs, no run history and no Delete card. Choosing a project in Project adds Report to; choosing another console puts Project back to None.",
    state,
    scheduledTasks: { tasks: TASKS },
    steps: [showPage, (ui) => ui.press(ui.t("scheduledTasks.new"))],
  },

  {
    id: "scheduled-tasks-new-plain",
    group: GROUP,
    category: "normal",
    title: "New scheduled task form beside six tasks that fit their rows",
    description:
      "The blank New scheduled task form, with no task selected in the sidebar, which lists six tasks whose names fit their rows. Unlike New scheduled task form, no task has a long name or a long second line.",
    state: plainState,
    scheduledTasks: { tasks: PLAIN_TASKS, runs: { "t-plain-deps": PLAIN_RUNS } },
    steps: [showPage, closeDrawer, (ui) => ui.press(ui.t("scheduledTasks.new"))],
  },

  {
    id: "scheduled-tasks-row-menu",
    group: GROUP,
    category: "normal",
    title: "Scheduled task row's menu",
    description: "The menu a task's row opens (by its button or a right-click), here on Nightly backup check, which is off: Run now, Enable (an enabled task offers Disable), then Delete in the destructive style under a separator.",
    state,
    scheduledTasks: { tasks: TASKS },
    steps: [showPage, (ui) => ui.contextMenu(rowOf("t-backup"))],
  },

  {
    id: "scheduled-tasks-delete",
    group: GROUP,
    category: "normal",
    title: "Delete scheduled task",
    description: "Delete at the bottom of a task's page asks first, in a destructive confirmation naming the task over the page; nothing else is touched, and sessions the task already started stay as they are.",
    state,
    scheduledTasks: { tasks: TASKS },
    steps: [showPage, (ui) => ui.press(ui.t("scheduledTasks.deleteLabel", { name: rowOf("t-deps") }))],
  },

  {
    id: "scheduled-tasks-unsaved",
    group: GROUP,
    category: "normal",
    title: "Leaving a scheduled task with unsaved changes",
    description:
      "The Name of Weekly dependency review was edited and another task, Morning report, was pressed: before anything changes a destructive confirmation opens over the page, titled Discard changes, saying that the changes in this form have not been saved and will be lost, with Keep editing and Discard. The form behind it still shows the edited name with Save enabled, and Weekly dependency review is still the selected row. Keep editing, Escape and the close button return to the form with focus where it was; Discard drops the edit and shows Morning report. Revert, Cancel and the Settings and Run history tabs never ask. Remote devices asks the same question when its form is left.",
    state,
    scheduledTasks: { tasks: TASKS },
    steps: [showPage, (ui) => ui.focus(ui.t("common.name")), (ui) => ui.type("Weekly dependency review, edited"), (ui) => ui.press(rowOf("t-report"))],
  },

  {
    id: "scheduled-tasks-empty",
    group: GROUP,
    category: "normal",
    title: "Scheduled tasks page with no tasks",
    description:
      "With nothing scheduled the content area shows the empty state alone, centred in it: the glyph, No scheduled tasks yet, the line saying what a scheduled task does, and New scheduled task as the panel's action. The sidebar keeps its header with the add button and holds one compact empty row in the list's place, whose press starts New scheduled task too. The breadcrumb has no task after Scheduled tasks.",
    state,
    scheduledTasks: { tasks: [] },
    steps: [showPage],
  },

  {
    id: "scheduled-tasks-no-console",
    group: GROUP,
    category: "normal",
    title: "Scheduled tasks page with no console",
    description:
      "A task starts a session in a console, so where none exists the content area shows the empty state alone, centred: the glyph, No consoles yet, a line saying a scheduled task starts its session in a console, and New console as its action. The sidebar lists nothing but the empty row (which is no button), and its New scheduled task button is unavailable, with the reason in its tooltip.",
    state: noConsoleState,
    scheduledTasks: { tasks: [] },
    steps: [showPage],
  },

  {
    id: "scheduled-tasks-invalid",
    group: GROUP,
    category: "exceptional",
    title: "New scheduled task form with invalid fields",
    description:
      "The blank form after its schedule was set to every week with Monday, the day it starts on, unchecked, and Save was pressed with the Prompt blank: each invalid field shows its own error under it (Enter a prompt; Choose at least one day) and focus is on the first, the Prompt. Save stays enabled, since it is the press that reports what is wrong. A field that is only edited shows its error when it is left.",
    state,
    scheduledTasks: { tasks: TASKS },
    steps: [
      showPage,
      (ui) => ui.press(ui.t("scheduledTasks.new")),
      // The picker is named by the kind it holds, the default of a new task.
      (ui) => ui.press(ui.t("scheduledTasks.schedule.daily")),
      (ui) => ui.press(ui.t("scheduledTasks.schedule.weekly")),
      (ui) => ui.press(weekdayName(ui.language, "monday", "long")),
      (ui) => ui.press(ui.t("common.save")),
    ],
  },

  {
    id: "scheduled-tasks-run-details",
    group: GROUP,
    category: "exceptional",
    title: "Failed run with its Details open",
    description:
      "The Details button of a failed run in the Run history was pressed, and its popover opens by it: the reason in the application's own words, here that the account is not logged in, with the account and the agent as chips. The row itself carries only Could not start a session.",
    state,
    scheduledTasks: { tasks: TASKS, runs: { "t-deps": [runOf("r-started"), runOf("r-signed-out")] } },
    steps: [showPage, closeDrawer, (ui) => ui.press(ui.t("scheduledTasks.view.runs")), (ui) => ui.press(ui.t("error.details"))],
  },

  {
    id: "scheduled-tasks-run-details-provider",
    group: GROUP,
    category: "exceptional",
    title: "Failed run whose reason is the agent's own, in its Details",
    description:
      "The Details button of a run that failed for a reason only one agent has, here Grok Build's sandbox, was pressed: its popover says the reason with the sandbox's profile and the file that sets it as chips, then, as its own paragraph, what to do about it, in the agent's wording with its names as chips. Check that the long path wraps inside the popover and neither paragraph overflows it.",
    state,
    scheduledTasks: { tasks: GROK_TASKS, runs: { "t-deps": [runOf("r-started"), SANDBOX_RUN] } },
    steps: [showPage, closeDrawer, (ui) => ui.press(ui.t("scheduledTasks.view.runs")), (ui) => ui.press(ui.t("error.details"))],
  },

  {
    id: "scheduled-tasks-run-details-raw",
    group: GROUP,
    category: "exceptional",
    title: "Failed run with an unforeseen failure in its Details",
    description:
      "The Details button of a run that failed for a reason the application has no words for was pressed: its popover says that something unexpected went wrong, with the daemon's raw text under it in a block of its own that scrolls sideways, and a button to copy it.",
    state,
    scheduledTasks: { tasks: TASKS, runs: { "t-deps": [runOf("r-started"), runOf("r-unforeseen")] } },
    steps: [showPage, closeDrawer, (ui) => ui.press(ui.t("scheduledTasks.view.runs")), (ui) => ui.press(ui.t("error.details"))],
  },

  {
    id: "scheduled-tasks-runs-failed",
    group: GROUP,
    category: "exceptional",
    title: "Run history that could not be read",
    description:
      "The Run history view of the selected task when the daemon refuses to list its runs: the history card holds the failure as the whole subject, with an alert glyph, Could not read the scheduled task's runs, the application's generic sentence, the daemon's raw message under it and Try again. The header and the Settings view are unaffected.",
    state,
    scheduledTasks: { tasks: TASKS, runsError: LISTING_FAILURE },
    steps: [showPage, closeDrawer, (ui) => ui.press(ui.t("scheduledTasks.view.runs"))],
  },

  {
    id: "scheduled-tasks-list-failed",
    group: GROUP,
    category: "exceptional",
    title: "Scheduled tasks that could not be read",
    description:
      "The daemon refuses to list the tasks: the content area holds the failure as the whole subject, Could not read the scheduled tasks, with the application's generic sentence, the daemon's raw message and Try again, and the sidebar keeps its header and holds one compact failure row in the list's place: a danger glyph, the sentence Could not read the scheduled tasks and a Try again button beside it. No alert opens over the window.",
    state,
    scheduledTasks: { tasks: [], listError: LISTING_FAILURE },
    steps: [showPage],
  },
];
