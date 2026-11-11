import type { ConsoleSessionColour, PageContent } from "../../protocol";
import type { Scenario } from "../scenario";
import type { Ui } from "../../fixtures/interact";
import { consoleOf, pageOf, projectOf, processTreeOf, sessionOf, shellTerminalOf, snapshotState } from "../../fixtures/builders";
import { allPagesDeleted, latestPageDeleted, pagePushed } from "./liveChanges";

const GROUP = "Report panel";

const console_ = consoleOf("c-1", "Main");
const project = projectOf("p-1", console_.id, "Search API");
/** A report that is only text and a table: the page most scenarios here need and do not look at. */
const summary = (title: string): PageContent => ({
  title,
  body: [{
    type: "markdown",
    text: "Two sessions finished since the last report. Nothing needs your attention.\n\n"
      + "| Session | Result |\n|---|---|\n| Add idempotency keys | Merged |\n| Audit the retry path | Needs review |\n\n"
      + "- Tests: 214 passed\n- Coverage: 91%",
  }],
});

const form: PageContent = {
  title: "Which release should the console session cut?",
  body: [
    { type: "text", name: "version", label: "Version", default: "1.4.0" },
    { type: "radio", name: "channel", label: "Channel", default: "stable", options: [{ value: "stable", label: "Stable" }, { value: "beta", label: "Beta" }] },
    { type: "textarea", name: "notes", label: "Notes", default: "Ship it.", rows: 3 },
    { type: "submit" },
  ],
};

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

// Pages with forms. Each console session below holds one page (or two, for the history), so a scenario picks its page
// by selecting the session.
const platforms = [{ value: "macos", label: "macOS" }, { value: "windows", label: "Windows" }, { value: "linux", label: "Linux" }];

/** Every kind of component. The Beta channel is chosen, so the notes that depend on it are shown; picking Stable hides them. */
const everyComponent: PageContent = {
  title: "Release readiness",
  body: [
    {
      type: "markdown",
      text: "The **1.4.0** candidate passed its checks. Open points are in the table; run `pnpm verify` again after changing any of them.\n\n"
        + "| Check | Result | Duration |\n|---|:---:|---:|\n| Unit tests | passed | 41 s |\n| Packaging | passed | 3 min |\n| Upgrade from 1.3 | ~~failed~~ passed | 2 min |\n\n"
        + "```rust\nfn main() {\n    println!(\"release {}\", env!(\"CARGO_PKG_VERSION\"));\n}\n```\n\n"
        + "- [x] Changelog written\n- [ ] Announcement drafted",
    },
    {
      type: "section",
      heading: "Release",
      children: [
        { type: "text", name: "version", label: "Version", description: "The version the tag is cut as.", default: "1.4.0", placeholder: "1.4.0", required: true },
        { type: "radio", name: "channel", label: "Channel", description: "Where the build is published first.", default: "beta", options: [{ value: "stable", label: "Stable" }, { value: "beta", label: "Beta" }] },
        { type: "textarea", name: "beta_notes", label: "What should beta testers try?", description: "Shown to the testers with the build.", default: "The new report forms.", rows: 3, visible_when: { field: "channel", equals: "beta" } },
        { type: "select", name: "region", label: "Region", placeholder: "Choose an option", options: [{ value: "eu", label: "Europe" }, { value: "us", label: "United States" }, { value: "ap", label: "Asia Pacific" }] },
        { type: "checkboxes", name: "platforms", label: "Platforms", description: "At least one.", default: ["macos", "windows"], options: platforms, min_selected: 1 },
      ],
    },
    {
      type: "section",
      heading: "Schedule",
      children: [
        { type: "date", name: "freeze", label: "Code freeze", default: "2026-11-20" },
        { type: "time", name: "window", label: "Daily window", default: "09:30" },
        { type: "datetime", name: "launch", label: "Launch", description: "Local time.", default: "2026-11-27T10:00" },
      ],
    },
    {
      type: "columns",
      children: [
        { type: "callout", tone: "info", title: "Beta build", text: "Testers get this build first.", visible_when: { field: "channel", equals: "beta" } },
        { type: "key_values", items: [{ key: "Open findings", value: 3 }, { key: "Blocking", value: 0 }] },
      ],
    },
    {
      type: "section",
      children: [
        { type: "number", name: "retries", label: "Retries", description: "Between 0 and 10.", default: 3, min: 0, max: 10 },
        { type: "slider", name: "rollout", label: "Rollout", description: "Share of users reached first.", min: 0, max: 100, step: 5, default: 25 },
        { type: "toggle", name: "notify", label: "Notify subscribers", default: true },
      ],
    },
    { type: "choices", name: "decision", label: "Decision", options: [{ value: "ship", label: "Ship it" }, { value: "hold", label: "Hold" }, { value: "cancel" }] },
    { type: "submit", label: "Send answers" },
  ],
};

