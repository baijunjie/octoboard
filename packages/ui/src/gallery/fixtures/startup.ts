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
  { id: "startup-no-address", group: GROUP, title: "No daemon address", startup: { kind: "noAddress" } },
  {
    id: "startup-daemon-failed",
    group: GROUP,
    title: "The daemon failed to start",
    description:
      "The daemon's own report, as long as the daemon made it. The screen scrolls once the report and its Quit button do not fit the window: use the Short height to see the content fade at the bottom edge, and the window at its default height to see it whole, with no fade.",
    startup: DAEMON_FAILED,
  },
  {
    id: "startup-daemon-failed-focused",
    group: GROUP,
    title: "The daemon failed to start, the report focused",
    description:
      "The report too long for the window, tabbed to: the whole column takes the focus ring, drawn inside the window's edge above the fade, and scrolls by keyboard to the buttons at its end. At the default height it fits and takes no focus.",
    height: 480,
    startup: DAEMON_FAILED,
    steps: [(ui) => ui.tabToRegion(ui.t("startup.message"))],
  },
  {
    id: "startup-crash",
    group: GROUP,
    title: "The UI crashed",
    description:
      "The error boundary's fallback, with Try again. The stack is a block of its own, so a stack taller than the window scrolls inside it: use the Short height to see the block fade at its bottom edge, and the default height to see it whole, with no fade.",
    startup: { kind: "crash", stack: stackOf(12) },
  },
  {
    id: "startup-crash-focused",
    group: GROUP,
    title: "The UI crashed, a long stack focused",
    description:
      "A stack far taller than the block that holds it, tabbed to: the focus ring is the frame round the block, drawn whole above the fade at the block's bottom edge, and the stack scrolls by keyboard.",
    startup: { kind: "crash", stack: stackOf(60) },
    steps: [(ui) => ui.tabToRegion(ui.t("error.stack"))],
  },
];
