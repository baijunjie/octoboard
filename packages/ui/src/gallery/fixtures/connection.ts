import type { Step } from "../../fixtures/interact";
import type { Scenario } from "../scenario";
import { consoleOf, projectOf, sessionOf, snapshotState, switchingAccount } from "../../fixtures/builders";
import { PROTOCOL_GENERATION } from "../../protocol";
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
    "/Users/dev/code/website/bin/helper": { view: { path: "bin/helper" } },
    "/Users/dev/code/website/docs/plans/summary-layout": { kind: "directory", view: { path: "docs/plans/summary-layout/README.md" } },
    "/Users/dev/code/search-api-experiments/src/ui.ts": { view: { worktree: "wt-side", path: "src/ui.ts" } },
    "/private/tmp/octoboard/scratchpad/icon/sheet3.png": "file",
  },
  state: connected("open"),
} satisfies Partial<Scenario>;

const STARTUP_AND_CONNECTION = "Startup and connection";
const TERMINAL = "Terminal";

/** Selects the session titled `title`, whose terminal the scenarios look at, then closes the drawer a pick leaves up
 * when the sidebar floats (at 768px and narrower a pick closes it), where it would cover the terminal's left side. */
const pickSessionTitled = (title: string): Step => async (ui) => {
  await ui.press(ui.session(title));
  await ui.closeDrawer();
};
const pickSession = pickSessionTitled("Fix the summary layout");