/** A form with a rule on every field and no default that breaks one, as the daemon would refuse: the
 * `report-validation` steps enter the values that break them. */
const invalidForm: PageContent = {
  title: "Release details",
  body: [
    { type: "text", name: "name", label: "Release name", required: true },
    { type: "text", name: "version", label: "Release version", description: "Like 1.4.0.", pattern: "\\d+\\.\\d+\\.\\d+" },
    { type: "text", name: "reference", label: "Reference code", description: "Only a's.", pattern: "^(a+)+$" },
    { type: "textarea", name: "summary", label: "Summary", min_length: 20, rows: 2 },
    { type: "number", name: "retries", label: "Retries", min: 0, max: 10, step: 5 },
    { type: "date", name: "freeze", label: "Code freeze", min: "2026-12-01" },
    { type: "select", name: "region", label: "Region", required: true, placeholder: "Choose an option", options: [{ value: "eu", label: "Europe" }, { value: "us", label: "United States" }] },
    { type: "radio", name: "channel", label: "Channel", required: true, options: [{ value: "stable", label: "Stable" }, { value: "beta", label: "Beta" }] },
    { type: "checkboxes", name: "platforms", label: "Platforms", options: platforms, min_selected: 2 },
    { type: "submit" },
  ],
};

/** A form that ends in choice buttons: what the submission states below are shown on. */
const decision: PageContent = {
  title: "Ship the release?",
  body: [
    { type: "markdown", text: "The candidate is ready. Pick the channel and say what to do." },
    { type: "text", name: "version", label: "Version", default: "1.4.0" },
    { type: "radio", name: "channel", label: "Channel", default: "stable", options: [{ value: "stable", label: "Stable" }, { value: "beta", label: "Beta" }] },
    { type: "checkboxes", name: "platforms", label: "Platforms", default: ["macos", "windows"], options: platforms },
    { type: "textarea", name: "notes", label: "Notes", default: "Mention the new report forms.", rows: 3 },
    { type: "toggle", name: "notify", label: "Notify subscribers", default: true },
    { type: "choices", name: "decision", label: "Decision", options: [{ value: "ship", label: "Ship it" }, { value: "hold", label: "Hold" }] },
  ],
};

const inlineSketch = `data:image/svg+xml;base64,${btoa('<svg xmlns="http://www.w3.org/2000/svg" width="240" height="90" viewBox="0 0 240 90"><rect width="240" height="90" rx="8" fill="#4f7cff"/><circle cx="45" cy="45" r="24" fill="#ffd166"/><rect x="90" y="30" width="120" height="12" rx="6" fill="#fff"/><rect x="90" y="54" width="80" height="12" rx="6" fill="#fff" fill-opacity=".7"/></svg>')}`;

/** Markdown that tries to reach beyond the page: none of it does. */
const untrustedMarkdown: PageContent = {
  title: "Notes from the agent",
  body: [{
    type: "markdown",
    text: "Raw HTML stays the text it was written as: <b>not bold</b> and <script>alert(1)</script>.\n\n"
      + '<img src="https://example.com/tracker.png" alt="tracker">\n\n'
      + `An image that carries its own bytes is drawn:\n\n![Inline sketch](${inlineSketch})\n\n`
      + "One that would be fetched stays its description: ![Build badge](https://example.com/badge.png)\n\n"
      + "Relative links, [the guide](./docs/guide.md) and [a heading](#notes), are plain text; [a web link](https://example.com/report) opens in the browser.",
  }],
};

