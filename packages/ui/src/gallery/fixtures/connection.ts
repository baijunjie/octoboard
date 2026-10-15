import type { Scenario } from "../scenario";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./builders";

const console_ = consoleOf("c-1", "Main");
const project = projectOf("p-1", console_.id, "Website");
const sessions = [
  sessionOf("s-console", console_.id, undefined, "Hub", "idle"),
  sessionOf("s-working", console_.id, project.id, "Fix the summary layout", "working"),
  sessionOf("s-interrupted", console_.id, project.id, "Interrupted by a restart", "interrupted"),
  sessionOf("s-archived", console_.id, project.id, "Archived and done", "archived"),
];
const connected = (connectionState: "open" | "reconnecting" | "closed") =>
  snapshotState({ consoles: [console_], projects: [project], sessions, connectionState });

const STARTUP_AND_CONNECTION = "Startup and connection";
const TERMINAL = "Terminal";

export const connectionScenarios: Scenario[] = [
  {
    id: "daemon-connecting",
    group: STARTUP_AND_CONNECTION,
    category: "normal",
    title: "Connecting, no snapshot yet",
    state: { connectionState: "connecting" },
  },
  {
    id: "daemon-closed-before-snapshot",
    group: STARTUP_AND_CONNECTION,
    category: "exceptional",
    title: "Never connected, attempts spent",
    description: "The banner with Try again over a window with nothing to show.",
    state: { connectionState: "closed" },
  },
  {
    id: "daemon-reconnecting",
    group: STARTUP_AND_CONNECTION,
    category: "exceptional",
    title: "Reconnecting",
    description: "The connection banner along the bottom over the last known state.",
    state: connected("reconnecting"),
    steps: [(ui) => ui.press(ui.session("Fix the summary layout"))],
  },
  {
    id: "daemon-lost",
    group: STARTUP_AND_CONNECTION,
    category: "exceptional",
    title: "Reconnect budget spent",
    description: "The banner offers Try again.",
    state: connected("closed"),
    steps: [(ui) => ui.press(ui.session("Fix the summary layout"))],
  },
  {
    id: "daemon-lost-behind-dialog",
    group: STARTUP_AND_CONNECTION,
    category: "exceptional",
    title: "Reconnect budget spent with a dialog open",
    description:
      "Try again has keyboard focus while the file viewer stays open. F6 returns to the viewer; another F6 reaches the banner again, and Enter retries without dismissing the viewer. After returning, Tab stays inside the viewer and Escape still closes it. A pointer press on Try again also leaves the viewer open. Narrow the window too: the viewer ends above the banner with its bottom edge and rounded corners visible.",
    state: connected("open"),
    steps: [
      (ui) => ui.press(ui.session("Fix the summary layout")),
      (ui) => ui.wait(400),
      (ui) => ui.press("src"),
      (ui) => ui.press("main.ts"),
      (ui) => ui.wait(600),
      (ui) => ui.dropConnection(),
      (ui) => ui.key("F6"),
    ],
  },
  {
    id: "terminal-output",
    group: TERMINAL,
    category: "normal",
    title: "Archived terminal with readable output",
    description: "The read-only archived terminal retains readable coloured output, the prompt and follow-up instruction, and a long wrapping line. Its breadcrumb shows the archived status.",
    state: connected("open"),
    steps: [
      (ui) => ui.press(ui.t("sidebar.project.actions", { name: project.name })),
      (ui) => ui.press(ui.t("sidebar.project.archive")),
      (ui) => ui.press((name) => name === "Archived and done"),
    ],
  },
  {
    id: "terminal-link-tooltip",
    group: TERMINAL,
    category: "normal",
    title: "Hovering a terminal hyperlink",
    description: "The pointer is on the \"layout notes\" hyperlink in the output: a tooltip over it names the link's real address, which its text does not say; the address reads left to right in Arabic too.",
    state: connected("open"),
    steps: [(ui) => ui.press(ui.session("Fix the summary layout")), (ui) => ui.wait(400), (ui) => ui.hoverTerminalLink("layout notes")],
  },
  {
    id: "terminal-reconnecting",
    group: TERMINAL,
    category: "exceptional",
    title: "Terminal reconnecting",
    description: "The session is live but its terminal socket never connects: after the first failed attempt the terminal stays covered by the spinner and \"Reconnecting the terminal…\".",
    terminal: "refuse",
    state: connected("open"),
    steps: [(ui) => ui.press(ui.session("Fix the summary layout")), (ui) => ui.wait(1500)],
  },
  {
    id: "terminal-disconnected",
    group: TERMINAL,
    category: "exceptional",
    title: "Terminal disconnected",
    description: "The automatic attempts are spent (run back to back here instead of over about 30 seconds): the terminal is covered by an unplugged icon, \"Terminal disconnected\" and a Reconnect button in the centre, reachable with F6; pressing it reconnects and puts focus on the terminal.",
    terminal: "refuse",
    terminalRetriesImmediate: true,
    state: connected("open"),
    steps: [(ui) => ui.press(ui.session("Fix the summary layout")), (ui) => ui.wait(2000)],
  },
];
