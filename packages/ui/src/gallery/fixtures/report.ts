import type { Scenario } from "../scenario";
import type { Ui } from "../../fixtures/interact";
import { consoleOf, pageOf, projectOf, processTreeOf, sessionOf, shellTerminalOf, snapshotState } from "../../fixtures/builders";
import { allPagesDeleted, latestPageDeleted, pagePushed } from "./liveChanges";

const GROUP = "Report panel";

const console_ = consoleOf("c-1", "Main");
const project = projectOf("p-1", console_.id, "Search API");
const summary = (title: string) => `<!doctype html><html><head><title>${title}</title></head><body>
<h1>${title}</h1>
<p>Two sessions finished since the last report. Nothing needs your attention.</p>
<table><thead><tr><th>Session</th><th>Result</th></tr></thead>
<tbody><tr><td>Add idempotency keys</td><td>Merged</td></tr><tr><td>Audit the retry path</td><td>Needs review</td></tr></tbody></table>
<ul><li>Tests: 214 passed</li><li>Coverage: 91%</li></ul>
</body></html>`;

const form = `<!doctype html><html><head><title>Choose a release</title></head><body>
<h1>Which release should the console session cut?</h1>
<form>
  <p><label>Version <input type="text" name="version" value="1.4.0"></label></p>
  <p><label><input type="radio" name="channel" value="stable" checked> Stable</label>
     <label><input type="radio" name="channel" value="beta"> Beta</label></p>
  <p><label>Notes<br><textarea name="notes" rows="3" cols="30">Ship it.</textarea></label></p>
  <p><button type="submit">Submit</button></p>
</form>
</body></html>`;

// Four console sessions keep empty, old, historical and interactive reports independently.
const hub1 = sessionOf("s-hub-1", console_.id, undefined, "Hub", "idle", { colour: "yellow" });
const hub2 = sessionOf("s-hub-2", console_.id, undefined, "Hub 2", "idle", { colour: "lime" });
const emptyHub = sessionOf("s-hub-empty", console_.id, undefined, "No reports", "idle", { colour: "cyan" });
const oldHub = sessionOf("s-hub-old", console_.id, undefined, "Old report", "idle", { colour: "violet" });
const twoSessions = snapshotState({
  consoles: [console_],
  projects: [project],
  sessions: [hub1, hub2, emptyHub, oldHub],
  pages: {
    [emptyHub.id]: [],
    [oldHub.id]: [pageOf("old-1", oldHub.id, summary("A day old"), 60 * 30)],
    [hub1.id]: [pageOf("h1-1", hub1.id, summary("Hub, first"), 60 * 6), pageOf("h1-2", hub1.id, summary("Hub, second"), 30)],
    [hub2.id]: [
      pageOf("h2-1", hub2.id, summary("Hub 2, first"), 60 * 5),
      pageOf("h2-2", hub2.id, summary("Hub 2, second"), 60 * 2),
      pageOf("h2-3", hub2.id, form, 10),
    ],
  },
});

// Project sessions that face the user push pages too. The Report panel is the selected session's own, beside the
// project's shared Files and Terminal panels: one session with pages, another whose pages must not show in it,
// and one with none.
const withReport = sessionOf("s-proj-report", console_.id, project.id, "Add idempotency keys", "idle");
const otherWithReport = sessionOf("s-proj-other", console_.id, project.id, "Audit the retry path", "idle");
const withoutReport = sessionOf("s-proj-empty", console_.id, project.id, "Tidy the fixtures", "idle");
const projectSessions = snapshotState({
  consoles: [console_],
  projects: [project],
  sessions: [withReport, otherWithReport, withoutReport],
  pages: {
    [withReport.id]: [pageOf("pr-1", withReport.id, summary("Idempotency keys, first pass"), 60 * 4), pageOf("pr-2", withReport.id, summary("Idempotency keys, ready for review"), 20)],
    [otherWithReport.id]: [pageOf("po-1", otherWithReport.id, summary("Retry path audit"), 45)],
  },
});