const formHub = (id: string, title: string, colour: ConsoleSessionColour) => sessionOf(id, console_.id, undefined, title, "idle", { colour });
const everyHub = formHub("s-form-all", "Every component", "teal");
const invalidHub = formHub("s-form-invalid", "Validation", "yellow");
const decisionHub = formHub("s-form-decision", "Release decision", "lime");
const decidedHub = formHub("s-form-decided", "Release decided", "cyan");
const earlierHub = formHub("s-form-earlier", "Earlier form", "violet");
const untrustedHub = formHub("s-form-untrusted", "Untrusted Markdown", "orange");
const formPages = snapshotState({
  consoles: [console_],
  projects: [project],
  sessions: [everyHub, invalidHub, decisionHub, decidedHub, earlierHub, untrustedHub],
  pages: {
    [everyHub.id]: [pageOf("pf-all", everyHub.id, everyComponent, 10)],
    [invalidHub.id]: [pageOf("pf-invalid", invalidHub.id, invalidForm, 10)],
    [decisionHub.id]: [pageOf("pf-decision", decisionHub.id, decision, 10)],
    [decidedHub.id]: [pageOf("pf-decided", decidedHub.id, decision, 60, {
      version: "1.4.0", channel: "beta", platforms: ["macos", "windows"], notes: "Mention the new report forms.", notify: true, decision: "ship",
    })],
    [earlierHub.id]: [pageOf("pf-earlier", earlierHub.id, decision, 60 * 5), pageOf("pf-later", earlierHub.id, summary("Release 1.4.0 shipped"), 20)],
    [untrustedHub.id]: [pageOf("pf-untrusted", untrustedHub.id, untrustedMarkdown, 10)],
  },
});

/** Selects the console session and brings its Report into view. */
const openForm = (title: string) => async (ui: Ui) => {
  await ui.press(ui.session(title));
  await ui.showPanel("report");
};

