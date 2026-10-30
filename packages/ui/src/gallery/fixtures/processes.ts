import type { FixtureError } from "./projectFiles";
import type { Ui } from "../interact";
import type { ProcessTree } from "../../protocol";
import type { Preferences, Scenario, Step } from "../scenario";
import { SAMPLE, keptOf, leftoverOf, processTreeOf, sessionOf, snapshotState } from "./builders";

const { console: console_, web } = SAMPLE;
const GROUP = "Processes";

const preview = sessionOf("s-proc-preview", console_.id, web.id, "Serve the preview", "idle");
const summary = sessionOf("s-proc-summary", console_.id, web.id, "Fix the summary layout", "working");
const state = snapshotState({ consoles: [console_], projects: [web], sessions: [preview, summary] });

const WEBSITE = "/Users/dev/code/website";
const VITE = `node ${WEBSITE}/node_modules/.bin/vite --host 0.0.0.0 --port 5173 --config ${WEBSITE}/vite.config.ts --mode development --clearScreen false --logLevel info`;

const panelPreferences: Preferences = { panelStacks: { project: { open: [], shares: {} }, report: { open: [], shares: {} } } };
const openSession: Step[] = [
  (ui) => ui.press(ui.session(preview.title)),
  (ui) => ui.press(ui.t("titleBar.processes.show")),
];
const openProject: Step[] = [
  (ui) => ui.press(ui.t("sidebar.project.actions", { name: web.name })),
  (ui) => ui.press(ui.t("sidebar.project.openPanel")),
  (ui) => ui.wait(300),
  (ui) => ui.press(ui.t("panel.processes.name")),
  (ui) => ui.wait(300),
];

// A session's trees, one of each kind a row has to draw: the long command with two ports and several processes, the
// newest one, a command the daemon could not read in a folder it could not read either, and a folder since deleted.
const sessionTrees = [
  processTreeOf("100:1", preview, VITE, 95, { process_count: 6, memory_bytes: 412_000_000, ports: [5173, 24678] }),
  processTreeOf("101:1", preview, "pnpm test --watch", 0.2, { process_count: 3, memory_bytes: 120_000_000 }),
  processTreeOf("102:1", preview, "python3 -m http.server 8000", 14, { cwd: `${WEBSITE}/dist`, memory_bytes: 18_000_000, ports: [8000] }),
  processTreeOf("103:1", preview, "", 3, { cwd: null, memory_bytes: 2_000_000 }),
  processTreeOf("104:1", preview, "tail -f logs/dev.log", 130, { cwd: `${WEBSITE}/.worktrees/old-nav`, cwd_missing: true, memory_bytes: 1_000_000 }),
];

// Two trees are enough for the views of one button at work.
const pair = sessionTrees.slice(0, 2);

// A row's buttons are named with its command, so a step names the row it presses in. Both sit in the row whatever
// the pointer does (hidden until the row is hovered or focused, then shown), so a press needs no hover first.
const stopOf = (ui: Ui, tree: ProcessTree) => ui.t("processes.stopNamed", { command: tree.command });
const keepOf = (ui: Ui, tree: ProcessTree) => ui.t("processes.keepNamed", { command: tree.command });

const projectTrees = [
  processTreeOf("200:1", preview, VITE, 95, { process_count: 6, memory_bytes: 412_000_000, ports: [5173, 24678] }),
  processTreeOf("201:1", preview, "python3 -m http.server 8000", 14, { cwd: `${WEBSITE}/dist`, memory_bytes: 18_000_000, ports: [8000] }),
  processTreeOf("202:1", summary, "node scripts/build-summary.mjs --watch", 40, { process_count: 2, memory_bytes: 96_000_000 }),
];
// What no session owns: one tree the user kept past its session, beside two left in a directory since deleted.
const projectProcesses = [
  keptOf("302:1", "node scripts/serve-docs.mjs --port 4000", 300, { memory_bytes: 64_000_000, ports: [4000] }),
  leftoverOf("300:1", `node ${WEBSITE}/.worktrees/old-nav/node_modules/.bin/vite --port 5180`, 600, { cwd: `${WEBSITE}/.worktrees/old-nav`, process_count: 4, memory_bytes: 230_000_000, ports: [5180] }),
  leftoverOf("301:1", "esbuild --service=0.21.5 --ping", 610, { cwd: `${WEBSITE}/.worktrees/old-nav`, memory_bytes: 24_000_000 }),
];

