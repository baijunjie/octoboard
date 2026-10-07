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

const DAEMON = "Daemon connection";
const TERMINAL = "Terminal";

export const connectionScenarios: Scenario[] = [
  {
    id: "daemon-connecting",
    group: DAEMON,
    title: "Connecting, no snapshot yet",
    state: { connectionState: "connecting" },
  },
  {
    id: "daemon-closed-before-snapshot",
    group: DAEMON,
    title: "Never connected, attempts spent",
    description: "The banner with Try again over a window with nothing to show.",
    state: { connectionState: "closed" },
  },
  {
    id: "daemon-reconnecting",
    group: DAEMON,
    title: "Reconnecting",
    description: "The connection banner along the bottom and the top bar's turning-arrow icon over the last known state.",
    state: connected("reconnecting"),
    steps: [(ui) => ui.press(ui.session("Fix the summary layout"))],
  },
  {
    id: "daemon-lost",
    group: DAEMON,
    title: "Reconnect budget spent",
    description: "The banner offers Try again; the top bar's unplugged icon says Disconnected.",
    state: connected("closed"),
    steps: [(ui) => ui.press(ui.session("Fix the summary layout"))],
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
    description: "The session is live but its terminal socket never connects: the top bar's turning-arrow icon after the first failed attempt.",
    terminal: "refuse",
    state: connected("open"),
    steps: [(ui) => ui.press(ui.session("Fix the summary layout")), (ui) => ui.wait(1500)],
  },
  {
    id: "terminal-disconnected",
    group: TERMINAL,
    title: "Terminal disconnected",
    description: "Opens after the automatic attempts are spent (about 30 seconds): the unplugged icon with Reconnect beside it.",
    terminal: "refuse",
    state: connected("open"),
    steps: [(ui) => ui.press(ui.session("Fix the summary layout")), (ui) => ui.wait(32_000)],
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
