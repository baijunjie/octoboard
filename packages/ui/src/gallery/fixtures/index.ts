import type { Scenario } from "../scenario";
import { activityScenarios } from "./activity";
import { archiveScenarios } from "./archive";
import { busyScenarios } from "./busy";
import { configurationScenarios } from "./configuration";
import { connectionScenarios } from "./connection";
import { consoleSessionsScenarios } from "./consoleSessions";
import { dialogScenarios } from "./dialogs";
import { emptyScenarios } from "./empty";
import { filterScenarios } from "./filter";
import { focusScenarios } from "./focus";
import { gitScenarios } from "./git";
import { layoutScenarios } from "./layout";
import { projectFilesScenarios } from "./projectFiles";
import { projectGitScenarios } from "./projectGit";
import { noticeScenarios } from "./notices";
import { reportScenarios } from "./report";
import { settingsScenarios } from "./settings";
import { startupScenarios } from "./startup";
import { viewerScenarios } from "./viewer";

export { GROUPS } from "./groups";

/** The catalog keeps stable deep links; navigation orders each function by category. */
export const SCENARIOS: Scenario[] = [
  ...emptyScenarios,
  ...busyScenarios,
  ...activityScenarios,
  ...focusScenarios,
  ...consoleSessionsScenarios,
  ...gitScenarios,
  ...filterScenarios,
  ...startupScenarios,
  ...connectionScenarios,
  ...noticeScenarios,
  ...archiveScenarios,
  ...reportScenarios,
  ...projectFilesScenarios,
  ...projectGitScenarios,
  ...settingsScenarios,
  ...dialogScenarios,
  ...configurationScenarios,
  ...viewerScenarios,
  ...layoutScenarios,
  {
    id: "code-colors",
    group: "Component colors",
    category: "normal",
    title: "Code colors and alert tones",
    description: "Six code colors outside alerts, all requested colors inside each alert tone, wrapping blocks, nested tone priority and a code token after the alerts. Scroll to compare the complete palette; theme and language controls show the same view in dark and right-to-left layouts.",
    codePalette: true,
  },
];

/** A successful welcome view is the landing page, regardless of catalog ordering. */
export const DEFAULT_SCENARIO = SCENARIOS.find(({ id }) => id === "empty-no-consoles")!;
