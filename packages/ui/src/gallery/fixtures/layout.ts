import { describing, type Matcher } from "../interact";
import type { Scenario } from "../scenario";
import { consoleOf, pageOf, projectOf, sessionOf, shellTerminalOf, snapshotState } from "./builders";

const GROUP = "Window layout";

// `railState` has five consoles covering everything the rail shows per console: a raised hand (Main,
// which has two waiting sessions there, and the one with the long name, which has one, so the waiting
// count reads 3 across both), work in progress (Operations), a session merely running (Billing), and
// nothing going on (Docs). Main and Website are also borrowed by `layout-waiting-overflow`, which
// gives them sessions of its own.
const main = consoleOf("c-main", "Main");
const operations = consoleOf("c-ops", "Operations");
const billing = consoleOf("c-billing", "Billing");
const docs = consoleOf("c-docs", "Docs");
const lab = consoleOf("c-lab", "A console with a name that is far too long for the rail");
const website = projectOf("p-web", main.id, "Website");
const deploys = projectOf("p-deploys", operations.id, "Deploys");
const invoices = projectOf("p-invoices", billing.id, "Invoices");
const manual = projectOf("p-manual", docs.id, "User manual");
const experiments = projectOf("p-experiments", lab.id, "Experiments");
const hub = sessionOf("s-hub", main.id, undefined, "Hub", "waiting_user", { colour: "teal" });
const railState = snapshotState({
  consoles: [main, operations, billing, docs, lab],
  projects: [website, deploys, invoices, manual, experiments],
  sessions: [
    hub,
    sessionOf("s-web-1", main.id, website.id, "Fix the summary layout", "working"),
    sessionOf("s-web-2", main.id, website.id, "Update the dependencies", "idle"),
    sessionOf("s-web-3", main.id, website.id, "Asks which region to use", "waiting_user"),
    sessionOf("s-ops-hub", operations.id, undefined, "Operations hub", "idle", { colour: "cyan" }),
    sessionOf("s-ops-1", operations.id, deploys.id, "Roll back the last deploy", "working"),
    sessionOf("s-bill-1", billing.id, invoices.id, "Reconcile March", "idle"),
    sessionOf("s-docs-1", docs.id, manual.id, "Old draft", "archived"),
    sessionOf("s-lab-1", lab.id, experiments.id, "Asks which branch to use", "waiting_user"),
  ],
  pages: { [hub.id]: [pageOf("pg-hub", hub.id, "<h1>Report</h1><p>Two sessions need you.</p>", 5)] },
});
// More consoles than the rail holds in any window, so its column of them scrolls whatever the
// window's height: forty tiles are taller than a browser window the gallery is opened in.
const crowd = Array.from({ length: 40 }, (_, i) => consoleOf(`c-crowd-${i}`, `Console ${i + 1}`));
const crowdState = snapshotState({
  consoles: crowd,
  projects: [projectOf("p-crowd", crowd[0].id, "Website")],
  sessions: [
    sessionOf("s-crowd-hub", crowd[0].id, undefined, "Hub", "idle", { colour: "teal" }),
    sessionOf("s-crowd-1", crowd[0].id, "p-crowd", "Fix the summary layout", "working"),
  ],
});
// What the quit confirmation is about: sessions that are idle hold a quit up no more than an idle
// shell does, so `quitIdleState` is held up by its busy shell alone, and `quitBusyState` by that and a
// working session.
const quitShells = [shellTerminalOf("sh-idle", website, "zsh", 0), shellTerminalOf("sh-dev", website, "pnpm dev", 1, { busy: true })];
const quitIdleState = snapshotState({
  consoles: [main],
  projects: [website],
  sessions: [sessionOf("s-web-2", main.id, website.id, "Update the dependencies", "idle")],
  shellTerminals: quitShells,
});
const quitQuietState = snapshotState({
  consoles: [main],
  projects: [website],
  sessions: [sessionOf("s-web-2", main.id, website.id, "Update the dependencies", "idle")],
});
const quitBusyState = snapshotState({
  consoles: [main],
  projects: [website],
  sessions: [sessionOf("s-web-1", main.id, website.id, "Fix the summary layout", "working"), sessionOf("s-web-2", main.id, website.id, "Update the dependencies", "idle")],
  shellTerminals: quitShells,
});
/** The rail's button for a console: its accessible name is the console's whole name, alone or
 * followed by punctuation and what is going on in it, whichever language writes that. A name
 * followed by a letter or a space is another control's ("Operations hub" is a session, not the
 * console "Operations"); one followed by punctuation of its own is not told apart. */
