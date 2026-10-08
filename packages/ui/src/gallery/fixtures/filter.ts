import type { Ui } from "../interact";
import type { Scenario } from "../scenario";
import { consoleOf, projectOf, sessionOf, snapshotState } from "./builders";

const GROUP = "Project filter";

const console_ = consoleOf("c-1", "Main");
const projects = [
  projectOf("p-1", console_.id, "Website", { tags: ["frontend", "customer-facing"] }),
  projectOf("p-2", console_.id, "Search API", { tags: ["backend", "customer-facing"] }),
  projectOf("p-3", console_.id, "Billing Service", { tags: ["backend", "payments"] }),
  projectOf("p-4", console_.id, "Docs Site", { tags: ["frontend", "customer-facing", "docs"] }),
  projectOf("p-5", console_.id, "Build Scripts"),
];
const state = snapshotState({
  consoles: [console_],
  projects,
  sessions: projects.map((p) => sessionOf(`s-${p.id}`, console_.id, p.id, `Work on ${p.name}`, "idle")),
});

const openFilter = (ui: Ui) => ui.press(ui.t("sidebar.filter.open"));

export const filterScenarios: Scenario[] = [
  {
    id: "filter-popover",
    group: GROUP,
    title: "Filter popover with tags picked",
    description: "The keyword field and the tag picker, a picked tag marked with a check.",
    state,
    steps: [
      openFilter,
      (ui) => ui.press("customer-facing"),
      (ui) => ui.press("frontend"),
    ],
  },
  {
    id: "filter-in-force",
    group: GROUP,
    title: "Filter in force",
    description:
      "The keyword beside the heading and the picked tags after it, wrapping when they do not fit; only the matching projects remain.",
    state,
    steps: [
      openFilter,
      (ui) => ui.type("site"),
      (ui) => ui.press("customer-facing"),
      (ui) => ui.press("frontend"),
      // Escape closes the popover and keeps the filter.
      (ui) => ui.key("Escape"),
    ],
  },
  {
    id: "filter-collapse-all",
    group: GROUP,
    title: "Collapse all, with a filter in force",
    description:
      "Collapse all projects acts on the projects the list shows and on those alone, so the ones the filter hides keep the state they had; dropping the filter brings them back expanded.",
    state,
    steps: [
      openFilter,
      (ui) => ui.press("frontend"),
      (ui) => ui.key("Escape"),
      (ui) => ui.press(ui.t("sidebar.collapseAll")),
      (ui) => ui.press(ui.t("sidebar.filter.clear")),
      // The rows the filter brings back are still animating into place.
      (ui) => ui.wait(500),
    ],
  },
];
