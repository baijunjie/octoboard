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

const starting = sessionOf("s-starting", console_.id, project.id, "Just created, still launching", "starting");

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
    description: "Try again has keyboard focus while the file viewer stays open. F6 moves between the viewer and the banner. Enter retries without dismissing the viewer, and so does a pointer press on Try again. After returning, Tab stays inside the viewer and Escape still closes it. Narrow the window: the viewer ends above the banner with its bottom edge and rounded corners visible.",
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
    description: "The read-only archived terminal keeps readable coloured output, the prompt and follow-up instruction and a long wrapping line, and its breadcrumb shows the archived status. The notice above the terminal output offers Reopen, which reopens the session.",
    state: connected("open"),
    steps: [
      (ui) => ui.press(ui.t("sidebar.project.actions", { name: project.name })),
      (ui) => ui.press(ui.t("sidebar.project.archive")),
      (ui) => ui.press((name) => name === "Archived and done"),
    ],
  },
  {
    id: "terminal-edit-menu",
    group: TERMINAL,
    category: "normal",
    title: "Terminal edit menu without a selection",
    description: "The right-click menu offers Copy, Paste and Select all, with Copy disabled without selected text. Paste uses a private sample clipboard containing echo hello, and copying updates only that clipboard.",
    clipboard: { text: "echo hello" },
    state: connected("open"),
    steps: [(ui) => ui.press(ui.session("Fix the summary layout")), (ui) => ui.terminalContextMenu()],
  },
  {
    id: "terminal-edit-menu-selected",
    group: TERMINAL,
    category: "normal",
    title: "Terminal edit menu with selected output",
    description: "Selected terminal output stays highlighted while the menu is open, and Copy is enabled. Copy and Paste share the private sample clipboard.",
    clipboard: { text: "echo hello" },
    state: connected("open"),
    steps: [
      (ui) => ui.press(ui.session("Fix the summary layout")),
      (ui) => ui.terminalContextMenu(),
      (ui) => ui.press(ui.t("terminal.selectAll")),
      (ui) => ui.terminalContextMenu(),
    ],
  },
  {
    id: "terminal-fullscreen-links",
    group: TERMINAL,
    category: "normal",
    title: "Links in a full-screen terminal response",
    description: "The localhost address and the labelled \"layout notes\" hyperlink are blue without hovering, even inside muted output, while the prose around them stays muted. Cmd/Ctrl+click opens their addresses.",
    terminal: "fullscreen",
    preferences: { asideVisible: false },
    state: connected("open"),
    steps: [(ui) => ui.press(ui.session("Fix the summary layout")), (ui) => ui.wait(400)],
  },
  {
    id: "terminal-link-tooltip",
    group: TERMINAL,
    category: "normal",
    title: "Hovering a terminal hyperlink",
    description: "The pointer is on the \"layout notes\" hyperlink: a tooltip names the link's real address, which its text does not say, and the address reads left to right in Arabic too.",
    state: connected("open"),
    steps: [(ui) => ui.press(ui.session("Fix the summary layout")), (ui) => ui.wait(400), (ui) => ui.hoverTerminalLink("layout notes")],
  },
  {
    id: "terminal-broken-url-hover",
    group: TERMINAL,
    category: "normal",
    title: "Hovering a URL the program broke across rows",
    description: "The pointer is on the last row of an address the program itself broke onto the next rows: the whole address, on every row it spans, is underlined, and Cmd/Ctrl+click opens the full address. The window's width decides where it breaks.",
    state: connected("open"),
    steps: [(ui) => ui.press(ui.session("Fix the summary layout")), (ui) => ui.wait(400), (ui) => ui.hoverTerminalUrl("lang=zh-Hans")],
  },
  {
    id: "terminal-starting",
    group: TERMINAL,
    category: "normal",
    title: "Terminal of a session still starting",
    description: "The selected session's agent is still being launched: the terminal is covered by a spinner and \"Starting session…\", with no Reconnect button. Its sidebar row shows the muted turning arc.",
    state: snapshotState({ consoles: [console_], projects: [project], sessions: [...sessions, starting] }),
    steps: [(ui) => ui.press(ui.session(starting.title))],
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
    description: "The automatic attempts are spent: the terminal is covered by an unplugged icon, \"Terminal disconnected\" and a centred Reconnect button, reachable with F6. Pressing it reconnects and puts focus on the terminal.",
    terminal: "refuse",
    terminalRetriesImmediate: true,
    state: connected("open"),
    steps: [(ui) => ui.press(ui.session("Fix the summary layout")), (ui) => ui.wait(2000)],
  },
];
