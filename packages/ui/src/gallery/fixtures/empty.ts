import type { Scenario } from "../scenario";
import { consoleOf, projectOf, snapshotState } from "./builders";

const GROUP = "Empty";

export const emptyScenarios: Scenario[] = [
  {
    id: "empty-no-consoles",
    group: GROUP,
    title: "No consoles",
    description: "A fresh install: the sidebar offers New console and the terminal asks for a session.",
    state: snapshotState({}),
  },
  {
    id: "empty-console-no-projects",
    group: GROUP,
    title: "A console with no projects",
    state: snapshotState({ consoles: [consoleOf("c-1", "Main")] }),
  },
  {
    id: "empty-project-no-sessions",
    group: GROUP,
    title: "A project with no sessions",
    state: snapshotState({
      consoles: [consoleOf("c-1", "Main")],
      projects: [projectOf("p-1", "c-1", "Website")],
    }),
  },
  {
    id: "empty-filter-no-tags",
    group: GROUP,
    title: "Project filter with no tags in use",
    description: "No project carries a tag, so the filter's tag picker says where tags come from.",
    state: snapshotState({
      consoles: [consoleOf("c-1", "Main")],
      projects: [projectOf("p-1", "c-1", "Website"), projectOf("p-2", "c-1", "Search API")],
    }),
    steps: [(ui) => ui.press(ui.t("sidebar.filter.open"))],
  },
];
