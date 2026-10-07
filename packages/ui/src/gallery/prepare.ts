import type { Language } from "../i18n/languages";
import { PREFERENCE_KEYS } from "../preferenceKeys";
import type { Preferences, Scenario } from "./scenario";

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
    localStorage.setItem("heroui-theme", theme);
    localStorage.setItem(PREFERENCE_KEYS.language, language);
    for (const [name, value] of Object.entries(scenario.preferences ?? {})) {
      if (value !== undefined) localStorage.setItem(PREFERENCE_KEYS[name as keyof Preferences], String(value));
    }
  } catch {
    // Without storage the app starts from its defaults, in the system's language.
  }
}