/** Selects the project session and gives the Files panel, open beside the Report, time to read its folder. */
const openProjectSession = (title: string) => async (ui: Ui) => {
  await ui.press(ui.session(title));
  await ui.wait(400);
};

// A project session whose report changes while it is on screen.
const liveSession = sessionOf("s-proj-live", console_.id, project.id, "Plan the rollout", "idle");
const livePages = [
  pageOf("pl-1", liveSession.id, summary("Rollout, draft"), 60 * 3),
  pageOf("pl-2", liveSession.id, summary("Rollout, revised"), 60),
  pageOf("pl-3", liveSession.id, summary("Rollout, final"), 10),
];
const liveProject = snapshotState({ consoles: [console_], projects: [project], sessions: [liveSession], pages: { [liveSession.id]: livePages } });

// Sessions whose page list the daemon refuses: a console session that already holds pages from the
// snapshot, and ones that have none yet, so nothing but the failure is known about them.
const listFailure = { code: "internal_error", params: { detail: "database is locked" }, message: "database is locked" };
const stale = sessionOf("s-hub-stale", console_.id, undefined, "Stale pages", "idle", { colour: "yellow" });
const unlisted = sessionOf("s-hub-unlisted", console_.id, undefined, "Pages never listed", "idle", { colour: "cyan" });
const failedLists = snapshotState({
  consoles: [console_],
  projects: [project],
  sessions: [stale, unlisted],
  pages: { [stale.id]: [pageOf("stale-1", stale.id, summary("Earlier report"), 60 * 3)] },
});

const panelState = snapshotState({
  consoles: [console_],
  projects: [project],
  sessions: [withReport],
  pages: { [withReport.id]: [pageOf("pc-1", withReport.id, summary("Idempotency keys, ready for review"), 20)] },
  shellTerminals: [shellTerminalOf("sh-report", project, "zsh", 0)],
});
const switchingProject = projectOf("p-panel-switch", console_.id, "Website");
const switchingSession = sessionOf("s-panel-switch", console_.id, switchingProject.id, "Adjust the page layout", "idle");
const threePanelState = snapshotState({
  consoles: [console_],
  projects: [project, switchingProject],
  sessions: [withReport, withoutReport, switchingSession],
  pages: { [withReport.id]: [pageOf("pc-1", withReport.id, summary("Idempotency keys, ready for review"), 20)] },
  shellTerminals: [shellTerminalOf("sh-report", project, "zsh", 0)],
});
const panelProcesses = { trees: [processTreeOf("400:1", withReport, "pnpm dev", 60, { ports: [5173] })] };

