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
      "One main landmark contains the page heading, centred, a danger alert titled \"Could not connect to the daemon\" with one line under it, and the daemon's diagnostic outside it, on a secondary surface of its own that keeps the danger tone: selectable code that wraps over as many lines as it needs, with no scroll region, fade or Tab stop of its own. All but the heading and the Quit button start at the edge. The error dialog holds only the application's own words (\"The daemon did not start.\") and the report guidance; the same raw text is behind its Error details disclosure, closed here, where it opens as a wrapping block on its own surface, tall enough that the dialog's body scrolls with a fade at its bottom edge. Acknowledge it to inspect the page, whose own column scrolls (a fade at its bottom edge) down to the Quit button. The native-capable browser stub exposes Quit without closing the preview.",
    quitRequested: true,
    startup: DAEMON_FAILED,
  },
  {
    id: "startup-crash",
    group: GROUP,
    category: "exceptional",
    title: "The UI crashed",
    description:
      "One main landmark contains the crash page heading, centred, a danger alert titled \"An unexpected error occurred\" over an instruction in the application's own words, which starts at the edge, and a separate centred Try again control. Acknowledge the error dialog to inspect the page. The error's message is not in the alert: it heads the stack below it, which sits on a secondary surface of its own that keeps the danger tone. The error dialog above the page says only \"Something unexpected went wrong.\" with the report guidance, and holds the same stack behind its Error details disclosure, closed here. The page's stack is as tall as the window allows up to 40% of its height, then scrolls (both ways) with a fade at the edge it hides the rest behind. Use Short to see that, then Tab onto the stack (named Error details) for its focus ring and keyboard scrolling; Fit shows it whole without a fade. The native-capable browser stub exposes Quit without closing the preview.",
    quitRequested: true,
    startup: { kind: "crash", stack: stackOf(12) },
  },
];
