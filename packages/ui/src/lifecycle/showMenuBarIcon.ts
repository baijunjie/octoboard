import { createPersistedPreference } from "../preferences/persistedPreference";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";

/** Whether the desktop app shows its menu bar icon (the notification area icon on Windows): the Settings switch the
 * desktop app offers. On until the user turns it off; `useMenuBarIconSync` hands the choice to the shell. */
export const showMenuBarIcon = createPersistedPreference<boolean>(
  PREFERENCE_KEYS.showMenuBarIcon,
  (raw) => raw !== "false",
  (value) => String(value),
);
