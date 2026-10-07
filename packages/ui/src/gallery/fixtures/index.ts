import type { Scenario } from "../scenario";
import { archiveScenarios } from "./archive";
import { busyScenarios } from "./busy";
import { connectionScenarios } from "./connection";
import { dialogScenarios } from "./dialogs";
import { emptyScenarios } from "./empty";
import { filterScenarios } from "./filter";
import { focusScenarios } from "./focus";
import { gitScenarios } from "./git";
import { layoutScenarios } from "./layout";
import { noticeScenarios } from "./notices";
import { reportScenarios } from "./report";
import { settingsScenarios } from "./settings";
import { startupScenarios } from "./startup";
import { statusScenarios } from "./statuses";

/** Every scenario, in the order the gallery lists them; a group is its scenarios' shared `group`. */
export const SCENARIOS: Scenario[] = [
  ...emptyScenarios,
  ...busyScenarios,
  ...statusScenarios,
  ...focusScenarios,
  ...gitScenarios,
  ...filterScenarios,
  ...connectionScenarios,
  ...noticeScenarios,
  ...archiveScenarios,
  ...reportScenarios,
  ...settingsScenarios,
  ...dialogScenarios,
  ...layoutScenarios,
  ...startupScenarios,
];