export const reportScenarios: Scenario[] = [
  {
    id: "report-three-panels",
    group: GROUP,
    category: "normal",
    title: "Three panels in two columns",
    description: "Report fills the first column, with Terminal above Files in the second. The saved pane width is 720px before any viewport clamp, split into two columns with their 10px gap and 16px outer spacing. Adding or removing a column preserves the remaining cards' widths by changing the group's total width, until the docked group's 720px limit or the floating group's available space requires narrower cards. Close Report in the initial layout: Terminal and Files stack in one column, keeping their displayed card widths. Select Tidy the fixtures in Search API or Adjust the page layout in Website, either with Report still open or after closing it: the remaining pair keeps the same narrow width. Opening or closing a card without changing the column count keeps the group width. Drag the full-height grip between columns or the height separator between Terminal and Files; both remain operable when the group floats. While the group is visible, an individual top-bar panel icon toggles only its card; the others stay visible until the last card is closed. With the sidebar visible the group docks from 1254px. At 1168px it floats; hide the sidebar with its top-bar toggle to dock both columns beside a terminal of at least 400px, then show the sidebar to float the group again. Choose Tablet portrait (768px) or Phone (390px): the sidebar becomes a drawer and only one right-hand card is shown; swipe towards the start to cycle through available panels, or towards the end to close it.",
    state: threePanelState,
    preferences: { asideWidth: 720, panelStacks: { project: { open: ["terminal", "files"], shares: {} } } },
    steps: [openProjectSession(withReport.title), (ui) => ui.closeDrawer(), (ui) => ui.showPanel("report")],
  },
  {
    id: "report-four-panels",
    group: GROUP,
    category: "normal",
    title: "Four panels in two columns",
    description: "Report and Terminal stack in the first column, Files and Processes in the second. Each column keeps at least 260px. A full-height grip changes their widths together: drag it, or focus it and press Left or Right; Shift makes a larger step, Home and End reach the limits, and double-click restores equal widths. The chosen proportion is remembered for this owner kind. Each column has its own height separator. While the group floats visibly, each panel’s top-bar icon toggles only its card, leaving the other open cards visible; closing the last card hides the group. When the two-column group fits beside the visible sidebar and a 400px terminal, both Report and Terminal offer Maximize panel. Otherwise the entire group floats over the agent terminal, retaining its two columns and all four open cards; at 768px and narrower it becomes one card at a time. Close a card to see the remaining cards fill columns in Report, Terminal, Files, Processes order, with at most two in each column.",
    state: panelState,
    preferences: { asideWidth: 720, panelStacks: { project: { open: ["terminal", "files", "processes"], shares: {} } } },
    processes: panelProcesses,
    steps: [openProjectSession(withReport.title), (ui) => ui.closeDrawer(), (ui) => ui.showPanel("report")],
  },
  {
    id: "report-floating-stack",
    group: GROUP,
    category: "normal",
    title: "All open panels in a floating group",
    description: "The right pane is set to float even on a wide screen. Hovering its group icon in the top bar reveals Report, Terminal and Files together over an agent terminal that keeps the full available width. Individual panel toggles also reveal the whole group. The window's right-edge trigger is 48px wide, matching the left rail: rest there for 200ms to reveal the group, or cross it briefly and leave to confirm it stays hidden. Moving from a top-bar trigger through the small gap directly below it into the revealed cards keeps the group visible, including when the pointer pauses in that gap. Moving away from this connecting strip still hides the group after the normal delay. On the left, a 200ms pause over blank rail space reveals the sidebar; console avatars keep their separate 130ms delay, while Usage, Settings and other action buttons are excluded. Move away to hide it after a short delay, then compare those three entry points. Move onto the width grip along its terminal-side edge and drag it: the grip remains reachable and resizing keeps the group visible, with a 546px minimum for its two columns. Clicking the group icon restores docking on a wide screen. At 800px the sidebar can remain docked while the right group floats automatically; clicking either side’s group icon reveals its floating group and keeps it visible on repeated clicks, preserving the saved docking preference. While the group is visible, an individual panel icon closes its open card or opens its closed card; other open cards remain visible, and closing the last card hides the group. When the group is hidden, pressing an individual icon first reveals the group without closing that icon’s already-open card. A card’s X also closes that card; leaving the group or pressing Escape dismisses it without closing its cards. Its height separators and outer width grip remain operable while it floats. Choose Phone (390px): at 768px and narrower the edge trigger and group icon are absent, and this same view opens Report as a single floating card. Swipe towards the start to cycle available cards and towards the end to close the shown card; individual panel toggles retain the mobile one-card interaction.",
    state: panelState,
    preferences: { asideDocked: false, asideWidth: 320, panelStacks: { project: { open: ["terminal", "files"], shares: {} } } },
    steps: [
      openProjectSession(withReport.title),
      (ui) => ui.closeDrawer(),
      (ui) => ui.floatPane((name) => [ui.t("titleBar.aside.dock"), ui.t("titleBar.aside.show")].includes(name), "report"),
    ],
  },
  {
    id: "report-maximized",
    group: GROUP,
    category: "normal",
    title: "Report panel maximized",
    description: "Report covers the agent terminal's area, keeping its page and pager, while Terminal fills the first column and Files and Processes stack in the second. The full-height grip still adjusts the widths of the remaining two columns while Report is maximized. Restore panel size returns Report above Terminal and puts Files above Processes, preserving the chosen column proportion. Restoring or maximizing leaves the report page and shell in place.",
    // Floating cards have no maximize control.
    minWidth: 1254,
    state: panelState,
    preferences: { asideWidth: 720, panelStacks: { project: { open: ["terminal", "files", "processes"], shares: {} } } },
    processes: panelProcesses,
    steps: [openProjectSession(withReport.title), (ui) => ui.closeDrawer(), (ui) => ui.pressInPanel("report", ui.t("panel.maximize"))],
  },
  {
    id: "report-two-console-sessions",
    group: GROUP,
    category: "normal",
    title: "Latest interactive report",
    state: twoSessions,
    description: "The latest report is an interactive form; pagination belongs to the selected console session. The \"Live changes\" buttons delete its newest page (the pager moves to the one before, and the count drops), delete all of its pages (the empty state takes the panel's body, the panel staying) and push a new one (it appears as the latest). Choose Phone (390px) to see the layout at 768px and narrower: the Report card floats over the console session's terminal instead of docking beside it, with a dimmed strip of the terminal beside it, reaching the window's start edge since no rail stands there, that closes it when pressed. The card keeps its header, the page arrows and count and Close panel; opening it with a mouse or the keyboard moves focus into the card, onto the page arrows' first enabled button (Previous page, on the last page shown).",
    steps: [
      (ui) => ui.press(ui.session("Hub 2")),
      (ui) => ui.showPanel("report"),
    ],
    liveChanges: [
      latestPageDeleted("delete-latest", "Hub 2's newest page is deleted", hub2.id),
      allPagesDeleted("delete-all", "All of Hub 2's pages are deleted", hub2.id),
      pagePushed("push-page", "A new page is pushed to Hub 2", pageOf("h2-new", hub2.id, summary("Hub 2, pushed live"), 0)),
    ],
  },
  {
    id: "report-history",
    group: GROUP,
    category: "normal",
    title: "Historical report",
    state: twoSessions,
    description: "The first page is read-only and its controls are disabled.",
    steps: [
      (ui) => ui.press(ui.session("Hub")),
      (ui) => ui.showPanel("report"),
      (ui) => ui.press(ui.t("report.previous")),
    ],
  },
  {
    id: "report-no-pages",
    group: GROUP,
    category: "normal",
    title: "No reports",
    state: twoSessions,
    steps: [
      (ui) => ui.press(ui.session("No reports")),
    ],
  },
  {
    id: "report-one-old-page",
    group: GROUP,
    category: "normal",
    title: "One old report",
    state: twoSessions,
    steps: [
      (ui) => ui.press(ui.session("Old report")),
    ],
  },
  {
    id: "report-project-session",
    group: GROUP,
    category: "normal",
    title: "Project session with a report",
    description: "A project session that has pages: the right pane stacks a Report card above the project's Files card, and the top bar offers the Report, Terminal and Files toggles in that order, the order the cards stack, Report and Files pressed. Close either card and reopen it: the pair remains stacked in one column, preserving the card width. The Report shows only the selected session's two pages, on the latest, not those of the project's other session. Selecting that other session swaps the Report for its own page; the Files card stays as it was. Choose Phone (390px) to see the layout at 768px and narrower: the Report floats over the terminal as the only card, as a console session's does, and the top bar lists the Report, Terminal and Files toggles, Report pressed. The \"Live changes\" buttons delete all of the session's pages (the card and its toggle go, the terminal uncovered) and push a page (only the toggle returns, not pressed: at that width the card does not float again until the toggle is pressed).",
    state: projectSessions,
    steps: [openProjectSession(withReport.title), (ui) => ui.showPanel("report")],
    liveChanges: [
      allPagesDeleted("delete-all", "All pages are deleted", withReport.id),
      pagePushed("push-page", "A page is pushed", pageOf("pr-new", withReport.id, summary("Idempotency keys, pushed live"), 0)),
    ],
  },
  {
    id: "report-project-closed",
    group: GROUP,
    category: "normal",
    title: "Project session with its report closed",
    description: "The session has pages but its Report is closed: the pane holds only Files, and the Report toggle in the top bar, still offered ahead of Terminal and Files, is not pressed and reads \"Show report panel\". Whether the Report is open belongs to the session: the project's other session, selected next, still has its own open.",
    state: projectSessions,
    preferences: { closedReportPanels: [withReport.id] },
    steps: [openProjectSession(withReport.title)],
  },
  {
    id: "report-project-no-pages",
    group: GROUP,
    category: "normal",
    title: "Project session with no reports",
    description: "A project session that has pushed nothing: the top bar offers only the Terminal and Files toggles and the pane holds Files, with no Report toggle or card.",
    state: projectSessions,
    steps: [openProjectSession(withoutReport.title)],
  },
  {
    id: "report-project-live",
    group: GROUP,
    category: "normal",
    title: "Project session's report changing live",
    description: "A project session with three pages, Report and Files open. The \"Live changes\" buttons delete its newest page (the pager moves to the one before and the count drops), delete all of its pages (the Report card and its toggle go, leaving Files alone in the pane) and push a page (the toggle and the card come back, showing the pushed page).",
    state: liveProject,
    steps: [openProjectSession(liveSession.title)],
    liveChanges: [
      latestPageDeleted("delete-latest", "The newest page is deleted", liveSession.id),
      allPagesDeleted("delete-all", "All pages are deleted", liveSession.id),
      pagePushed("push-page", "A page is pushed", pageOf("pl-new", liveSession.id, summary("Rollout, pushed live"), 0)),
    ],
  },
  {
    id: "report-list-failed",
    group: GROUP,
    category: "exceptional",
    title: "Reports that could not be listed, none known yet",
    description: "The console session has no page the panel knows of, so the failure takes the panel's place in the empty-state shape, under the panel header: a danger glyph, the heading and the application's generic sentence. The daemon's raw message sits on a surface of its own under the sentence, with Try again as the only button below it.",
    state: failedLists,
    pageListErrors: { [unlisted.id]: listFailure },
    steps: [
      (ui) => ui.press(ui.session("Pages never listed")),
    ],
  },
  {
    id: "report-project-list-failed",
    group: GROUP,
    category: "exceptional",
    title: "Project session whose reports could not be listed",
    description: "A project session whose page listing failed, with nothing known of its pages: the Report toggle is still offered, ahead of Terminal and Files, and its card sits above Files with the failure in place of a page, under the panel header, with the daemon's raw message on a surface of its own under the sentence and Try again below it.",
    state: projectSessions,
    pageListErrors: { [withoutReport.id]: listFailure },
    steps: [openProjectSession(withoutReport.title)],
  },
  {
    id: "report-list-failed-with-pages",
    group: GROUP,
    category: "exceptional",
    title: "Reports that could not be listed again, earlier ones kept",
    description: "The page already in hand stays readable with no Alert above it. A small warning mark sits in the panel header right after the title Report, ahead of the pager, named \"Could not load the pages of this session\".",
    state: failedLists,
    pageListErrors: { [stale.id]: listFailure },
    steps: [
      (ui) => ui.press(ui.session("Stale pages")),
    ],
  },
  {
    id: "report-list-failed-with-pages-details",
    group: GROUP,
    category: "exceptional",
    title: "Reports that could not be listed again, mark opened",
    description: "The warning mark after Report is hovered, and its popover leads with \"Could not load the pages of this session.\", then the application's generic sentence about the failure, the daemon's raw message under it, and Try again, over the page that stays readable.",
    state: failedLists,
    pageListErrors: { [stale.id]: listFailure },
    steps: [
      (ui) => ui.press(ui.session("Stale pages")),
      (ui) => ui.showPanel("report"),
      (ui) => ui.hoverStatusMark(ui.t("report.listFailed")),
    ],
  },
];
