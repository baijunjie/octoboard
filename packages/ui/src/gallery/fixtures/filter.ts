import type { Ui } from "../interact";
import type { Scenario } from "../scenario";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Project filter";

const console_ = consoleOf("c-1", "Normal tags");
const projects = [
  projectOf("p-1", console_.id, "Website", { tags: ["frontend", "customer-facing"] }),
  projectOf("p-2", console_.id, "Search API", { tags: ["backend", "customer-facing"] }),
  projectOf("p-3", console_.id, "Billing Service", { tags: ["backend", "payments"] }),
  projectOf("p-4", console_.id, "Docs Site", { tags: ["frontend", "customer-facing", "docs"] }),
  projectOf("p-5", console_.id, "Build Scripts"),
];
const sessions = projects.map((p) => sessionOf(`s-${p.id}`, console_.id, p.id, `Work on ${p.name}`, "idle"));


// A vocabulary too long for the tag picker's list to show at once, spread over the same projects.
const MANY_TAGS = [
  "frontend", "backend", "customer-facing", "internal", "payments", "docs", "infrastructure", "mobile", "desktop", "security",
  "analytics", "search", "billing", "onboarding", "accessibility", "performance", "legacy", "experiments", "tooling", "data",
  "design-system", "release", "monitoring", "compliance", "localization", "notifications", "integrations", "reporting", "storage", "support",
];
const crowdedConsole = consoleOf("c-crowded", "Crowded tags");
const plainConsole = consoleOf("c-plain", "No tags");
const state = snapshotState({
  consoles: [console_, crowdedConsole, plainConsole],
  projects: [
    ...projects,
    ...projects.map((p, projectIndex) => ({ ...p, id: `crowded-${p.id}`, console_id: crowdedConsole.id, tags: MANY_TAGS.filter((_, tagIndex) => tagIndex % projects.length === projectIndex) })),
    projectOf("p-plain-web", plainConsole.id, "Website"),
    projectOf("p-plain-api", plainConsole.id, "Search API"),
  ],
  sessions: [...sessions, ...sessions.map((session) => ({ ...session, id: `crowded-${session.id}`, console_id: crowdedConsole.id, project_id: `crowded-${session.project_id}` }))],
});

const openFilter = (ui: Ui) => ui.press(ui.t("sidebar.filter.open"));

export const filterScenarios: Scenario[] = [
  {
    id: "filter-popover",
    group: GROUP,
    title: "Project filtering and folding",
    description: "Normal tags has a picker whose entries fit. Open it and pick customer-facing and frontend for selected checks and wrapping chips. Type site and press Escape to see matching projects and wrapping chips; reopen and replace it with no-such-project for the empty match. Clear the filter to return to all projects. Filter frontend, close the popover and Collapse all; clearing the filter leaves only those matching projects collapsed, while the hidden projects remain expanded. Switch the rail to Crowded tags for a scrolling picker with edge fades; No tags shows the empty picker.",
    state,
    preferences: { sidebarConsole: console_.id },
    steps: [
      openFilter,
      (ui) => ui.press("frontend"),
      (ui) => ui.key("Escape"),
      (ui) => ui.press(ui.t("sidebar.collapseAll")),
      (ui) => ui.press(ui.t("sidebar.filter.clear")),
      (ui) => ui.wait(500),
    ],
  },
];
