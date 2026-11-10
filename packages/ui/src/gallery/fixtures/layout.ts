import { describing, type Matcher, type Ui } from "../../fixtures/interact";
import type { Scenario } from "../scenario";
import { consoleOf, pageOf, projectOf, sessionOf, shellTerminalOf, snapshotState } from "../../fixtures/builders";

const GROUP = "Window layout";

// `railState` has five consoles covering the rail's common badges: a raised hand (Main,
// which has two waiting sessions there, and the one with the long name, which has one, so the waiting
// count reads 3 across both), a Done status (Operations, whose hub outranks its working session, and Billing), and
// nothing going on (Docs). Main and Website are also borrowed by `layout-waiting-overflow`, which
// gives them sessions of its own. `collapsed-project-activity` also shows Paused and Awaiting instructions
// and consoles whose other statuses are filtered out of the rail badges.
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
// shell does, so `quitBusyState` is held up by a busy shell and a working session.
const quitShells = [shellTerminalOf("sh-idle", website, "zsh", 0), shellTerminalOf("sh-dev", website, "pnpm dev", 1, { busy: true })];
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
/** The rail names a console on its own or through the translated status sentence. Matching the
 * whole name and its sentence prefix keeps a similarly named session out. */
const avatar = (ui: Ui, name: string): Matcher => {
  const prefix = ui.t("session.status.named", { name, status: "\0" }).split("\0")[0];
  return describing(`avatar(${name})`, (label) => label === name || label.startsWith(prefix));
};

