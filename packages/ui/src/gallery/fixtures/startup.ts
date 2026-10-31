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
    title: "No daemon address or key",
    description:
      "One main landmark contains the page heading, centred, and a danger alert titled \"Could not connect to the daemon\" with one line under it; the alert and the text below it start at the edge. Outside the alert, a line on the shell's handoff of the port and the key follows, then the three development-server connection methods, each naming the key beside the port, with selectable code tokens. The lines are long: use Narrow and Short to check that they wrap, and that the code tokens wrap or scroll without being clipped; Arabic shows the code inside right-to-left copy. After the methods come the sentence saying that, if the steps above do not resolve it, Report issue files an Issue, and, centred under it, the Report issue button (a link in the shape of a button that opens the Issue page prefilled with the title \"No daemon address or key\"); there is no Quit.",
    startup: { kind: "noAddress" },
  },
  {
    id: "startup-daemon-failed",
    group: GROUP,
    category: "exceptional",
    title: "The daemon failed to start",
    description:
      "The page is the whole failure, in the empty-state shape, with no error dialog over it: a danger glyph, the heading naming the failure as the page's only h1 with no mark beside it, one line saying what the daemon is, and under it a second line saying that Octoboard cannot work without it and should be opened again. Under them, start-aligned on a surface of its own, are the sentence saying that, if the steps above do not resolve it, Report issue files an Issue, and then the daemon's text, wrapped over as many lines as it needs and no horizontal scroll. The text is taller than the surface allows (up to 40% of the window's height), so it scrolls vertically inside the surface with a fade at the edge it hides the rest behind; Tab onto the text, named Error details, for its focus ring and keyboard scrolling. Use Short to see the page scroll as well, its region named Message. The row under the surface holds Report issue alone (a link in the shape of a button that opens the Issue page prefilled with the title \"The daemon failed to start\"): there is no Try again, since nothing can be retried from here, no Quit and no Details button.",
    startup: DAEMON_FAILED,
  },
  {
    id: "startup-crash",
    group: GROUP,
    category: "exceptional",
    title: "The UI crashed",
    description:
      "The crash page in the empty-state shape, with no error dialog over it: a danger glyph, the heading \"An unexpected error occurred\" as the page's only h1 with no mark beside it, one line saying that Octoboard hit an error it could not recover from, and under it \"Try again. If the error comes back, report it.\" Under them, start-aligned on a surface of its own, is the stack alone, wrapped, with no horizontal scroll. The stack is taller than the surface allows (up to 40% of the window's height), so it scrolls vertically inside the surface with a fade at the edge it hides the rest behind; Tab onto the stack, named Error details, for its focus ring and keyboard scrolling. Use Short to see the page scroll as well, its region named Message. The row under the surface holds Try again as the primary button, then Report issue (a link in the shape of a button that opens the prefilled Issue page): no Quit and no Details.",
    startup: { kind: "crash", stack: stackOf(12) },
  },
];
