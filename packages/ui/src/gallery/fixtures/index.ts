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
import { sessionHistoryScenarios } from "./sessionHistory";
import { settingsScenarios } from "./settings";
import { structuredErrorScenarios } from "./structuredErrors";
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
  ...sessionHistoryScenarios,
  ...terminalPanelScenarios,
  ...noticeScenarios,
  ...structuredErrorScenarios,
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
    title: "Inline references, status badges, failure marks and alert tones",
    description: "Names, labels, files, paths and italic confirmation words take their text's font at 12px, 14px and 16px, with long-path wrapping and right-to-left isolation. The sizes differ only in padding, and each copy style shows the steps that fit its line: 12px text on 16px lines xs; 14px on 20px xs and sm; 16px on 24px xs, sm and md; 14px Alert titles on 24px lines up to lg; status badges cross five colours with the soft and bordered variants; failure marks show the danger and warning glyphs (press one for its reason). Six colors outside alerts, every color inside each alert tone, code blocks and nested tone priority follow; scroll to compare the complete palette.",
    codePalette: true,
  },
  {
    id: "semantic-colours",
    group: "Component colors",
    category: "normal",
    title: "Semantic colours: tokens, components and surfaces",
    description: "Each meaning (neutral, accent, success, warning, danger and help) as labelled swatches of its roles with their token names, then the roles that exist once (link, keyboard focus ring, selected toggle segment, solid buttons, +N/−M counts, secondary text), then the real components in each colour (status badges soft and bordered, chips in a sentence, activity and failure marks, Alerts) on a card, a dialog, a panel, a tooltip and a sidebar row resting and selected. Compare the same colour across surfaces; use the viewing controls for light and dark.",
    semanticColours: true,
  },
  ...PREVIEW_SCENARIOS,
];

/** A successful welcome view is the landing page, regardless of catalog ordering. */
export const DEFAULT_SCENARIO = SCENARIOS.find(({ id }) => id === "empty-no-consoles")!;
