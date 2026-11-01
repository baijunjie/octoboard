import type { Language } from "../i18n/languages";
import { writeStartingPreferences } from "../fixtures/preferences";
import type { Scenario } from "./scenario";

/**
 * Sets what the app reads from `localStorage` when its modules load to the scenario's starting
 * point, dropping whatever an earlier scenario or the dev app left. The scenario's page runs it
 * before it loads the app: the modules that own these preferences read them as they are evaluated,
 * so this has to come first. The origin is shared with the dev app, whose own `octoboard.*`
 * preferences this resets.
 */
export function prepareStorage(scenario: Scenario, language: Language, theme: "light" | "dark"): void {
  try {
    for (const key of Object.keys(localStorage)) {
      if (key.startsWith("octoboard.")) localStorage.removeItem(key);
    }
    writeStartingPreferences(localStorage, { language, theme, preferences: scenario.preferences });
  } catch {
    // Without storage the app starts from its defaults, in the system's language.
  }
}