const notRunning = { code: "session_not_running", params: { session: decisionHub.id }, message: "This session is not running." };
const unconfirmed = { code: "message_acceptance_unconfirmed", params: { agent: "Codex" }, message: "Codex did not confirm that it accepted a message." };

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
    id: "report-components",
    group: GROUP,
    category: "normal",
    title: "Page with every kind of component",
    description: "A page title over a Markdown block with a table, code and a task list, then a section with a heading, one without, every kind of input with its label, description and default, and both kinds of submitter. A row of two columns holds a note for beta builds beside key-values. The Beta channel is chosen, so the beta testers' notes and that note show; choosing Stable hides them, and the key-values take the whole row with no empty cell left where the note was (the two columns sit side by side only in a wide panel: Maximize panel). Pick Light or Dark with the page's Theme control: the page takes the application's appearance and direction.",
    state: formPages,
    steps: [openForm(everyHub.title)],
  },
  {
    id: "report-markdown-untrusted",
    group: GROUP,
    category: "normal",
    title: "Markdown in a page stays inert",
    description: "The raw HTML is shown as text, the image that carries its own data is drawn, the remote image stays its description, and the relative links are plain text beside the web link.",
    state: formPages,
    steps: [openForm(untrustedHub.title)],
  },
  {
    id: "report-submission-pending",
    group: GROUP,
    category: "normal",
    title: "Form locked while its submission is out",
    description: "The Ship it button is pressed and waiting for the daemon: it shows its busy state while every other control has the read-only look (a plain 1px border, no fill or shadow, text at full strength and selectable), and the panel header has not yet shown its Submitted badge.",
    state: formPages,
    pageSubmit: "pending",
    steps: [openForm(decisionHub.title), (ui) => ui.pressInPanel("report", "Ship it")],
  },
  {
    id: "report-submitted",
    group: GROUP,
    category: "normal",
    title: "Submitted page",
    description: "The page was submitted earlier, so its controls have the read-only look: text fields, text areas and selects keep a plain 1px border with no fill or shadow and their text at full strength, still selectable. A Submitted badge sits in the panel header after the page's time, with the pressed choice, Ship it, marked and the other unavailable.",
    state: formPages,
    steps: [openForm(decidedHub.title)],
  },
  {
    id: "report-history",
    group: GROUP,
    category: "normal",
    title: "Historical report",
    state: formPages,
    description: "The first page, a form that was never submitted, is shown after paging back from a newer one: a Read-only badge in the panel header after the page's time, and its controls with the read-only look (a plain 1px border, no fill or shadow) or unavailable.",
    steps: [
      openForm(earlierHub.title),
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
    description: "The page was pushed more than a day ago, so the time in the panel header, after the pager, carries its date; a page pushed today shows the time alone.",
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
  {
    id: "report-validation",
    group: GROUP,
    category: "exceptional",
    title: "Form refused for invalid fields",
    description: "Values that break each field's rule were entered, then Submit was pressed: each field says what is wrong under itself, in its own words (required, format, length, step, earliest date, choice, selection count), and focus is on the first. One field's pattern runs too long to be tested, so it says the value could not be checked against the expected format rather than that it does not match. Nothing was sent.",
    state: formPages,
    steps: [
      openForm(invalidHub.title),
      (ui) => ui.focus("Release version"),
      (ui) => ui.type("1.4"),
      // A pattern that backtracks without end on this text: the page's test of it gives up, as it does on any that runs
      // long, and the field says so.
      (ui) => ui.focus("Reference code"),
      (ui) => ui.type(`${"a".repeat(40)}!`),
      (ui) => ui.focus("Summary"),
      (ui) => ui.type("Too short"),
      (ui) => ui.focus("Retries"),
      (ui) => ui.type("7"),
      (ui) => ui.focus("Code freeze"),
      (ui) => ui.type("2026-11-20"),
      (ui) => ui.press("macOS"),
      (ui) => ui.pressInPanel("report", ui.t("report.submit")),
    ],
  },
  {
    id: "report-submission-failed",
    group: GROUP,
    category: "exceptional",
    title: "Submission that certainly failed",
    description: "The daemon refused the submission before delivering it: the error is raised as an alert titled \"Could not submit the page\", and the form is editable again with the values that were entered, to submit again.",
    state: formPages,
    pageSubmit: { error: notRunning, delivered: false },
    steps: [openForm(decisionHub.title), (ui) => ui.pressInPanel("report", "Ship it")],
  },
  {
    id: "report-submission-unconfirmed",
    group: GROUP,
    category: "exceptional",
    title: "Submission whose delivery is unconfirmed",
    description: "The agent did not confirm it accepted the message, so the page stays Submitted, read-only, and the error raised says the message may already be queued rather than inviting a second submission.",
    state: formPages,
    pageSubmit: { error: unconfirmed, delivered: true },
    steps: [openForm(decisionHub.title), (ui) => ui.pressInPanel("report", "Ship it")],
  },
];

// The visual components of a page: columns, tabs, callouts, key-values, stats, code, diffs, copyable text and tables.
// Each session holds one page, so a scenario picks its page by the session's title.
const retryDiff = [
  "--- a/src/retry.rs",
  "+++ b/src/retry.rs",
  "@@ -12,7 +12,10 @@ impl Retry {",
  "     pub fn delay(&self, attempt: u32) -> Duration {",
  "-        self.base * attempt",
  "+        let jitter = self.jitter(attempt);",
  "+        // The jitter is drawn from the configured range and capped at a quarter of the base delay, so a long wait never doubles",
  "+        self.base * 2u32.pow(attempt) + jitter",
  "     }",
  " ",
  "     pub fn exhausted(&self, attempt: u32) -> bool {",
  "-        attempt > 3",
  "+        attempt >= self.max_attempts",
  "     }",
].join("\n");

const retryCode = [
  "pub fn delay(&self, attempt: u32) -> Duration {",
  "    // The jitter is drawn from the configured range and capped at a quarter of the base delay, so a long wait never doubles",
  "    let jitter = self.jitter(attempt);",
  "    self.base * 2u32.pow(attempt) + jitter",
  "}",
].join("\n");

/** Stats in columns, then every tone of callout, key-values, code, a diff and both kinds of copyable block, with a rule
 * between blocks: plain and labelled in the page's body, in a section, in a column (a section, as the daemon asks for
 * a column of several components) and with a label long enough to wrap. */
const visualReport: PageContent = {
  title: "Retry audit",
  body: [
    {
      type: "columns",
      children: [
        { type: "stat", label: "Open findings", value: 12, change: "3 fewer than last week", trend: "down", sentiment: "positive" },
        { type: "stat", label: "p95 latency (ms)", value: 1480, change: "+60 ms", trend: "up", sentiment: "negative" },
        { type: "stat", label: "Flaky tests", value: 2, change: "No change", trend: "flat", sentiment: "neutral" },
      ],
    },
    { type: "divider" },
    {
      type: "columns",
      children: [
        {
          type: "key_values",
          items: [
            { key: "Branch", value: "fix/retry-backoff" },
            { key: "Commit", value: "3f9c2ab" },
            { key: "Tests passed", value: 214 },
            { key: "Coverage (%)", value: 91.2 },
            { key: "Docs updated", value: false },
          ],
        },
        {
          type: "section",
          children: [
            { type: "stat", label: "Median retries per request", value: "1.3" },
            { type: "divider", label: "Latency" },
            { type: "stat", label: "Median latency (ms)", value: 38 },
          ],
        },
      ],
    },
    { type: "divider", label: "Findings" },
    { type: "callout", tone: "info", title: "Heads up", text: "The audit covers `src/retry.rs` only; the **client** wrappers are out of scope." },
    { type: "callout", tone: "success", title: "Backoff fixed", text: "Delays now grow exponentially and carry the configured jitter." },
    {
      type: "section",
      heading: "Open risks",
      children: [
        { type: "callout", tone: "warning", title: "Needs review", text: "The attempt limit is read from the environment and has no upper bound." },
        { type: "divider" },
        { type: "callout", tone: "danger", text: "Requests that reach the limit are dropped without a log line." },
      ],
    },
    { type: "divider", label: "The code the audit changed, the diff against the previous release and the commands that reproduce each finding on a clean checkout" },
    { type: "code", label: "Delay calculation", language: "rust", code: retryCode },
    { type: "diff", label: "src/retry.rs", diff: retryDiff },
    { type: "copyable", label: "Reproduce with", text: "pnpm --filter @octoboard/daemon test retry -- --reporter verbose --coverage --test-timeout 30000 --no-file-parallelism" },
    { type: "copyable", label: "Suggested commit message", monospace: false, text: "Back off exponentially with jitter and stop at the configured attempt limit" },
  ],
};

/** Tabs holding other components, the last label long enough to test the tab strip. */
const tabbedReport: PageContent = {
  title: "Audit results",
  body: [
    {
      type: "tabs",
      tabs: [
        {
          label: "Summary",
          children: [
            { type: "key_values", items: [{ key: "Findings", value: 12 }, { key: "Blocking", value: 2 }, { key: "Reviewed", value: true }] },
            { type: "divider", label: "Blocking" },
            { type: "callout", tone: "warning", text: "Two findings block the release." },
          ],
        },
        { label: "Changes", children: [{ type: "diff", label: "src/retry.rs", diff: retryDiff }] },
        { label: "Code", children: [{ type: "code", language: "rust", label: "Delay calculation", code: retryCode }] },
        { label: "Cross-region replication check", children: [{ type: "copyable", label: "Command", text: "pnpm run replication:check --all-regions" }] },
      ],
    },
  ],
};

/** The table the sorting and filtering scenarios work on: numbers in one column, a yes/no column and empty cells. */
const findingsTable: Extract<PageContent["body"][number], { type: "table" }> = {
  type: "table",
  caption: "Findings by area",
  columns: [{ label: "Area" }, { label: "Owner" }, { label: "Severity" }, { label: "Unresolved" }, { label: "Fixed" }, { label: "Notes" }],
  rows: [
    ["Retry path", "Mara", "High", 7, false, "Backoff ignores the jitter setting"],
    ["Request signing", "Jonas", "Medium", 12, true, null],
    ["Rate limiting", "Mara", "Low", 3, false, "Limits are per process, not per key"],
    ["Pagination", "Ines", "Medium", 25, false, null],
    ["Webhooks", "Jonas", "High", 1, true, "Retries stop after the first failure"],
    ["Idempotency keys", "Ines", "Low", 9, true, null],
    ["Audit log", "Mara", "Medium", 4, false, "Entries are not ordered across shards"],
    ["Timeouts", "Jonas", "Low", null, false, "No data yet"],
  ],
};

/** Two tables with their controls left off and a wide one that scrolls in its own frame. */
const tablesReport: PageContent = {
  title: "Open findings",
  body: [
    findingsTable,
    {
      type: "table",
      caption: "Release gates",
      sortable: false,
      filterable: false,
      columns: [{ label: "Gate" }, { label: "Status" }, { label: "Duration (s)", align: "end" }],
      rows: [["Unit tests", "passed", 41], ["Packaging", "passed", 180], ["Upgrade from 1.3", "failed", 120]],
    },
    {
      type: "table",
      caption: "Latency by region",
      columns: [
        { label: "Region" }, { label: "Requests per second" }, { label: "Median latency (ms)" }, { label: "p95 latency (ms)" },
        { label: "p99 latency (ms)" }, { label: "Error rate (%)" }, { label: "Retries per request" }, { label: "Cache hit ratio (%)" }, { label: "Healthy" },
      ],
      rows: [
        ["eu-central", 1840, 42, 180, 410, 0.4, 1.1, 93, true],
        ["us-east", 3120, 38, 150, 380, 0.2, 1.0, 95, true],
        ["ap-south", 960, 77, 340, 820, 1.6, 1.9, 81, false],
      ],
    },
  ],
};

/** A submitted page with a block to copy: the copy button works on a read-only page. */
const submittedWithCopy: PageContent = {
  title: "Run the migration",
  body: [
    { type: "copyable", label: "Command", text: "pnpm --filter @octoboard/daemon migrate --dry-run" },
    { type: "text", name: "environment", label: "Environment", default: "staging" },
    { type: "choices", name: "decision", label: "Decision", options: [{ value: "run", label: "Run it" }, { value: "wait", label: "Wait" }] },
  ],
};

const visualHub = formHub("s-visual-components", "Visual components", "teal");
const tabsHub = formHub("s-visual-tabs", "Tabs", "lime");
const tablesHub = formHub("s-visual-tables", "Tables", "yellow");
const sortedHub = formHub("s-visual-table-sorted", "Table sorted and filtered", "cyan");
const noMatchHub = formHub("s-visual-table-no-match", "Table with no match", "violet");
const copiedHub = formHub("s-visual-copy", "Copy on a submitted page", "orange");
const visualPages = snapshotState({
  consoles: [console_],
  projects: [project],
  sessions: [visualHub, tabsHub, tablesHub, sortedHub, noMatchHub, copiedHub],
  pages: {
    [visualHub.id]: [pageOf("pv-components", visualHub.id, visualReport, 10)],
    [tabsHub.id]: [pageOf("pv-tabs", tabsHub.id, tabbedReport, 10)],
    [tablesHub.id]: [pageOf("pv-tables", tablesHub.id, tablesReport, 10)],
    [sortedHub.id]: [pageOf("pv-sorted", sortedHub.id, { title: "Open findings", body: [findingsTable] }, 10)],
    [noMatchHub.id]: [pageOf("pv-no-match", noMatchHub.id, { title: "Open findings", body: [findingsTable] }, 10)],
    [copiedHub.id]: [pageOf("pv-copy", copiedHub.id, submittedWithCopy, 30, { environment: "staging", decision: "run" })],
  },
});

export const reportVisualScenarios: Scenario[] = [
  {
    id: "report-visual-components",
    group: GROUP,
    category: "normal",
    title: "Page with stats, callouts, key-values, code, diff and copyable blocks",
    description: "Three stats side by side, each with its figure and change: a down arrow in green, an up arrow in red and a right arrow in grey. A plain rule separates them from a second row of columns: key-values beside a column of two stats with a labelled rule between them. A rule labelled Findings, its label at the start in small muted text, leads into callouts of each tone (information, success, then warning and error in a section headed Open risks with a plain rule between them, the last callout without a title). A rule whose label is long wraps it over several lines. Then a code block, a diff whose added lines are tinted green and removed lines red, and two copyable blocks, one monospace and one in the page's own font, on the code block's surface with the copy button over the top end corner of each. A line too long for the code, the diff and the monospace block scrolls sideways inside it, the copy button staying in its corner; the page's own font wraps. Choose Phone (390px) with the page's width control: the columns stack one under the other.",
    state: visualPages,
    steps: [openForm(visualHub.title)],
  },
  {
    id: "report-tabs",
    group: GROUP,
    category: "normal",
    title: "Page with tabs",
    description: "A tab strip of four tabs, the first selected, over its panel with key-values, a rule labelled Blocking and a callout; the last tab's label is long. Select the other tabs to see a diff, a code block and a copyable block in their own panels, whose longest line scrolls sideways in its block (the code and the diff) or wraps (the copyable block).",
    state: visualPages,
    steps: [openForm(tabsHub.title)],
  },
  {
    id: "report-tables",
    group: GROUP,
    category: "normal",
    title: "Page with tables",
    description: "Each table's caption is the label above it. The first has a filter field (placeholder Search) above it and a sort button in every column header, with numbers aligned to the end of their column, a yes or no for each boolean and empty cells left blank. The second has neither sorting nor filtering, and a column the page aligns to the end. The first and third are wider than the panel at its default width: each scrolls sideways inside its own frame while the rest of the page stays put, and is a tab stop only for as long as it overflows; the second fits and is not.",
    state: visualPages,
    steps: [openForm(tablesHub.title)],
  },
  {
    id: "report-table-sorted-filtered",
    group: GROUP,
    category: "normal",
    title: "Table sorted and filtered",
    description: "The table is sorted by Unresolved from the largest, its header showing the down arrow while the other headers keep the neutral arrow, and filtered by an owner's name: only that owner's rows remain, with \"Rows shown: 3 of 8\" under the filter field. Press Unresolved again to reverse the order.",
    state: visualPages,
    steps: [
      openForm(sortedHub.title),
      (ui) => ui.press("Unresolved"),
      (ui) => ui.press("Unresolved"),
      (ui) => ui.focus(ui.t("report.table.filterLabel")),
      (ui) => ui.type("mara"),
    ],
  },
  {
    id: "report-table-no-match",
    group: GROUP,
    category: "normal",
    title: "Table whose filter matches no row",
    description: "A filter that matches nothing leaves the headers over the application's empty state, a search icon and \"No rows match\" in a row of its own, with \"Rows shown: 0 of 8\" under the filter field.",
    state: visualPages,
    steps: [
      openForm(noMatchHub.title),
      (ui) => ui.focus(ui.t("report.table.filterLabel")),
      (ui) => ui.type("zzz"),
    ],
  },
  {
    id: "report-copy-confirmed",
    group: GROUP,
    category: "normal",
    title: "Copy button confirming on a submitted page",
    description: "The page is submitted and read-only, yet the copy button of its block still works: it was pressed, so its tooltip reads \"Copied\" and its icon is the check mark.",
    state: visualPages,
    clipboard: { text: "" },
    steps: [
      openForm(copiedHub.title),
      (ui) => ui.hoverTitledControl(ui.t("report.copyNamed", { label: "Command" })),
      (ui) => ui.press(ui.t("report.copyNamed", { label: "Command" })),
    ],
  },
];
