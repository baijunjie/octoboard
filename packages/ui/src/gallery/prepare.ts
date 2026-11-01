import type { Language } from "../i18n/languages";
import { writeStartingPreferences } from "../fixtures/preferences";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";
import type { Scenario } from "./scenario";

/**
 * Sets what the app reads from `localStorage` when its modules load to the scenario's starting
 * point, dropping whatever an earlier scenario or the dev app left. The scenario's page runs it
 * before it loads the app: the modules that own these preferences read them as they are evaluated,
 * so this has to come first. The origin is shared with the dev app, whose own `octoboard.*`
 * preferences this resets. `markdownStyle` is the gallery's Markdown style control (the URL's `markdown`), or null
 * when it was not used; it stands over the scenario's own. It is passed on as typed, since the app reads a style it
 * does not know as its default and the module that lists the styles must not load before this has run.
 */
export function prepareStorage(scenario: Scenario, language: Language, theme: "light" | "dark", markdownStyle: string | null): void {
  try {
    for (const key of Object.keys(localStorage)) {
      if (key.startsWith("octoboard.")) localStorage.removeItem(key);
    }
    writeStartingPreferences(localStorage, { language, theme, preferences: scenario.preferences });
    if (markdownStyle) localStorage.setItem(PREFERENCE_KEYS.markdownStyle, markdownStyle);
  } catch {
    // Without storage the app starts from its defaults, in the system's language.
  }
}
