import type { Scenario } from "../scenario";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./builders";
import { PATH_LINK_FILES, PATH_LINK_WORKTREE_FILES } from "./pathLinkFiles";

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

/** What the path link scenarios share: the sample output naming files, and which of them the fixture
 * daemon finds (and, for a file the viewer shows, where it reads it from). */
const PATH_LINK_SCENARIO = {
  terminal: "paths",
  files: PATH_LINK_FILES,
  worktreeFiles: { "wt-side": PATH_LINK_WORKTREE_FILES },
  pathLinks: {
    "/Users/dev/code/website/src/summary/Summary.css": { view: { path: "src/summary/Summary.css" } },
    "/Users/dev/code/website/src/summary/Summary.tsx": { view: { path: "src/summary/Summary.tsx" } },
    "/Users/dev/code/website/docs/summary-notes.md": { view: { path: "docs/summary-notes.md" } },
    "/Users/dev/code/website/bin/helper": { view: { path: "bin/helper" }, viewerOnly: true },
    "/Users/dev/code/website/docs/plans/summary-layout": { kind: "directory", view: { path: "docs/plans/summary-layout/README.md" } },
    "/Users/dev/code/search-api-experiments/src/ui.ts": { view: { worktree: "wt-side", path: "src/ui.ts" } },
    "/private/tmp/octoboard/scratchpad/icon/sheet3.png": "file",
  },
  state: connected("open"),
} satisfies Partial<Scenario>;

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
    id: "terminal-edit-menu-no-clipboard",
    group: TERMINAL,
    category: "normal",
    title: "Terminal edit menu on a platform with no clipboard",
    description: "With output selected, Copy is still enabled and Paste is disabled: only reading the clipboard needs the platform's clipboard.",
    clipboard: "unavailable",
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
    description: "The localhost address and the labelled \"layout notes\" hyperlink are blue without hovering, even inside muted output, while the prose around them stays muted. Clicking opens their addresses.",
    terminal: "fullscreen",
    preferences: { panelStacks: { project: { open: [], shares: {} }, report: { open: [], shares: {} } } },
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
    description: "The pointer is on the last row of an address the program itself broke onto the next rows: the whole address, on every row it spans, is underlined, and clicking opens the full address. The window's width decides where it breaks.",
    state: connected("open"),
    steps: [(ui) => ui.press(ui.session("Fix the summary layout")), (ui) => ui.wait(400), (ui) => ui.hoverTerminalUrl("lang=zh-Hans")],
  },
  {
    id: "terminal-path-links",
    group: TERMINAL,
    category: "normal",
    title: "File paths in terminal output",
    description:
      "The paths the daemon finds are in the same blue as the address on the last line, without hovering: the one in Read(...), the absolute project path, src/summary/Summary.tsx:42:7, docs/summary-notes.md:13, the side worktree's src/ui.ts:7, bin/helper, the folder docs/plans/summary-layout, the temp-folder path and the one after @, where the colour starts at src, not at the @. " +
      "The closing parenthesis, the full stop after :42:7, the commas after :13 and after the temp path and the semicolon after the @ mention stay in the output's own colour. " +
      "Three paths stay plain: src/summary/Draft.ts, which does not exist, src/shared/index.ts, which the daemon answers none for as it would one that more than one of the project's folders holds, and app/[id]/page.tsx, a route that is not looked up at all. " +
      "The pointer is on src/summary/Summary.tsx:42:7, which is underlined from its first character to the 7, not the full stop.",
    ...PATH_LINK_SCENARIO,
    steps: [(ui) => ui.press(ui.session("Fix the summary layout")), (ui) => ui.wait(800), (ui) => ui.hoverTerminalUrl("src/summary/Summary.tsx:42:7")],
  },
  {
    id: "terminal-path-link-code",
    group: TERMINAL,
    category: "normal",
    title: "A path with a line opens its code file in the viewer",
    description:
      "Clicking src/summary/Summary.tsx:42:7 opened Summary.tsx in the file viewer over the window, with the terminal left as it was behind it: line 42 is marked and scrolled into view, and the column is ignored. " +
      "There are no Previous and Next buttons. At the far right of the header row, after the copy and wrap buttons, are File manager and Default application.",
    ...PATH_LINK_SCENARIO,
    steps: [(ui) => ui.press(ui.session("Fix the summary layout")), (ui) => ui.wait(800), (ui) => ui.clickTerminalLink("src/summary/Summary.tsx:42:7")],
  },
  {
    id: "terminal-path-link-markdown",
    group: TERMINAL,
    category: "normal",
    title: "A path with a line opens its Markdown file as source",
    description: "Clicking docs/summary-notes.md:13 opened the document in its source view, not rendered, with line 13 marked and scrolled into view.",
    ...PATH_LINK_SCENARIO,
    steps: [(ui) => ui.press(ui.session("Fix the summary layout")), (ui) => ui.wait(800), (ui) => ui.clickTerminalLink("docs/summary-notes.md:13")],
  },
  {
    id: "terminal-path-link-worktree",
    group: TERMINAL,
    category: "normal",
    title: "A path inside a side worktree opens in the viewer",
    description:
      "Clicking the side worktree's src/ui.ts:7 opened a file that exists only there, with line 7 marked: its first line says it is the experimental layout in the side worktree. " +
      "The project's own folder has no src/ui.ts.",
    ...PATH_LINK_SCENARIO,
    steps: [(ui) => ui.press(ui.session("Fix the summary layout")), (ui) => ui.wait(800), (ui) => ui.clickTerminalLink("src/ui.ts:7")],
  },
  {
    id: "terminal-path-link-directory",
    group: TERMINAL,
    category: "normal",
    title: "A folder with a README opens the README in the viewer",
    description:
      "Clicking docs/plans/summary-layout, a folder, opened its README.md in the file viewer rather than the file manager: the plan's heading and milestones, rendered as a document. " +
      "Clicking the same link with Command (Control off macOS) held opens the folder on the host instead, which the gallery cannot show.",
    ...PATH_LINK_SCENARIO,
    steps: [(ui) => ui.press(ui.session("Fix the summary layout")), (ui) => ui.wait(800), (ui) => ui.clickTerminalLink("docs/plans/summary-layout")],
  },
  {
    id: "terminal-path-link-launchable",
    group: TERMINAL,
    category: "normal",
    title: "A program a click could launch opens in the viewer, not on the host",
    description:
      "Clicking bin/helper, a built program of no type the viewer can show, opened the viewer on it all the same, with a \"This file cannot be displayed\" panel and its size in place of any content. " +
      "Nothing is launched on the host and no other window opens.",
    ...PATH_LINK_SCENARIO,
    steps: [(ui) => ui.press(ui.session("Fix the summary layout")), (ui) => ui.wait(800), (ui) => ui.clickTerminalLink("bin/helper")],
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