const listFailed: FixtureError = { code: "internal_error", params: { detail: "the process table could not be read" }, message: "the process table could not be read" };
const stopRefused: FixtureError = { code: "internal_error", params: { detail: "the process ledger could not be updated" }, message: "the process ledger could not be updated" };
const keepRefused: FixtureError = { code: "internal_error", params: { detail: "the process ledger could not be kept" }, message: "the process ledger could not be kept" };

// The project's sessions again with one archived beside them, which the session filter must not offer.
const archivedPreview = sessionOf("s-proc-archived", console_.id, web.id, "Migrate the build to Vite", "archived");
const filterState = snapshotState({ consoles: [console_], projects: [web], sessions: [preview, summary, archivedPreview] });

export const processScenarios: Scenario[] = [
  {
    id: "processes-session",
    group: GROUP,
    category: "normal",
    title: "Processes of a session, one row of each kind",
    description: "The native right panel is open beside the selected session. Its session filter, a picker with no label, follows Serve the preview; below the general message it lists that session's processes, oldest first, each in a white card whose leading success-green Play icon stays still. At the end of the command's line, hovering the row or moving keyboard focus into it reveals a danger-soft Stop and a secondary Keep button without moving the command; touch devices show both directly. The directory and the facts line below take the card's whole width, with or without the buttons. The top-bar Processes toggle uses a SquareActivity glyph; the leading command control lines up with the folder icon before its working directory. The Vite command is longer than the row: it fades at the right edge and its tooltip (hover it) shows it whole; under it come the working directory and one line of facts: memory, the two ports it listens on (\"5173 and 24678\"), its age and, for a tree of several, the number of processes. The newest row reads \"started just now\". The row with no readable command says Unknown command over Working directory unknown, which starts in the text column, level with the directories above it, and the row whose directory was deleted carries a soft warning badge Directory deleted after the path.",
    state,
    preferences: panelPreferences,
    processes: { trees: sessionTrees },
    steps: openSession,
  },
  {
    id: "processes-with-files-search",
    group: GROUP,
    category: "normal",
    title: "Processes panel under the Files search field",
    description: "In a docked window Files and Processes are stacked in the right column (a narrower window floats one panel at a time, so only Files is on screen). Files' search field holds a typed query with its clear (×) button; the Processes card below carries its session filter, Serve the preview picked and ending in its clear (×) button, in the same strip: the same inset, groove below and field height as the search field above. Both clear buttons use the default foreground colour, not a muted one. Processes lists that session's rows, whose buttons are on the command's line alone.",
    state,
    preferences: { panelStacks: { project: { open: ["files", "processes"], shares: {} }, report: { open: [], shares: {} } } },
    processes: { trees: pair },
    steps: [
      (ui) => ui.press(ui.session(preview.title)),
      (ui) => ui.showPanel("files"),
      (ui) => ui.press(ui.t("browser.filter.show")),
      (ui) => ui.type("server"),
      (ui) => ui.wait(400),
    ],
  },
  {
    id: "processes-session-empty",
    group: GROUP,
    category: "normal",
    title: "Processes of a session with none running",
    description: "The empty panel is centred horizontally and vertically in the space below the session filter. It carries the SquareActivity glyph, reads No processes and says nothing the session started is still running, with no automatic-stop note above it.",
    state,
    preferences: panelPreferences,
    steps: openSession,
  },
  {
    id: "processes-session-loading",
    group: GROUP,
    category: "normal",
    title: "Processes of a session, list not yet answered",
    description: "Until the first answer the body is the single muted line Loading processes…, with no empty panel or automatic-stop note in its place.",
    state,
    preferences: panelPreferences,
    processes: { list: "pending" },
    steps: openSession,
  },
  {
    id: "processes-project",
    group: GROUP,
    category: "normal",
    title: "Processes of a project, by session and project processes",
    description: "No session is selected, so the native right panel's session filter is empty and shows its placeholder All sessions, with the general message below it. The trees are grouped under the title of the session that started them (Serve the preview with two, Fix the summary layout with one), each group's rows oldest first. The last group, Project processes, carries the note that these processes belong to no session, are never stopped automatically and only you can stop them; it holds a kept process beside two whose working directory was deleted (the Directory deleted badge), oldest first, and its rows offer Stop alone, with no Keep, while the rows of the sessions' groups offer both.",
    state,
    preferences: panelPreferences,
    processes: { trees: projectTrees, projectProcesses: { [web.id]: projectProcesses } },
    steps: openProject,
  },
  {
    id: "processes-project-left-behind",
    group: GROUP,
    category: "normal",
    title: "Processes of a project, only project processes",
    description: "No running session of the project has a process, so Project processes is the only group, with no session heading above it. Its note says they are never stopped automatically, which is what the general message above it has to hold for them too.",
    state,
    preferences: panelPreferences,
    processes: { projectProcesses: { [web.id]: projectProcesses } },
    steps: openProject,
  },
  {
    id: "processes-project-empty",
    group: GROUP,
    category: "normal",
    title: "Processes of a project with none running and no project processes",
    description: "The empty panel is centred horizontally and vertically in the space below the session filter. It carries the SquareActivity glyph and No processes heading, with a concise description saying no session or project processes are running. No automatic-stop note appears above it.",
    state,
    preferences: panelPreferences,
    steps: openProject,
  },
  {
    id: "processes-console-session",
    group: GROUP,
    category: "normal",
    title: "Processes of a console session, without a session filter",
    description: "The console session's own Processes panel lists only what it started. There is no session filter, Files panel or project shell Terminal toggle; Report and Processes are the available panel toggles. Its row offers Stop alone, with no Keep, since nothing a console session started outlives it.",
    state: snapshotState({ consoles: [console_], projects: [web], sessions: [SAMPLE.consoleSession] }),
    preferences: panelPreferences,
    processes: { trees: [processTreeOf("400:1", SAMPLE.consoleSession, "caddy run --config /Users/dev/Caddyfile --watch", 180, { cwd: "/Users/dev", ports: [2019, 8443] })] },
    steps: [
      (ui) => ui.press(ui.session(SAMPLE.consoleSession.title)),
      (ui) => ui.press(ui.t("titleBar.processes.show")),
    ],
  },
  {
    id: "processes-session-filter",
    group: GROUP,
    category: "normal",
    title: "Processes session filter, choices open",
    description: "The session filter's list is open and holds only the two running project sessions; the project's archived session, Migrate the build to Vite, is not offered, and there is no All sessions item. Serve the preview is selected, following the sidebar selection. Choosing the other session changes the process list without selecting that session in the sidebar; clearing the filter (the clear button inside the picker) shows every session's processes.",
    state: filterState,
    preferences: panelPreferences,
    processes: { trees: projectTrees, projectProcesses: { [web.id]: projectProcesses } },
    steps: [...openSession, (ui) => ui.press(ui.t("processes.sessionFilter"))],
  },
  {
    id: "processes-project-menu-session-selected",
    group: GROUP,
    category: "normal",
    title: "Processes opened from the project menu with a session selected",
    description: "Serve the preview stays selected in the sidebar and its conversation remains in the centre, yet the project menu's Open panel, Processes opens the panel for the whole project: the session filter is empty and shows its All sessions placeholder, and the list holds both sessions' processes and Project processes. Refreshes keep this empty filter. Select Fix the summary layout in the sidebar to see the filter follow that newly selected session.",
    state,
    preferences: panelPreferences,
    processes: { trees: projectTrees, projectProcesses: { [web.id]: projectProcesses } },
    steps: [(ui) => ui.press(ui.session(preview.title)), ...openProject],
  },
  {
    id: "processes-phone-empty",
    group: GROUP,
    category: "normal",
    title: "Empty Processes panel manually opened on a phone",
    description: "At phone width the explicit top-bar toggle opens the empty Processes card over the conversation. It keeps its session filter and centres the No processes message in the remaining space, with no automatic-stop note, even though gestures skip this confirmed-empty panel and a shell Terminal with no tabs. Close this card and swipe towards the start: Files is reachable, and further swipes stay on Files while those other panels are empty.",
    width: 390,
    state,
    preferences: panelPreferences,
    steps: [
      (ui) => ui.press(ui.t("titleBar.sidebar.show")),
      ...openSession,
    ],
  },
  {
    id: "processes-phone",
    group: GROUP,
    category: "normal",
    title: "Processes panel reached by swiping on a phone",
    description: "At phone width the Processes card floats over the conversation, with its session filter and process rows fitting within the card. On a touch device each row's Stop and Keep buttons stay in view beside the command even at this narrow width, while the directory and facts lines below keep the card's whole width; with a mouse, hover or focus a row to reveal them. A swipe towards the start from Files reaches this nonempty panel; swiping again cycles to Files because Report has no pages and the shell Terminal has no tabs. Loading or failed process lists also remain reachable by swiping.",
    width: 390,
    state,
    preferences: panelPreferences,
    processes: { trees: pair },
    steps: [
      (ui) => ui.press(ui.t("titleBar.sidebar.show")),
      (ui) => ui.press(ui.session(preview.title)),
      (ui) => ui.swipe("start"),
      (ui) => ui.swipe("start"),
    ],
  },
  {
    id: "processes-stop-focused",
    group: GROUP,
    category: "normal",
    title: "Stop and Keep revealed by keyboard focus",
    description: "Keyboard focus on the first row's Stop button reveals the end of the command's line: a danger-soft Stop and a secondary Keep, with the focus ring on Stop. The command keeps its starting position, the directory and facts lines below keep the card's whole width, the leading Play icon stays as it was, and the other row shows no buttons until it is hovered or focused. Hovering a row with the mouse reveals the same pair.",
    state,
    preferences: panelPreferences,
    processes: { trees: pair },
    steps: [...openSession, (ui) => ui.focusControl(stopOf(ui, pair[0]))],
  },
  {
    id: "processes-stop-confirmation",
    group: GROUP,
    category: "normal",
    title: "Stop a process, confirmation before sending",
    description: "Stop opens a warning confirmation over the Processes panel with the ordinary warning icon in its header. It names the process with its whole command, wrapped and never cut, led by a danger-coloured PlayOff glyph instead of the green Play of the process rows, then its working directory and the line of facts, and says the process and its child processes are stopped. Cancel sends nothing; the confirming Stop sends the request.",
    state,
    preferences: panelPreferences,
    processes: { trees: pair },
    steps: [...openSession, (ui) => ui.press(stopOf(ui, pair[0]))],
  },
  {
    id: "processes-keep-confirmation",
    group: GROUP,
    category: "normal",
    title: "Keep a process, confirmation before sending",
    description: "Keep opens a confirmation titled Keep process over the Processes panel, in the normal colour with no warning sign. It names the process as Stop's confirmation does, whole command (led by the same green Play as the process rows, not the PlayOff of Stop), working directory and facts, and says the process and its child processes will no longer stop when the session ends and stay listed under the project until you stop them. Cancel sends nothing; the confirming Keep sends the request.",
    state,
    preferences: panelPreferences,
    processes: { trees: pair },
    steps: [...openSession, (ui) => ui.press(keepOf(ui, pair[0]))],
  },
  {
    id: "processes-stop-done",
    group: GROUP,
    category: "normal",
    title: "After a confirmed Stop, focus on the next row",
    description: "The first process was stopped from the confirmation: its row is gone, the toast \"Stopped the process.\" has been raised (it fades after a few seconds), and, because the Stop button held focus as the confirmation opened (opened from the keyboard, or by a pointer press while nothing else held focus), focus has moved to the next row, whose Stop and Keep buttons are revealed with the focus ring on Stop, whatever confirmed (the pointer too), so a keyboard user can carry on down the list. A Stop pressed while another control held focus returns focus there instead, which this scenario does not show.",
    state,
    preferences: panelPreferences,
    processes: { trees: pair },
    steps: [...openSession, (ui) => ui.focusControl(stopOf(ui, pair[0])), (ui) => ui.press(stopOf(ui, pair[0])), (ui) => ui.confirm(), (ui) => ui.wait(400)],
  },
  {
    id: "processes-stop-pending",
    group: GROUP,
    category: "normal",
    title: "Stop in flight",
    description: "The user confirmed stopping the first process and the request is still pending. The confirmation stays open with its confirming button pending and cannot be dismissed (Cancel, Escape and the close button do nothing) until the request settles, while the panel remains behind it.",
    state,
    preferences: panelPreferences,
    processes: { trees: pair, stop: "pending" },
    steps: [...openSession, (ui) => ui.press(stopOf(ui, pair[0])), (ui) => ui.confirm()],
  },
  {
    id: "processes-keep-pending",
    group: GROUP,
    category: "normal",
    title: "Keep in flight",
    description: "The user confirmed keeping the first process and the request is still pending: the Keep process confirmation stays open with its confirming button pending, and cannot be dismissed until the request settles.",
    state,
    preferences: panelPreferences,
    processes: { trees: pair, keep: "pending" },
    steps: [...openSession, (ui) => ui.press(keepOf(ui, pair[0])), (ui) => ui.confirm()],
  },
  {
    id: "processes-list-failed",
    group: GROUP,
    category: "exceptional",
    title: "Processes list could not be read",
    description: "No list has been read yet, so a native pane failure fills the Processes panel below its session filter: the danger icon, title \"Could not list the processes\", an explanation that Octoboard did not get the list, Try again and Details. No extra modal opens. The failed first load waits for a manual retry; the panel remains open.",
    state,
    preferences: panelPreferences,
    processes: { list: listFailed },
    steps: openSession,
  },
  {
    id: "processes-list-refresh-failed",
    group: GROUP,
    category: "exceptional",
    title: "Processes list could not be refreshed",
    description: "The list was read and two rows are shown, but its latest poll failed: a warning Alert titled \"Could not refresh the list\" stands above the rows, saying the processes shown may be out of date, with a Details disclosure for the daemon's own text. The rows stay and keep their buttons. Each poll that fails keeps the Alert; the first that succeeds removes it. The Alert appears a few seconds after the panel opens, at its next poll.",
    state,
    preferences: panelPreferences,
    processes: { trees: pair, listLater: listFailed },
    // The Alert is the answer to the panel's second poll, which comes `POLL_MS` (3 s) after the first.
    steps: [...openSession, (ui) => ui.wait(3600)],
  },
  {
    id: "processes-stop-refused",
    group: GROUP,
    category: "exceptional",
    title: "Stop refused",
    description: "The user confirmed Stop on the first row, and the stop request was refused: an error alert titled \"Could not stop the process\" stands over the stop confirmation. After OK the confirmation remains available for retry or cancellation, with both process rows preserved in the panel behind it.",
    state,
    preferences: panelPreferences,
    processes: { trees: pair, stop: stopRefused },
    steps: [...openSession, (ui) => ui.press(stopOf(ui, pair[0])), (ui) => ui.confirm()],
  },
  {
    id: "processes-keep-refused",
    group: GROUP,
    category: "exceptional",
    title: "Keep refused",
    description: "The user confirmed Keep on the first row, and the request was refused: an error alert titled \"Could not keep the process\" stands over the Keep confirmation. After OK the confirmation remains available for retry or cancellation, with both process rows preserved in the panel behind it.",
    state,
    preferences: panelPreferences,
    processes: { trees: pair, keep: keepRefused },
    steps: [...openSession, (ui) => ui.press(keepOf(ui, pair[0])), (ui) => ui.confirm()],
  },
];
