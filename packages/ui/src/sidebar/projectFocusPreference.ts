import { createPersistedPreference } from "../preferences/persistedPreference";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";

/** Whether every project focus view lists console-bound sessions alongside its own sessions. */
export const projectFocusIncludeConsoleSessions = createPersistedPreference<boolean>(
  PREFERENCE_KEYS.projectFocusIncludeConsoleSessions,
  (raw) => raw === "true",
  (value) => String(value),
);
