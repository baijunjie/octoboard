import type { Scenario, Startup } from "../scenario";

const GROUP = "Startup and connection";

/** A stack of `frames` frames, in the shape a browser prints one: the error's own line, then one
 * `at` line per frame. */
function stackOf(frames: number): string {
  const sites = [
    "SessionRow (src/sidebar/SessionRow.tsx:88:17)",
    "ProjectSection (src/sidebar/ProjectSection.tsx:142:9)",
    "Sidebar (src/sidebar/Sidebar.tsx:61:5)",
    "ContentPanel (src/components/ContentPanel.tsx:23:3)",
    "App (src/App.tsx:210:7)",
  ];
  return [
    "Error: Gallery: a component threw while rendering.",
    ...Array.from({ length: frames }, (_, i) => `    at ${sites[i % sites.length]}`),
  ].join("\n");
}

const DAEMON_FAILED: Startup = {
  kind: "daemonFailed",
  error:
    "another instance already holds the lock at /Users/dev/.octoboard/daemon.lock (held by process 4821, started at 2026-10-12 09:14 from /Applications/Octoboard.app/Contents/MacOS/octoboardd). " +
    "Caused by: failed to acquire an exclusive lock on /Users/dev/.octoboard/daemon.lock: Resource temporarily unavailable (os error 35). " +
    "Caused by: the previous instance did not answer on /Users/dev/.octoboard/daemon.sock within 5 seconds: Connection timed out (os error 60). " +
    "Caused by: no port file was found at /var/folders/zz/octoboard/T/octoboardd.port, so the instance could not be asked to shut down. " +
    "Caused by: the database at /Users/dev/.octoboard/octoboard.db is opened by the same process, which was last seen writing to /Users/dev/.octoboard/octoboard.db-wal at 2026-10-12 09:58, " +
    "and a second connection would risk corrupting it: refusing to continue. " +
    "Caused by: the supervisor that restarted the daemon three times in the last minute, at 2026-10-12 09:57:12, 09:57:31 and 09:57:50, gave up after the third attempt, " +
    "and left the state it found in /Users/dev/.octoboard/supervisor.state untouched so that the cause could be looked into: " +
    "the log of the last attempt is at /Users/dev/.octoboard/logs/octoboardd-2026-10-12.log. " +
    "Close the other instance, or remove /Users/dev/.octoboard/daemon.lock if no process holds it, and start Octoboard again.",
};

export const startupScenarios: Scenario[] = [
  {
    id: "startup-no-address",
    group: GROUP,
    category: "exceptional",
    title: "No daemon address",
    description:
      "One main landmark contains the page heading, centred, and a danger alert titled \"Could not connect to the daemon\" with one line under it; the alert and the text below it start at the edge. The shell's port handoff and three development-server connection instructions follow outside the alert, with selectable code tokens. Use Narrow and Short to check wrapping and scrolling; Arabic shows the code inside right-to-left copy. The native-capable browser stub exposes Quit without closing the preview.",
    quitRequested: true,
    startup: { kind: "noAddress" },
  },
  {
    id: "startup-daemon-failed",
    group: GROUP,
    category: "exceptional",
    title: "The daemon failed to start",
    description:
      "The error dialog has been acknowledged, leaving the page that is the whole failure, in the empty-state shape: a danger glyph, the heading naming the failure as the page's only h1 with a danger mark (a circled !, named \"Why the daemon did not start\") right after it, and one line in the application's own words saying what to do. The row under it holds Quit alone: there is no Try again, since nothing can be retried from here, and no Details button. No daemon text is on the page; it is in the mark's popover, shown in the next scenario. The native-capable browser stub exposes Quit without closing the preview.",
    quitRequested: true,
    startup: DAEMON_FAILED,
    steps: [(ui) => ui.press(ui.t("error.acknowledge"))],
  },
  {
    id: "startup-daemon-failed-details",
    group: GROUP,
    category: "exceptional",
    title: "The daemon failed to start, the mark's popover open",
    description:
      "The mouse rests on the danger mark after the heading: its popover opens with no click and no focus taken, holding only the daemon's text, wrapped over as many lines as it needs, on a surface of its own; nothing in the application's own words repeats there. The text is far taller than the window: the surface holds as much as the popover allows and the text scrolls vertically inside it with a fade at the edge it hides the rest behind, and no horizontal scroll. Use Short to see that, then Tab onto the text, named Error details, for its focus ring and keyboard scrolling.",
    quitRequested: true,
    startup: DAEMON_FAILED,
    steps: [(ui) => ui.press(ui.t("error.acknowledge")), (ui) => ui.hoverStatusMark(ui.t("startup.daemonFailed.markLabel"))],
  },
  {
    id: "startup-daemon-failed-alert",
    group: GROUP,
    category: "exceptional",
    title: "The daemon failed to start, the error dialog",
    description:
      "The error dialog that opens over the page, titled \"The daemon failed to start\": its body is only the application's own sentence (\"The daemon did not start.\") and the report guidance. The daemon's text is behind Error details, open here, in a wrapping block on its own surface, not as the dialog's body text. The message is taller than the dialog's body, which scrolls with a fade at its bottom edge. The page behind is dimmed and is looked at in the scenarios above.",
    quitRequested: true,
    startup: DAEMON_FAILED,
    steps: [(ui) => ui.press(ui.t("error.diagnostics"))],
  },
  {
    id: "startup-crash",
    group: GROUP,
    category: "exceptional",
    title: "The UI crashed",
    description:
      "The error dialog has been acknowledged, leaving the crash page in the empty-state shape: a danger glyph, the heading \"An unexpected error occurred\" as the page's only h1 with a danger mark (a circled !, named \"Why Octoboard stopped\") right after it, and one line in the application's own words reading \"Try again. If the error comes back, report it.\" The row under it holds Try again as the primary button and Quit: two buttons, no Details. No error text is on the page; the stack is in the mark's popover, shown in the next scenario. The native-capable browser stub exposes Quit without closing the preview.",
    quitRequested: true,
    startup: { kind: "crash", stack: stackOf(12) },
    steps: [(ui) => ui.press(ui.t("error.acknowledge"))],
  },
  {
    id: "startup-crash-details",
    group: GROUP,
    category: "exceptional",
    title: "The UI crashed, the mark's popover open",
    description:
      "The mouse rests on the danger mark after the heading: its popover opens with no click and no focus taken, holding only the stack, on a surface of its own named Error details. The stack is as tall as the popover allows (up to 40% of the window's height), then scrolls (both ways) with a fade at the edge it hides the rest behind. Use Short to see that, then Tab onto the stack for its focus ring and keyboard scrolling; Fit shows it whole without a fade.",
    quitRequested: true,
    startup: { kind: "crash", stack: stackOf(12) },
    steps: [(ui) => ui.press(ui.t("error.acknowledge")), (ui) => ui.hoverStatusMark(ui.t("error.crashed.markLabel"))],
  },
  {
    id: "startup-crash-alert",
    group: GROUP,
    category: "exceptional",
    title: "The UI crashed, the error dialog",
    description:
      "The error dialog that opens over the crash page, titled \"An unexpected error occurred\": its body is only the application's own sentence (that Octoboard hit an error it could not recover from) and the report guidance. The stack is behind Error details, open here, on a surface of its own that keeps the danger tone, as tall as the dialog allows with a fade at the edge it hides the rest behind. The page behind is dimmed and is looked at in the scenarios above.",
    quitRequested: true,
    startup: { kind: "crash", stack: stackOf(12) },
    steps: [(ui) => ui.press(ui.t("error.diagnostics"))],
  },
];
