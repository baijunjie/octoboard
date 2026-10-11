import { createPersistedPreference } from "../persistedPreference";
import { PREFERENCE_KEYS } from "../preferenceKeys";

/** Whether closing the window leaves the app running in the background (the Settings switch the
 * desktop app offers) rather than quitting it. Off until the user turns it on; `useBackgroundRunSync`
 * hands the choice to the shell. */
export const runInBackground = createPersistedPreference<boolean>(
  PREFERENCE_KEYS.runInBackground,
  (raw) => raw === "true",
  (value) => String(value),
);
