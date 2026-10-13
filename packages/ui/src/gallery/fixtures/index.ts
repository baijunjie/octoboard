import type { Scenario } from "../scenario";
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

/** Every scenario; within a group the gallery lists them in this order, and the first entry is
 * the one the gallery opens on when the URL names no scenario, which is why this order does not
 * follow `GROUPS`: an empty state makes a better landing page than a startup failure. */
export const SCENARIOS: Scenario[] = [
  ...emptyScenarios,
  ...busyScenarios,
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
];