const avatar = (name: string): Matcher =>
  describing(`avatar(${name})`, (label) => label === name || (label.startsWith(name) && /^\p{P}/u.test(label.slice(name.length))));

export const layoutScenarios: Scenario[] = [
  {
    id: "layout-rail-consoles",
    group: GROUP,
    category: "normal",
    title: "Rail with five consoles",
    description:
      "Waiting, working, running and inactive avatars appear together, including a long name. Hover for activity details. Delete the inactive Docs console from its context menu: its avatar fades out while sliding to the right while the remaining avatars slide into place. Use the rail’s New console button to add an avatar and inspect its entrance.",
    state: railState,
    steps: [
      (ui) => ui.press(ui.session("Hub")),
    ],
  },
  {
    id: "layout-rail-crowded",
    group: GROUP,
    category: "normal",
    title: "More consoles than the rail holds",
    description:
      "More consoles than the column holds, which scrolls with no scrollbar to say so: the avatars fade at the bottom edge, where more are waiting. Scroll it and both edges fade until the end, where that edge stops. Rail with five consoles shows the same column with everything in view, and no fade.",
    state: crowdState,
  },
  {
    id: "layout-waiting-overflow",
    group: GROUP,
    category: "normal",
    title: "More than 99 raised hands",
    description:
      "The rail's waiting count at its limit: a hundred and five sessions waiting in one console, so the count shows its overflow label and the glyph is cut out around the wider pill.",
    state: snapshotState({
      consoles: [main],
      projects: [website],
      sessions: Array.from({ length: 105 }, (_, i) =>
        sessionOf(`s-${i}`, main.id, website.id, `Needs an answer ${i + 1}`, "waiting_user"),
      ),
    }),
  },
  {
    id: "layout-history",
    group: GROUP,
    category: "normal",
    title: "Back and Forward available",
    state: railState,
    steps: [
      (ui) => ui.press(ui.session("Hub")),
      (ui) => ui.press(ui.session("Fix the summary layout")),
      (ui) => ui.press(avatar(operations.name)),
      (ui) => ui.press(ui.session("Roll back the last deploy")),
      (ui) => ui.press(ui.t("titleBar.back")),
    ],
  },
  {
    id: "layout-sidebar-hidden",
    group: GROUP,
    category: "normal",
    title: "Hidden docked sidebar",
    state: railState,
    preferences: { sidebarVisible: false, sidebarConsole: main.id },
    description: "The breadcrumb remains while the sidebar is hidden. From fully closed, hover the sidebar toggle, then Main's avatar and another console's avatar: each inset preview fades in while the whole card rises by 4% of its own height over 240ms, shows its console and fades out as it sinks back, so let it fully close before comparing the next trigger. A preview card's header line runs level with the docked report header's. Press the toggle to compare the docked column's 200ms opening and closing with the report pane.",
    steps: [
      (ui) => ui.press(ui.session("Hub")),
    ],
  },
  {
    id: "layout-sidebar-float-beside-pane",
    group: GROUP,
    category: "normal",
    title: "Floating sidebar card beside the docked Files pane",
    state: railState,
    preferences: { sidebarVisible: false, sidebarConsole: main.id },
    description: "The hidden docked sidebar floated in by hovering its toggle, over the terminal, while the Files panel stays docked on the other side, to compare the two cards: both are rounded-xl with the same hairline edge, the same drop shadow and the same 8px gap to the top bar and the bottom (and to the rail or window edge), and their headers are both 40px tall with the same type, padding and bottom line. Any difference in corner radius, edge, shadow or header height between them is a mismatch.",
    steps: [
      (ui) => ui.press(ui.session("Fix the summary layout")),
      (ui) => ui.floatPane(ui.t("titleBar.sidebar.show")),
    ],
  },
  {
    id: "layout-report-hidden",
    group: GROUP,
    category: "normal",
    title: "No report panel open",
    state: railState,
    preferences: { panelStacks: { report: { open: [], shares: {} } } },
    description: "With the report closed there is no pane: the terminal takes the window's full width and the top bar's Report toggle is not pressed. Hover the toggle to float the report in, fading in as the card rises by 4% of its own height like the hidden sidebar's preview, while the pointer rests on it, and press it to dock the report, which appears at once.",
    steps: [
      (ui) => ui.press(ui.session("Hub")),
    ],
  },
  {
    id: "layout-report-floating",
    group: GROUP,
    category: "normal",
    title: "Floating report",
    state: railState,
    preferences: { panelStacks: { report: { open: [], shares: {} } } },
    description: "The report floated in over the terminal by hovering the Report toggle with no panel open, the pointer now resting on it, after fading in as the card rose by 4% of its own height. Only the Report card floats, with its header (the Report name, the page arrows and count, Close panel), rounded corners, hairline edge and the same drop shadow as the sidebar's floating card, 8px from the top bar, the window's end and the bottom; the terminal shows in the gap around it, unobscured. The Report toggle is not pressed: pressing it docks the pane.",
    steps: [
      (ui) => ui.press(ui.session("Hub")),
      (ui) => ui.floatPane(ui.t("titleBar.report.show")),
    ],
  },
  {
    id: "layout-phone-rail-away",
    group: GROUP,
    category: "normal",
    title: "No rail in a phone-width window",
    width: 390,
    state: railState,
    preferences: { sidebarConsole: main.id },
    description: "Below 782px the rail is not in the row: the content panel spans the window's full width under the top bar, and its top corner at the start edge is square, where wider windows round it beside the rail. The sidebar toggle brings the rail in with the drawer (see Mobile sidebar card), and so does a swipe towards the end of the line on a touch screen (a touch-emulating window will do), the card sliding in from the start edge; a swipe towards the start opens the right pane's panels one at a time, in the order of the top bar's toggles (here the project session's Files, then Terminal; Report comes first for a session that has pages), each sliding in from the end edge and the last returning to the first, and one towards the end closes the open panel. The width is fixed because the layout exists only in this range.",
    steps: [
      (ui) => ui.swipe("end"),
      // Picking a session closes the drawer, and the rail goes with it.
      (ui) => ui.press(ui.session("Fix the summary layout")),
    ],
  },
  {
    id: "layout-sidebar-drawer",
    group: GROUP,
    category: "normal",
    title: "Mobile sidebar card",
    width: 390,
    state: railState,
    preferences: { sidebarConsole: main.id },
    description: "The mobile sidebar is one card floating over the dimmed terminal, inset 4px from the window's edges and below the top bar, with the rounded corners, hairline edge and drop shadow of the hidden desktop sidebar's preview. The rail, which is not in the row at this width, is inside it at the start side, transparent over the card, with a single vertical line between it and the sidebar's content; the backdrop covers the whole window around the card. Close and reopen it with the toggle to inspect the card sliding in from the start edge with a fade, the backdrop fading with it; a swipe does the same on a touch screen (a touch-emulating window will do): sideways towards the end of the line opens it, towards the start closes it. The backdrop and Escape also close it; Arabic mirrors the card, which slides in from the right.",
    steps: [
      // The same swipe a finger makes, which the toggle's press would also have done.
      (ui) => ui.swipe("end"),
    ],
  },
  {
    id: "layout-report-docked",
    group: GROUP,
    category: "normal",
    title: "Docked report",
    state: railState,
    description: "A console session's report is a panel of the right pane: one card filling the pane's height, with the Report name, the page arrows and count, and Close panel in its 40px header, rounded corners, a hairline edge and the shadow of the sidebar's floating card. A gap showing the terminal's own background surrounds it: 8px at the window's end, the bottom and below the top bar, and 16px on the terminal side, which holds the width handle; no sidebar-coloured column sits behind it. The top bar has only the Report toggle, pressed: a console session has no Files or Terminal toggle, a terminal needing a project's folder, and none for the pane itself. Press Close panel to close the pane, which goes at once.",
    steps: [
      (ui) => ui.press(ui.session("Hub")),
    ],
  },
  {
    id: "layout-project-pane-narrow",
    group: GROUP,
    category: "normal",
    title: "Files panel in a narrow window",
    width: 800,
    state: railState,
    preferences: { sidebarConsole: main.id },
    description: "Below the docked breakpoint the sessions list is a drawer, closed here, but the right pane still docks down to 782px (below that it floats, see Files panel over the terminal on a phone): its width is clamped to what leaves the terminal its 398px floor, so the Files card sits beside the terminal about 346px wide in this 800px window. It has the same card and header as in a wide window, with the Files and Changes tabs, the filter and refresh buttons and Close panel fitting, and the top bar's Files toggle pressed.",
    steps: [
      (ui) => ui.press(ui.t("titleBar.sidebar.show")),
      (ui) => ui.press(ui.session("Fix the summary layout")),
      (ui) => ui.press(ui.t("titleBar.sidebar.hide")),
    ],
  },
  {
    id: "layout-project-pane-overlay",
    group: GROUP,
    category: "normal",
    title: "Files panel over the terminal on a phone",
    width: 390,
    state: railState,
    preferences: { sidebarConsole: main.id },
    description: "Below 782px the right pane no longer docks beside the terminal: the Files card floats over it, the only panel shown, with a dimmed 48px strip of the terminal beside it, reaching the window's start edge (mirrored under right-to-left) since no rail stands there, that closes it when pressed. The terminal under the card keeps the whole width and cannot be focused. The card fades in as it slides in from the window's end edge (the start edge under right-to-left), the dimmed strip fading with it; on a touch screen a swipe towards the end of the line closes it, and one towards the start switches to the next panel. The top bar's Files toggle is pressed; Close panel, the strip and Escape all close the card, and opening the sessions drawer closes it, as opening the card closes the drawer. The width is fixed because the layout exists only in this range.",
    steps: [
      (ui) => ui.press(ui.t("titleBar.sidebar.show")),
      // Picking a session closes the drawer, which has cleared any floating panel: the toggle opens it.
      (ui) => ui.press(ui.session("Fix the summary layout")),
      (ui) => ui.press(ui.t("titleBar.files.show")),
    ],
  },
  {
    id: "layout-project-pane-floating",
    group: GROUP,
    category: "normal",
    title: "Floating Files panel",
    state: railState,
    preferences: { panelStacks: { project: { open: [], shares: {} } } },
    description: "The Files panel floated in over the agent terminal by hovering the Files toggle with no panel open, the pointer now resting on it, after fading in as the card rose by 4% of its own height. Only the Files card floats, with rounded corners, a hairline edge and the same drop shadow as the sidebar's floating card, 8px from the top bar, the window's end and the bottom; the terminal shows in the gap around it, unobscured. The Files toggle is not pressed: pressing it docks the pane.",
    steps: [
      (ui) => ui.press(ui.session("Fix the summary layout")),
      (ui) => ui.floatPane(ui.t("titleBar.files.show")),
    ],
  },
  {
    id: "layout-quit-terminals",
    group: GROUP,
    category: "normal",
    title: "Quit held up by a terminal",
    description:
      "Quitting while a shell terminal is running something, with every session idle: the dialog is titled \"Quit Octoboard\" (the app name is plain text in the title) and says some terminals are still running something and that quitting ends it. It makes no mention of sessions, which an idle session never holds a quit up for, nor of resuming, which a terminal cannot.",
    state: quitIdleState,
    quitRequested: true,
  },
  {
    id: "layout-quit-sessions-and-terminals",
    group: GROUP,
    category: "normal",
    title: "Quit held up by a session and a terminal",
    description:
      "Quitting with a working session and a busy shell terminal: one dialog says both are still running, that the sessions are interrupted and can each be resumed next time, and that what the terminals run is ended.",
    state: quitBusyState,
    quitRequested: true,
  },
  {
    id: "layout-quit-failed",
    group: GROUP,
    category: "exceptional",
    title: "Quit that did not end the app",
    description:
      "Nothing holds the quit up, so it goes straight to the shell, which refuses: the error alert dialog is titled \"Could not quit\" and says Octoboard kept running instead of ending and to try quitting again; the shell's text is behind Error details, closed, with the Report issue guidance and button of an unexpected failure.",
    state: quitQuietState,
    quitRequested: true,
    exitFailure: "the application is already shutting down",
  },
];
