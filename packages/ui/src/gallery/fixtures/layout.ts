import type { Scenario } from "../scenario";
import { SAMPLE, consoleOf, pageOf, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Window layout";

const { console: console_, web, api, consoleSession, sessions } = SAMPLE;
const pages = [pageOf("pg-1", consoleSession.id, "<h1>Report</h1><p>Nothing needs your attention.</p>", 5)];
const state = snapshotState({ consoles: [console_], projects: [web, api], sessions, pages: { [consoleSession.id]: pages } });

// Five consoles, each in a different state, for what the rail shows per console: a raised hand
// (Main, which has two waiting sessions, so the waiting count reads 2), work in progress (Operations),
// a session merely running (Billing), and nothing going on (Docs, Lab).
const main = consoleOf("c-main", "Main");
const operations = consoleOf("c-ops", "Operations");
const billing = consoleOf("c-billing", "Billing");
const docs = consoleOf("c-docs", "Docs");
const lab = consoleOf("c-lab", "A console with a name that is far too long for the rail");
const website = projectOf("p-web", main.id, "Website");
const deploys = projectOf("p-deploys", operations.id, "Deploys");
const invoices = projectOf("p-invoices", billing.id, "Invoices");
const manual = projectOf("p-manual", docs.id, "User manual");
const hub = sessionOf("s-hub", main.id, undefined, "Hub", "waiting_user", { colour: "teal" });
const railState = snapshotState({
  consoles: [main, operations, billing, docs, lab],
  projects: [website, deploys, invoices, manual],
  sessions: [
    hub,
    sessionOf("s-web-1", main.id, website.id, "Fix the summary layout", "working"),
    sessionOf("s-web-2", main.id, website.id, "Update the dependencies", "idle"),
    sessionOf("s-web-3", main.id, website.id, "Asks which region to use", "waiting_user"),
    sessionOf("s-ops-hub", operations.id, undefined, "Operations hub", "idle", { colour: "azure" }),
    sessionOf("s-ops-1", operations.id, deploys.id, "Roll back the last deploy", "working"),
    sessionOf("s-bill-1", billing.id, invoices.id, "Reconcile March", "idle"),
    sessionOf("s-docs-1", docs.id, manual.id, "Old draft", "archived"),
  ],
  pages: { [hub.id]: [pageOf("pg-hub", hub.id, "<h1>Report</h1><p>Two sessions need you.</p>", 5)] },
});
/** The rail's button for a console: its accessible name starts with the console's name. */
const consoleButton = (name: string) => (label: string) => label.startsWith(name);

export const layoutScenarios: Scenario[] = [
  {
    id: "layout-rail-consoles",
    group: GROUP,
    title: "Rail with five consoles",
    description:
      "Console avatars on the left rail with their activity marker (a hand, a working dot, a speech bubble, none), the current one on a tile; New console under them. At the bottom: the waiting count, the report toggle for the console session, Settings. Right-click an avatar for its console's actions.",
    width: 1440,
    state: railState,
    steps: [(ui) => ui.press(ui.session("Hub")), (ui) => ui.wait(500)],
  },
  {
    id: "layout-history",
    group: GROUP,
    title: "Back and Forward available",
    description: "Four locations visited, then Back: both buttons in the top bar are enabled.",
    width: 1440,
    state: railState,
    steps: [
      (ui) => ui.press(ui.session("Hub")),
      (ui) => ui.press(ui.session("Fix the summary layout")),
      (ui) => ui.press(consoleButton("Operations")),
      (ui) => ui.press(ui.session("Roll back the last deploy")),
      (ui) => ui.press(ui.t("titleBar.back")),
      (ui) => ui.wait(300),
    ],
  },
  {
    id: "layout-rail-narrow",
    group: GROUP,
    title: "Narrow window, sessions drawer open",
    description: "The rail and the top bar stay; the sidebar is a drawer over the terminal, starting right of the rail.",
    width: 800,
    state: railState,
    steps: [(ui) => ui.press(ui.t("titleBar.sidebar.show")), (ui) => ui.wait(400)],
  },
  {
    id: "layout-rail-narrow-report",
    group: GROUP,
    title: "Narrow window, report drawer open",
    width: 800,
    state: railState,
    steps: [
      (ui) => ui.press(ui.t("titleBar.sidebar.show")),
      (ui) => ui.press(ui.session("Hub")),
      (ui) => ui.press(ui.t("rail.report.show")),
      (ui) => ui.wait(500),
    ],
  },
  {
    id: "layout-sidebar-hover-console",
    group: GROUP,
    title: "Docked sidebar hidden, five consoles",
    description:
      "The breadcrumb follows the buttons when the sidebar is not docked. Main is current. Rest the mouse on an avatar to float the sidebar in showing that console, current or not; move along the avatars to switch the preview, and press inside it to make that console current.",
    width: 1440,
    preferences: { sidebarVisible: false, sidebarConsole: main.id },
    state: railState,
    steps: [(ui) => ui.press(ui.session("Hub"))],
  },
  {
    id: "layout-wide-console-session",
    group: GROUP,
    title: "Wide window, console session selected",
    description: "Sidebar, terminal and the report panel docked side by side.",
    width: 1440,
    state,
    steps: [(ui) => ui.press(ui.session("Hub")), (ui) => ui.wait(500)],
  },
  {
    id: "layout-report-hidden",
    group: GROUP,
    title: "Docked report panel hidden",
    width: 1440,
    preferences: { asideVisible: false },
    state,
    steps: [(ui) => ui.press(ui.session("Hub"))],
  },
  {
    id: "layout-wide-panes",
    group: GROUP,
    title: "Widest panes",
    width: 1440,
    preferences: { sidebarWidth: 480, asideWidth: 720 },
    state,
    steps: [(ui) => ui.press(ui.session("Hub")), (ui) => ui.wait(500)],
  },
];
