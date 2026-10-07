import type { Scenario } from "../scenario";
import { SAMPLE, pageOf, snapshotState } from "./builders";

const GROUP = "Window layout";

const { console: console_, web, api, sessions } = SAMPLE;
const pages = [pageOf("pg-1", console_.id, "<h1>Report</h1><p>Nothing needs your attention.</p>", 5)];
const state = snapshotState({ consoles: [console_], projects: [web, api], sessions, pages: { [console_.id]: pages } });

export const layoutScenarios: Scenario[] = [
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
    id: "layout-sidebar-hidden",
    group: GROUP,
    title: "Docked sidebar hidden",
    description: "Hover the window's start edge to float it in.",
    width: 1440,
    preferences: { sidebarVisible: false },
    state,
    steps: [(ui) => ui.press(ui.session("Fix the summary layout"))],
  },
  {
    id: "layout-report-hidden",
    group: GROUP,
    title: "Docked report panel hidden",
    width: 1440,
    preferences: { reportVisible: false },
    state,
    steps: [(ui) => ui.press(ui.session("Hub"))],
  },
  {
    id: "layout-wide-panes",
    group: GROUP,
    title: "Widest panes",
    width: 1440,
    preferences: { sidebarWidth: 480, reportWidth: 720 },
    state,
    steps: [(ui) => ui.press(ui.session("Hub")), (ui) => ui.wait(500)],
  },
  {
    id: "layout-narrow-sidebar",
    group: GROUP,
    title: "Narrow window, sessions drawer open",
    width: 800,
    state,
    steps: [(ui) => ui.press(ui.t("titleBar.sidebar.show"))],
  },
  {
    id: "layout-narrow-report",
    group: GROUP,
    title: "Narrow window, report drawer open",
    width: 800,
    state,
    steps: [(ui) => ui.press(ui.t("titleBar.sidebar.show")), (ui) => ui.press(ui.session("Hub")), (ui) => ui.press(ui.t("titleBar.report.show")), (ui) => ui.wait(500)],
  },
];
