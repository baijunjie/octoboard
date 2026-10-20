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
import { handoffScenarios } from "./handoff";
import { layoutScenarios } from "./layout";
import { projectFilesScenarios } from "./projectFiles";
import { projectGitScenarios } from "./projectGit";
import { noticeScenarios } from "./notices";
import { quotaGatingScenarios } from "./quotaGating";
import { railQuotaScenarios } from "./railQuota";
import { reportScenarios } from "./report";
import { settingsScenarios } from "./settings";
import { startupScenarios } from "./startup";
import { terminalPanelScenarios } from "./terminalPanel";
import { viewerScenarios } from "./viewer";

export { GROUPS } from "./groups";

// Scenarios added only to show a change for review, kept untracked in `src/gallery/preview/` of the
// checkout that shows them. They import fixtures from their own modules: importing this one is a cycle
// the eager glob turns into an uninitialized `SCENARIOS`.
const PREVIEW_SCENARIOS = Object.values(
  import.meta.glob<Scenario[]>("../preview/*.{ts,tsx}", { eager: true, import: "default" }),
).flat();

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
  ...terminalPanelScenarios,
  ...noticeScenarios,
  ...archiveScenarios,
  ...reportScenarios,
  ...projectFilesScenarios,
  ...projectGitScenarios,
  ...settingsScenarios,
  ...dialogScenarios,
  ...handoffScenarios,
  ...quotaGatingScenarios,
  ...configurationScenarios,
  ...viewerScenarios,
  ...layoutScenarios,
  ...railQuotaScenarios,
  {
    id: "code-colors",
    group: "Component colors",
    category: "normal",
    title: "Inline chips, sizes and alert tones",
    description: "Names, labels, files, paths and italic confirmation words compare at the inherited 12px, 14px and 16px sizes, with long-path wrapping and right-to-left isolation. Four fixed metadata sizes, six colors outside alerts, every color inside each alert tone, code blocks and nested tone priority are all on the page; scroll to compare the complete palette.",
    codePalette: true,
  },
  ...PREVIEW_SCENARIOS,
];

/** A successful welcome view is the landing page, regardless of catalog ordering. */
export const DEFAULT_SCENARIO = SCENARIOS.find(({ id }) => id === "empty-no-consoles")!;
