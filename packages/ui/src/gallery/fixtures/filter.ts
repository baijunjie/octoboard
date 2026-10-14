import type { Ui } from "../interact";
import type { Scenario } from "../scenario";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Project filter";

const console_ = consoleOf("c-1", "Normal tags");
const projects = [
  projectOf("p-1", console_.id, "Website", { tags: ["frontend", "customer-facing"] }),
  projectOf("p-2", console_.id, "Search API", { tags: ["backend", "customer-facing"] }),
  projectOf("p-3", console_.id, "Billing Service", { tags: ["backend", "payments"] }),
  projectOf("p-4", console_.id, "Docs Site", { tags: ["frontend", "customer-facing", "docs", "x", "a-long-tag-for-checking-project-interface-layout"] }),
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
    title: "Project filter",
    description: "Selected tags have checkmarks; the normal vocabulary fits without scrolling.",
    state,
    preferences: { sidebarConsole: console_.id },
    steps: [
      openFilter,
      (ui) => ui.press("customer-facing"),
      (ui) => ui.press("frontend"),
    ],
  },
  {
    id:"filter-in-force",
    group:GROUP,
    title:"Matching projects",
    description:"The keyword, short tag and long tag fit their content at rest. Hover a short tag to reveal its remove button and minimum width.",
    state:state,
    preferences:{sidebarConsole:console_.id},
    steps: [
      openFilter,
      (ui) => ui.type("site"),
      (ui) => ui.press("x"),
      (ui) => ui.press("a-long-tag-for-checking-project-interface-layout"),
      (ui) => ui.key("Escape"),
    ],
  },
  {
    id:"filter-no-match",
    group:GROUP,
    title:"No matching projects",
    state:state,
    preferences:{sidebarConsole:console_.id},
    steps: [
      openFilter,
      (ui) => ui.type("no-such-project"),
      (ui) => ui.key("Escape"),
    ],
  },
  {
    id:"filter-many-tags",
    group:GROUP,
    title:"Scrollable tag picker",
    description:"The tag list scrolls at the popover edge while its tags keep the search field's inset.",
    state:state,
    preferences:{sidebarConsole:crowdedConsole.id},
    steps: [
      openFilter,
    ],
  },
  {
    id:"filter-no-tags",
    group:GROUP,
    title:"Empty tag picker",
    state:state,
    preferences:{sidebarConsole:plainConsole.id},
    steps: [
      openFilter,
    ],
  },
];