export const connectionScenarios: Scenario[] = [
  {
    id: "daemon-connecting",
    group: STARTUP_AND_CONNECTION,
    category: "normal",
    title: "Connecting, no snapshot yet",
    state: { connectionState: "connecting" },
  },
  {
    id: "daemon-other-generation",
    group: STARTUP_AND_CONNECTION,
    category: "exceptional",
    title: "Daemon of another protocol generation",
    description: "The whole window is the danger-toned screen, whatever the connection had loaded before, with no connection banner: the title, the line saying the window cannot read the daemon, the remedy for the desktop app, then the report guidance sentence and under it the versions of the window (0.1.0) and the daemon (0.2.0). The primary action is Quit Octoboard, as in the desktop app, with Report issue beside it. A browser offers Reload instead: see the next scenario.",
    updater: { currentVersion: "0.1.0" },
    state: snapshotState({ consoles: [console_], projects: [project], sessions, connectionState: "incompatible", daemon: { version: "0.2.0", generation: PROTOCOL_GENERATION + 1 } }),
  },
  {
    id: "daemon-other-generation-browser",
    group: STARTUP_AND_CONNECTION,
    category: "exceptional",
    title: "Daemon of another protocol generation, in a browser, no hello",
    description: "The same screen as in a browser, which cannot quit or update: the primary action is Reload, with Report issue beside it, and the remedy line is the browser's (reload the page, then update Octoboard so that the page and the daemon are the same version). The window's version is unknown, and so is the daemon's, as for a daemon too old to send a hello.",
    state: snapshotState({ consoles: [console_], projects: [project], sessions, connectionState: "incompatible", daemon: null }),
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
    steps: [pickSession],
  },
  {
    id: "daemon-lost",
    group: STARTUP_AND_CONNECTION,
    category: "exceptional",
    title: "Reconnect budget spent",
    description: "The banner offers Try again.",
    state: connected("closed"),
    steps: [pickSession],
  },
  {
    id: "daemon-exited",
    group: STARTUP_AND_CONNECTION,
    category: "exceptional",
    title: "The daemon exited by itself",
    description: "The error alert dialog, titled \"The daemon exited unexpectedly\", says the daemon runs your sessions and terminals and Octoboard cannot work without it, and to restart Octoboard to continue, the app name a chip; the shell's own text is behind Error details, closed, and the Report issue guidance and button are offered as for any unexpected failure. The window behind is dimmed, with the connection banner of the lost daemon.",
    state: connected("closed"),
    daemonExited: "the daemon process exited with status 101",
    steps: [pickSession],
  },
  {
    id: "daemon-lost-behind-dialog",
    group: STARTUP_AND_CONNECTION,
    category: "exceptional",
    title: "Reconnect budget spent with a dialog open",
    description: "Try again has keyboard focus while the file viewer stays open. F6 moves between the viewer and the banner. Enter retries without dismissing the viewer, and so does a pointer press on Try again. After returning, Tab stays inside the viewer and Escape still closes it. Narrow the window: the viewer ends above the banner with its bottom edge and rounded corners visible.",
    state: connected("open"),
    steps: [
      pickSession,
      (ui) => ui.showPanel("files"),
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
    description: "The right-click menu offers Copy in its own group above Paste and Select all, with Copy disabled and no hint under it, since nothing is selected and the program does not report the mouse. Paste uses a private sample clipboard containing echo hello, and copying updates only that clipboard.",
    clipboard: { text: "echo hello" },
    state: connected("open"),
    steps: [pickSession, (ui) => ui.terminalContextMenu()],
  },
  {
    id: "terminal-edit-menu-mouse-reporting",
    group: TERMINAL,
    category: "normal",
    title: "Terminal edit menu while the program reports the mouse",
    description: "Copy is disabled with nothing selected and nothing the program copied; since a plain drag goes to the program, a hint under Copy says how to select instead (Option-drag on macOS, Shift-drag elsewhere).",
    terminal: "mouse",
    clipboard: { text: "echo hello" },
    state: connected("open"),
    // The menu reads the mouse mode when it opens, and the program turns it on after its first output.
    steps: [pickSession, (ui) => ui.terminalMouseReporting(), (ui) => ui.terminalContextMenu()],
  },
  {
    id: "terminal-edit-menu-selected",
    group: TERMINAL,
    category: "normal",
    title: "Terminal edit menu with selected output",
    description: "Selected terminal output stays highlighted while the menu is open, and Copy is enabled in its own group above Paste and Select all. Copy and Paste share the private sample clipboard.",
    clipboard: { text: "echo hello" },
    state: connected("open"),
    steps: [
      pickSession,
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
    description: "With output selected, Copy is still enabled and Paste is disabled with \"Clipboard unavailable\" under its name: only reading the clipboard needs the platform's clipboard.",
    clipboard: "unavailable",
    state: connected("open"),
    steps: [
      pickSession,
      (ui) => ui.terminalContextMenu(),
      (ui) => ui.press(ui.t("terminal.selectAll")),
      (ui) => ui.terminalContextMenu(),
    ],
  },
  {
    id: "terminal-edit-menu-path-link",
    group: TERMINAL,
    category: "normal",
    title: "Terminal edit menu over a file path",
    description: "The right-click was made on src/summary/Summary.tsx:42:7, so the menu has \"Open in file manager\", with a folder icon, above the separate Copy group, followed by Paste and Select all; the rest is the plain menu. Right-click anywhere else in the output and the item is not there. Choosing it shows the path in the host's file manager, which the gallery cannot show.",
    ...PATH_LINK_SCENARIO,
    clipboard: { text: "echo hello" },
    steps: [pickSession, (ui) => ui.wait(800), (ui) => ui.terminalContextMenu("src/summary/Summary.tsx:42:7")],
  },
  {
    id: "terminal-cursor-selection",
    group: TERMINAL,
    category: "normal",
    title: "Block cursor beside reversed text",
    description: "Only the first two Chinese characters are reversed. The focused terminal's gray block cursor covers the third character, whose glyph stays black, so the cursor is distinct from the reversed text. Switch themes to compare both backgrounds; click a control outside the terminal to see the inactive cursor outline, then click the terminal to restore its block. The program requests a steady block so the comparison stays visible.",
    terminal: "cursor-selection",
    preferences: { panelStacks: { project: { open: [], shares: {} }, report: { open: [], shares: {} } } },
    state: snapshotState({ consoles: [console_], projects: [project], sessions: sessions.map((session) => session.id === "s-working" ? { ...session, agent: "codex" as const } : session) }),
    steps: [pickSession, (ui) => ui.focusTerminal()],
  },
  {
    id: "terminal-fullscreen-links",
    group: TERMINAL,
    category: "normal",
    title: "Links in a full-screen terminal response",
    description: "The localhost address and the labelled \"layout notes\" hyperlink are in the accent text colour without hovering, even inside muted output, while the prose around them stays muted. Clicking opens their addresses.",
    terminal: "fullscreen",
    preferences: { panelStacks: { project: { open: [], shares: {} }, report: { open: [], shares: {} } } },
    state: connected("open"),
    steps: [pickSession, (ui) => ui.wait(400)],
  },
  {
    id: "terminal-link-tooltip",
    group: TERMINAL,
    category: "normal",
    title: "Hovering a terminal hyperlink",
    description: "The pointer is on the \"layout notes\" hyperlink: a tooltip names the link's real address, which its text does not say, and the address reads left to right in Arabic too.",
    state: connected("open"),
    steps: [pickSession, (ui) => ui.wait(400), (ui) => ui.hoverTerminalLink("layout notes")],
  },
  {
    id: "terminal-status-touch-popover",
    group: TERMINAL,
    category: "normal",
    title: "Tapping the top bar's status glyph",
    description: "The glyph at the end of the top bar's breadcrumb has been tapped with a finger, so its text, Working, stays open in a popover drawn like the tooltip, above it and with no tooltip on top of it; unlike the tooltip, it stays until Escape or an outside tap.",
    state: connected("open"),
    steps: [pickSession, (ui) => ui.tapTopBarStatus()],
  },
  {
    id: "terminal-broken-url-hover",
    group: TERMINAL,
    category: "normal",
    title: "Hovering a URL the program broke across rows",
    description: "The pointer is on the last row of an address the program broke at the right edge, so the first piece fills the row: the whole address, on every row it spans, is underlined, and clicking opens the full address. The window's width decides where it breaks.",
    state: connected("open"),
    steps: [pickSession, (ui) => ui.wait(400), (ui) => ui.hoverTerminalUrl("lang=zh-Hans")],
  },
  {
    id: "terminal-broken-url-short-hover",
    group: TERMINAL,
    category: "normal",
    title: "Hovering a URL broken short of the right edge",
    description: "The pointer is on the last row of the address that stops short of the right edge, so blank cells remain after its first piece. The address above it still fills its row. The link colour covers only the address text, not those blank cells. The underline runs across every row this address spans: on the first row it continues through the blank cells to the right edge, and on the last row it includes the indent and stops at the end of the address. Clicking opens the full address.",
    state: connected("open"),
    steps: [pickSession, (ui) => ui.wait(400), (ui) => ui.hoverTerminalUrl("account&theme=light")],
  },
  {
    id: "terminal-path-links",
    group: TERMINAL,
    category: "normal",
    title: "File paths in terminal output",
    description:
      "The paths the daemon finds are in the same accent text colour as the address on the last line, without hovering: the one in Read(...), the absolute project path, src/summary/Summary.tsx:42:7, docs/summary-notes.md:13, the side worktree's src/ui.ts:7, bin/helper, the folder docs/plans/summary-layout, the temp-folder path and the one after @, where the colour starts at src, not at the @. " +
      "The closing parenthesis, the full stop after :42:7, the commas after :13 and after the temp path and the semicolon after the @ mention stay in the output's own colour. " +
      "Three paths stay plain: src/summary/Draft.ts, which does not exist, src/shared/index.ts, which the daemon answers none for as it would one that more than one of the project's folders holds, and app/[id]/page.tsx, a route that is not looked up at all. " +
      "The pointer is on src/summary/Summary.tsx:42:7, which is underlined from its first character to the 7, not the full stop.",
    ...PATH_LINK_SCENARIO,
    steps: [pickSession, (ui) => ui.wait(800), (ui) => ui.hoverTerminalUrl("src/summary/Summary.tsx:42:7")],
  },
  {
    id: "terminal-path-link-code",
    group: TERMINAL,
    category: "normal",
    title: "A path with a line opens its code file in the viewer",
    description:
      "Clicking src/summary/Summary.tsx:42:7 opened Summary.tsx in the file viewer over the window, with the terminal left as it was behind it: line 42 is marked and scrolled into view, and the column is ignored. " +
      "There are no Previous and Next buttons. At the far end of the header row, after the copy and wrap buttons, are File manager and Default application. A Markdown file opens in its source view the same way, and so does a file that exists only in a side worktree.",
    ...PATH_LINK_SCENARIO,
    steps: [pickSession, (ui) => ui.wait(800), (ui) => ui.clickTerminalLink("src/summary/Summary.tsx:42:7")],
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
    steps: [pickSession, (ui) => ui.wait(800), (ui) => ui.clickTerminalLink("docs/plans/summary-layout")],
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
    steps: [pickSession, (ui) => ui.wait(800), (ui) => ui.clickTerminalLink("bin/helper")],
  },
  {
    id: "terminal-starting",
    group: TERMINAL,
    category: "normal",
    title: "Terminal of a session still starting",
    description: "The selected session's agent is still being launched: the terminal is covered by a spinner and \"Starting session…\", with no Reconnect button. Its sidebar row shows the muted turning arc.",
    state: snapshotState({ consoles: [console_], projects: [project], sessions: [...sessions, starting] }),
    steps: [pickSessionTitled(starting.title)],
  },
  {
    id: "terminal-switching-account",
    group: TERMINAL,
    category: "normal",
    title: "Terminal of a session switching its account",
    description: "The selected session's account switch is running, as the daemon reports it: its process is down meanwhile, so the session reads interrupted, yet the terminal is covered by a spinner and \"Switching account\" and takes no input, with no \"Not running\" bar or Resume offered. In the sidebar row and the title bar's breadcrumb the interrupted pause sign gives way to the muted turning arc, whose tooltip reads \"Switching account\"; Hub, in the same batch, shows it too.",
    state: switchingAccount(
      snapshotState({
        consoles: [console_],
        projects: [project],
        sessions: sessions.map((session) => (session.id === "s-console" ? { ...session, status: "interrupted" as const } : session)),
      }),
      ["s-interrupted", "s-console"],
    ),
    steps: [pickSessionTitled("Interrupted by a restart")],
  },
  {
    id: "terminal-path-link-removed",
    group: TERMINAL,
    category: "exceptional",
    title: "A project folder removed since the link was drawn",
    description:
      "Clicking docs/plans/summary-layout, a folder the daemon found when the output was drawn but which is gone now, opened the error alert instead of the file manager, titled \"Could not open the path\" and saying docs/plans/summary-layout does not exist any more. " +
      "Its remedy says it may have been moved, renamed or deleted, and to find it in the project's files and try again from there; it does not offer Browse. A folder refused for being in more than one worktree, or one others can change, uses the same alert with its own sentence and remedy. A file of the project that is gone opens in the viewer, which says so in its own pane.",
    ...PATH_LINK_SCENARIO,
    pathLinks: { ...PATH_LINK_SCENARIO.pathLinks, "/Users/dev/code/website/docs/plans/summary-layout": { kind: "directory", gone: true } },
    steps: [pickSession, (ui) => ui.wait(800), (ui) => ui.clickTerminalLink("docs/plans/summary-layout")],
  },
  {
    id: "terminal-path-link-removed-outside",
    group: TERMINAL,
    category: "exceptional",
    title: "A path outside the project removed since the link was drawn",
    description:
      "Clicking /private/tmp/octoboard/scratchpad/icon/sheet3.png, a temporary file the daemon found when the output was drawn but which is gone now, opened the error alert instead of launching anything, titled \"Could not open the path\" and saying the path does not exist any more. " +
      "It has no remedy line: unlike a project path, there are no project files to find it in. A path that is neither a file nor a folder, or that leads outside the project and the temporary folders, reads alike.",
    ...PATH_LINK_SCENARIO,
    pathLinks: { ...PATH_LINK_SCENARIO.pathLinks, "/private/tmp/octoboard/scratchpad/icon/sheet3.png": { kind: "file", gone: true } },
    steps: [pickSession, (ui) => ui.wait(800), (ui) => ui.clickTerminalLink("/private/tmp/octoboard/scratchpad/icon/sheet3.png")],
  },
  {
    id: "terminal-path-link-refused-unreadable",
    group: TERMINAL,
    category: "exceptional",
    title: "A temporary path that could not be read",
    description:
      "Clicking /private/tmp/octoboard/scratchpad/icon/sheet3.png, which was a file when the output was drawn but which the system fails to look at now, opened the error alert instead of opening anything, titled \"Could not open the path\" and saying it could not be read, so it is not opened. " +
      "It has no remedy line. The system's own text, Input/output error (os error 5), is only in the Error details disclosure, closed here.",
    ...PATH_LINK_SCENARIO,
    pathLinks: { ...PATH_LINK_SCENARIO.pathLinks, "/private/tmp/octoboard/scratchpad/icon/sheet3.png": { kind: "file", refused: "unreadable", detail: "Input/output error (os error 5)" } },
    steps: [pickSession, (ui) => ui.wait(800), (ui) => ui.clickTerminalLink("/private/tmp/octoboard/scratchpad/icon/sheet3.png")],
  },
  {
    id: "terminal-reconnecting",
    group: TERMINAL,
    category: "exceptional",
    title: "Terminal reconnecting",
    description: "The session is live but its terminal socket never connects: after the first failed attempt the terminal stays covered by the spinner and \"Reconnecting the terminal…\".",
    terminal: "refuse",
    state: connected("open"),
    steps: [pickSession, (ui) => ui.wait(1500)],
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
    steps: [pickSession, (ui) => ui.wait(2000)],
  },
];
