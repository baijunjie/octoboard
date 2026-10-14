import { createPersistedPreference } from "../persistedPreference";
import { PREFERENCE_KEYS } from "../preferenceKeys";

/** Whether every project focus view lists console-bound sessions alongside its own sessions. */
export const projectFocusIncludeConsoleSessions = createPersistedPreference<boolean>(
  PREFERENCE_KEYS.projectFocusIncludeConsoleSessions,
  (raw) => raw === "true",
  (value) => String(value),
);