export const layoutScenarios: Scenario[] = [
  {
    id: "layout-rail-consoles",
    group: GROUP,
    category: "normal",
    title: "Rail with five consoles",
    description:
      "Waiting, done and inactive avatars appear together, including a long name; each badge is the console's top status (Operations' Done hub outranks its working session). Hover an avatar for its status. Delete the inactive Docs console from its context menu: its avatar fades out while sliding to the right while the remaining avatars slide into place. Use the rail’s New console button to add an avatar and inspect its entrance.",
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
      "The rail's orange waiting count at its limit: a hundred and five sessions waiting in one console, so the count shows 99+ in a wider pill with the component's default border at the button's outer top-right corner (top-left in right-to-left layouts).",
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
      (ui) => ui.press(avatar(ui, operations.name)),
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
    description: "The breadcrumb remains while the sidebar is hidden. From fully closed, hover the sidebar toggle, then Main's avatar and another console's avatar: each inset preview fades in while the whole card rises by 4% of its own height over 240ms, shows its console and fades out as it sinks back, so let it fully close before comparing the next trigger. Rest on a blank part of the left rail for 200ms to reveal the current console, then move into the card; briefly crossing the rail must leave it closed. Console avatars keep their independent 130ms preview delay. Hovering Usage, Settings, or any other rail button must not open the sidebar, and entering a button or leaving the rail cancels the blank-area delay. Move from the top-bar sidebar toggle through the small gap directly below it into the card: the preview stays visible even if the pointer pauses in that gap. These rail triggers are absent in the single-card mobile layout. A preview card's header line runs level with the docked report header's. Press the toggle to compare the docked column's 200ms opening and closing with the report pane.",
    steps: [
      (ui) => ui.press(ui.session("Hub")),
    ],
  },
  {
    id: "layout-sidebar-float-beside-pane",
    group: GROUP,
    category: "normal",
    title: "Resizable floating sidebar card",
    // Hover reveals the sidebar above 768px; at that width and narrower it is a fixed-width mobile card.
    minWidth: 769,
    state: railState,
    preferences: { sidebarVisible: false, sidebarConsole: main.id },
    description: "The sidebar floats over the terminal, revealed by hovering its toggle. Drag its inner edge to resize it from 260px to 480px; the handle matches the right pane's short rounded grip, centred in the gap outside the card, and keeps the card open while hovering or dragging. Its hit area is 24px wide and its visible bar is 4px by 40px, appearing on hover, keyboard focus and drag. The remembered width is shared with the docked sidebar. Set the width to 800px to inspect the floating drawer, or Desktop (1440px) for the hidden sidebar's hover preview beside the docked Files panel. At Desktop compare the two cards: both are rounded-xl with the same hairline edge, drop shadow, 8px gaps and 40px headers. Dismiss the sidebar and rest on a blank part of its rail for 200ms to reveal it again; crossing the rail briefly or hovering a rail action button must not reveal it. Console avatars retain their independent 130ms preview delay, and moving from the blank rail into the revealed card keeps it visible.",
    steps: [
      (ui) => ui.press(ui.session("Fix the summary layout")),
      (ui) => ui.closeDrawer(),
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
    description: "With the report closed there is no pane: the terminal takes the window's full width and the top bar's Report toggle is not pressed. The agent terminal's character grid has 14px of visible space on each side, with its internal scrollbar included in the physical right side; any fraction left by whole character columns is shared between the two sides. Hovering the panel or group toggles leaves the closed pane hidden, in docked and floating modes alike. Switching the empty group to floating does not preview a card. Only pressing a panel toggle opens it. Pressing the toggle opens the report: it docks when the visible sidebar, its own minimum width and a 400px agent terminal fit, otherwise above 768px it opens in the floating group, and at 768px and narrower (where there is no hover preview) it floats alone as one card.",
    steps: [
      (ui) => ui.press(ui.session("Hub")),
    ],
  },
  {
    id: "layout-report-floating",
    group: GROUP,
    category: "normal",
    title: "Floating report",
    // Hover reveals a pane above 768px; at that width and narrower only a press opens a single mobile card.
    minWidth: 769,
    state: railState,
    preferences: { asideDocked: false, asideWidth: 1000, panelStacks: { report: { open: ["report"], shares: {} } } },
    description: "The open Report card is revealed in the floating group, with its header, rounded corners, hairline edge and shadow, 8px from the top bar, the window's end and the bottom. The terminal shows through the surrounding gap without resizing. The remembered width is 1000px, so the group can exceed 720px within the docked sidebar's content edge. Its outer width handle resizes the floating group. Leaving or Escape hides it; hovering the group or a panel toggle reveals the stored open Report again. Hovering Processes reveals only Report, without opening Processes. The Report toggle is pressed while the group is visible, and pressing it closes Report. With no cards left open, hovering the toggles or the end-edge strip leaves the pane hidden until a panel is explicitly opened.",
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
    description: "At 768px and narrower the rail is not in the row: the content panel spans the window's full width under the top bar, and its top corner at the start edge is square, where wider windows round it beside the rail. The sidebar toggle brings the rail in with the drawer (see Mobile sidebar card), and so does a swipe towards the end of the line on a touch screen (a touch-emulating window will do), the card sliding in from the start edge; a swipe towards the start opens the right pane's panels one at a time, in the order of the top bar's toggles (here only Files: Report has no pages, Processes has confirmed no processes, and the shell Terminal has no tabs; nonempty panels join the cycle in toggle order), each sliding in from the end edge and the last returning to the first, and one towards the end closes the open panel. The width is fixed because the layout exists only in this range.",
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
    description: "At 768px and narrower the sidebar keeps its fixed 328px width, including the rail, and has no resize handle. It is one card floating over the dimmed terminal, inset 4px from the window's edges and below the top bar, with the rounded corners, hairline edge and drop shadow of the hidden desktop sidebar's preview. The rail, which is not in the row at this width, is inside it at the start side, transparent over the card, with a single vertical line between it and the sidebar's content; the backdrop covers the whole window around the card. Close and reopen it with the toggle to inspect the card sliding in from the start edge with a fade, the backdrop fading with it; a swipe does the same on a touch screen (a touch-emulating window will do): sideways towards the end of the line opens it, towards the start closes it. The backdrop and Escape also close it; Arabic mirrors the card, which slides in from the right.",
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
    // Rail 48px + visible sidebar 260px + terminal 400px + one-column pane 276px.
    minWidth: 984,
    state: railState,
    description: "A console session's report is a panel of the right pane: one card filling the pane's height, with the Report name, the page arrows and count, and Close panel in its 40px header, rounded corners, a hairline edge and the shadow of the sidebar's floating card. A gap showing the terminal's own background surrounds it: 8px at the window's end, the bottom and below the top bar; no sidebar-coloured column sits behind it. The agent terminal's character grid has 14px of visible space on each side, including its internal scrollbar on the physical right and the panel-side gap without adding either to the spacing. Any fraction left by whole character columns is shared between the two sides. The narrow width handle sits just inside the card's terminal-facing edge, clear of the terminal scrollbar. Choose Arabic to mirror the panes while the terminal stays left to right with the same visible spacing. The top bar has the Report toggle, pressed, and the Processes toggle, closed: a console session has no Files or shell Terminal toggle, a terminal needing a project's folder, and none for the pane itself. At 1168px the report stays docked beside the sidebar; 984px is the minimum docking width with the sidebar visible, leaving at least 400px for the agent terminal. Above 768px, hiding the sidebar lets the report dock whenever its minimum width and the terminal fit. Press Close panel to close the pane, which goes at once.",
    steps: [
      (ui) => ui.press(ui.session("Hub")),
    ],
  },
  {
    id: "layout-project-pane",
    group: GROUP,
    category: "normal",
    title: "Files panel beside or over the terminal",
    state: railState,
    preferences: { sidebarConsole: main.id },
    description: "In a wide window the Files panel docks beside the terminal as one card, with the Files and Changes tabs, the filter and refresh buttons and Close panel in its header and the top bar's Files toggle pressed. At 1168px Files and the sidebar both stay docked. Above 768px the sidebar keeps its docked space while Files floats whenever the terminal and both panes do not fit; hiding the sidebar can let Files dock. While the group floats, its stored open Files card appears by hovering or pressing a toggle and hides after the pointer leaves. Docked panels do not appear on hover, and a closed card needs a press to open. While the floating group is visible, pressing Files again closes only that card; any other open cards remain, and closing the last card hides the group. When the group is hidden, pressing an already-open panel’s icon reveals it first. Either side’s group icon reveals its floating group without toggling it closed. Escape dismisses a floating group, and a card’s X closes that card. Other open right-hand panels would float with Files. Choose Tablet portrait (768px) or Phone (390px): only one right-hand card floats over the terminal, with a dimmed 48px strip of the terminal beside it, reaching the window's start edge (mirrored under right-to-left) since no rail stands there, that closes it when pressed. Below 390px the dimmed strip narrows to 24px to leave more room for the card. The terminal under the card keeps the whole width and cannot be focused. The card fades in as it slides in from the window's end edge (the start edge under right-to-left), the dimmed strip fading with it; on a touch screen a swipe towards the end of the line closes it, and one towards the start switches to the next panel. The top bar's Files toggle is pressed; Close panel, the strip and Escape all close the card, and opening the sessions drawer closes it, as opening the card closes the drawer.",
    steps: [
      (ui) => ui.press(ui.session("Fix the summary layout")),
      // Picking a session closes the mobile sidebar, which clears any floating panel.
      (ui) => ui.closeDrawer(),
      (ui) => ui.showPanel("files"),
    ],
  },
  {
    id: "layout-project-pane-floating",
    group: GROUP,
    category: "normal",
    title: "Floating Files panel",
    // Hover reveals a pane above 768px; at that width and narrower only a press opens a single mobile card.
    minWidth: 769,
    state: railState,
    preferences: { asideDocked: false, panelStacks: { project: { open: ["files"], shares: {} } } },
    description: "The stored open Files panel is revealed as a floating group, with rounded corners, a hairline edge and the sidebar card's shadow, 8px from the top bar, the window's end and the bottom. The surrounding gaps show the agent terminal without resizing it. The Files toggle is pressed while visible. Hover the Terminal toggle: only the open Files card remains, without a Terminal preview. Close Files, then hover either panel or group toggle or the end-edge strip: the empty group stays hidden. Press a panel toggle to open that card. The group toggle docks the open cards when there is room.",
    steps: [
      (ui) => ui.press(ui.session("Fix the summary layout")),
      (ui) => ui.floatPane(ui.t("titleBar.files.show")),
    ],
  },
  {
    id: "layout-quit-sessions-and-terminals",
    group: GROUP,
    category: "normal",
    title: "Quit held up by a session and a terminal",
    description:
      "Quitting with a working session and a busy shell terminal: one dialog says both are still running, that the sessions are interrupted and can each be resumed next time, and that what the terminals run is ended. A quit held up by a terminal alone words only that half.",
    state: quitBusyState,
    quitRequested: true,
  },
  {
    id: "layout-quit-failed",
    group: GROUP,
    category: "exceptional",
    title: "Quit that did not end the app",
    description:
      "Nothing holds the quit up, so it goes straight to the shell, which refuses: the error alert dialog is titled \"Could not quit\" and says Octoboard kept running instead of ending and to try quitting again; the shell's text is behind Error details, closed, with the Report issue guidance and button of an unexpected failure. A restart that the shell refuses, and a permission request that does not go through, use the same alert with their own sentence.",
    state: quitQuietState,
    quitRequested: true,
    exitFailure: "the application is already shutting down",
  },
];
