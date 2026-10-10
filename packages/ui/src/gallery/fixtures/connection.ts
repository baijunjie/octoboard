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
    title: "Connecting, no snapshot yet",
    state: { connectionState: "connecting" },
  },
  {
    id: "daemon-closed-before-snapshot",
    group: STARTUP_AND_CONNECTION,
    title: "Never connected, attempts spent",
    description: "The banner with Try again over a window with nothing to show.",
    state: { connectionState: "closed" },
  },
  {
    id: "daemon-reconnecting",
    group: STARTUP_AND_CONNECTION,
    title: "Reconnecting",
    description: "The connection banner along the bottom over the last known state.",
    state: connected("reconnecting"),
    steps: [(ui) => ui.press(ui.session("Fix the summary layout"))],
  },
  {
    id: "daemon-lost",
    group: STARTUP_AND_CONNECTION,
    title: "Reconnect budget spent",
    description: "The banner offers Try again.",
    state: connected("closed"),
    steps: [(ui) => ui.press(ui.session("Fix the summary layout"))],
  },
  {
    id: "daemon-lost-behind-dialog",
    group: STARTUP_AND_CONNECTION,
    title: "Reconnect budget spent with a dialog open",
    description:
      "The file viewer is opened first and the connection dropped under it, the order a user meets this in. A modal's backdrop covers the whole viewport, the banner's strip included, so a press where Try again is drawn has to reach the button rather than the backdrop, and must not dismiss the viewer.",
    width: 1440,
    state: connected("open"),
    steps: [
      (ui) => ui.press(ui.session("Fix the summary layout")),
      (ui) => ui.wait(400),
      (ui) => ui.press("src"),
      (ui) => ui.press("main.ts"),
      (ui) => ui.wait(600),
      (ui) => ui.dropConnection(),
    ],
  },
  {
    id: "daemon-lost-behind-dialog-narrow",
    group: STARTUP_AND_CONNECTION,
    title: "Reconnect budget spent with a dialog open, narrow",
    description:
      "The same state below the docked breakpoint, where a dialog fills the window inside a margin smaller than the strip: the dialog has to end clear of the strip rather than run under it, and its bottom edge and rounded corners have to be drawn.",
    width: 800,
    state: connected("open"),
    steps: [
      (ui) => ui.press(ui.session("Fix the summary layout")),
      (ui) => ui.wait(400),
      (ui) => ui.press("src"),
      (ui) => ui.press("main.ts"),
      (ui) => ui.wait(600),
      (ui) => ui.dropConnection(),
    ],
  },
  {
    id: "terminal-output",
    group: TERMINAL,
    title: "A terminal with output",
    description: "A canned ANSI sample: colours, box drawing, CJK and a long line.",
    state: connected("open"),
    steps: [(ui) => ui.press(ui.session("Fix the summary layout"))],
  },
  {
    id: "terminal-reconnecting",
    group: TERMINAL,
    title: "Terminal reconnecting",
    description: "The session is live but its terminal socket never connects: after the first failed attempt the terminal stays covered by the spinner and \"Reconnecting the terminal…\".",
    terminal: "refuse",
    state: connected("open"),
    steps: [(ui) => ui.press(ui.session("Fix the summary layout")), (ui) => ui.wait(1500)],
  },
  {
    id: "terminal-disconnected",
    group: TERMINAL,
    title: "Terminal disconnected",
    description: "The automatic attempts are spent (run back to back here instead of over about 30 seconds): the terminal is covered by an unplugged icon, \"Terminal disconnected\" and a Reconnect button in the centre, reachable with F6; pressing it reconnects and puts focus on the terminal.",
    terminal: "refuse",
    terminalRetriesImmediate: true,
    state: connected("open"),
    steps: [(ui) => ui.press(ui.session("Fix the summary layout")), (ui) => ui.wait(2000)],
  },
  {
    id: "terminal-not-running",
    group: TERMINAL,
    title: "Not running, with Resume",
    description: "An interrupted session selected; the fixture's resume is a no-op, so the overlay stays.",
    state: connected("open"),
    steps: [(ui) => ui.press(ui.session("Interrupted by a restart")), (ui) => ui.wait(500)],
  },
  {
    id: "terminal-archived",
    group: TERMINAL,
    title: "Archived session",
    state: connected("open"),
    steps: [
      (ui) => ui.press(ui.t("sidebar.project.actions", { name: project.name })),
      (ui) => ui.press(ui.t("sidebar.project.archive")),
      (ui) => ui.press("Archived and done"),
    ],
  },
];
