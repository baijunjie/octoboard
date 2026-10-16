import type { Ui } from "../interact";
import type { Scenario } from "../scenario";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Project filter";

const console_ = consoleOf("c-1", "Normal tags");
const projects = [
  projectOf("p-1", console_.id, "Website", { tags: ["frontend", "customer-facing"] }),
  projectOf("p-2", console_.id, "Search API", { tags: ["backend", "customer-facing"] }),
  projectOf("p-3", console_.id, "Billing Service", { tags: ["backend", "payments"] }),
  projectOf("p-4", console_.id, "Docs Site", { tags: ["frontend", "customer-facing", "docs", "a-long-tag-for-checking-project-interface-layout"] }),
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

// The matching project stays long enough for its session list to scroll while the Projects
// heading, with the conditions in force beside it, remains pinned.
const scrollingMatchState = {
  ...state,
  sessions: new Map([
    ...state.sessions!,
    ...Array.from({ length: 24 }, (_, i) => {
      const session = sessionOf(`s-docs-scroll-${i}`, console_.id, projects[3].id, `Review documentation section ${i + 1}`, "idle");
      return [session.id, session] as const;
    }),
  ]),
};

const openFilter = (ui: Ui) => ui.press(ui.t("sidebar.filter.open"));

export const filterScenarios: Scenario[] = [
  {
    id: "filter-popover",
    group: GROUP,
    category: "normal",
    title: "Project filter",
    description: "Selected tags have checkmarks; the normal vocabulary fits without scrolling. The popover stays open as conditions are picked. After closing it, the heading's Funnel X clears all conditions.",
    state,
    preferences: { sidebarConsole: console_.id },
    steps: [
      openFilter,
      (ui) => ui.press("customer-facing"),
      (ui) => ui.press("frontend"),
    ],
  },
  {
    id: "filter-in-force",
    group: GROUP,
    category: "normal",
    title: "Matching projects",
    description: "Projects (1) counts the single matching project. The keyword and the tag sit beside the heading, between its label and the Funnel X, which clears them together; they are not interactive. Scroll the project sessions: the heading and its conditions stay pinned.",
    state: scrollingMatchState,
    preferences: { sidebarConsole: console_.id },
    steps: [
      openFilter,
      (ui) => ui.type("site"),
      (ui) => ui.press("docs"),
      (ui) => ui.key("Escape"),
    ],
  },
  {
    id: "filter-overflowing-tags",
    group: GROUP,
    category: "normal",
    title: "Conditions wider than the heading",
    description: "The keyword and three tags do not fit beside the Projects label. They stay on one line at full width and fade at the clipped end; hover the strip and all of it runs as one marquee, and its tooltip lists the whole filter.",
    state,
    preferences: { sidebarConsole: console_.id },
    steps: [
      openFilter,
      (ui) => ui.type("site"),
      (ui) => ui.press("customer-facing"),
      (ui) => ui.press("docs"),
      (ui) => ui.press("a-long-tag-for-checking-project-interface-layout"),
      (ui) => ui.key("Escape"),
    ],
  },
  {
    id: "filter-no-match",
    group: GROUP,
    category: "normal",
    title: "No matching projects",
    description: "Projects (0) appears above the no-match row while the filter remains active. The heading's Funnel X clears the keyword and restores every project.",
    state: state,
    preferences: { sidebarConsole: console_.id },
    steps: [
      openFilter,
      (ui) => ui.type("no-such-project"),
      (ui) => ui.key("Escape"),
    ],
  },
  {
    id: "filter-many-tags",
    group: GROUP,
    category: "normal",
    title: "Scrollable tag picker",
    description: "The tag list scrolls at the popover edge while its tags keep the search field's inset.",
    state: state,
    preferences: { sidebarConsole: crowdedConsole.id },
    steps: [
      openFilter,
    ],
  },
  {
    id: "filter-no-tags",
    group: GROUP,
    category: "normal",
    title: "Empty tag picker",
    state: state,
    preferences: { sidebarConsole: plainConsole.id },
    steps: [
      openFilter,
    ],
  },
];
