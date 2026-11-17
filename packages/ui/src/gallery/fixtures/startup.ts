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
  cause: "instanceLocked",
  // The shell's own message for a start that failed three times, the last with the daemon refusing a data directory
  // another daemon holds: the daemon's stderr, which ends with a newline, closes it.
  error:
    "the daemon did not start in 3 attempts. The last one: the daemon stopped with exit code 73 before reporting its port and key. " +
    "Error: another octoboardd is already running against /Users/dev/.octoboard\n",
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
    desktopApp: true,
    description:
      "The page is the whole failure, in the empty-state shape, with no error dialog over it: a danger glyph, the heading naming the failure as the page's only h1 with no mark beside it, one line saying what the daemon is, and under it a second line, the remedy, saying that another copy of Octoboard is already using the same application data folder and to quit it, then restart this app. Under them, start-aligned on a surface of its own, are the sentence saying that, if the steps above do not resolve it, Report issue files an Issue, and then the daemon's text as one original long line, scrolling horizontally without automatic wrapping. The block takes only the height it needs, up to 40% of the window's height, and the floating Copy button stays at its top right and copies the complete original text; Tab onto the text, named Error details, for its focus ring and keyboard scrolling. Use Short to see the page scroll as well, its region named Message. The row under the surface holds the primary Restart button, as in the desktop app, and Report issue after it (a link in the shape of a button that opens the Issue page prefilled with the title \"The daemon failed to start\"): there is no Try again, since nothing can be retried from here, no Quit and no Details button. Pressing Restart holds the button pending, and it stays so, since the gallery's shell never closes the window.",
    startup: DAEMON_FAILED,
  },
  {
    id: "startup-crash",
    group: GROUP,
    category: "exceptional",
    title: "The UI crashed",
    description:
      "The crash page in the empty-state shape, with no error dialog over it: a danger glyph, the heading \"An unexpected error occurred\" as the page's only h1 with no mark beside it, one line saying that Octoboard hit an error it could not recover from, and under it \"Try again. If the error comes back, report it.\" Under them, start-aligned on a surface of its own, is the stack alone, preserving its original line breaks and indentation without automatic wrapping, with horizontal scrolling when its lines exceed the available width. The floating Copy button stays at its top right while scrolling and copies the complete original stack. The stack is taller than the surface allows (up to 40% of the window's height), so it scrolls vertically inside the surface with a fade at the edge it hides the rest behind; Tab onto the stack, named Error details, for its focus ring and keyboard scrolling. Use Short to see the page scroll as well, its region named Message. The row under the surface holds Try again as the primary button, then Report issue (a link in the shape of a button that opens the prefilled Issue page): no Quit and no Details.",
    startup: { kind: "crash", stack: stackOf(12) },
  },
];
