import { createPersistedPreference } from "../preferences/persistedPreference";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";

/** The id of the task the Scheduled tasks page last showed, which it opens on again while that task still exists. Kept
 * per daemon, as the window's other view state is, since the tasks are the daemon's. */
export const scheduledTaskPreference = createPersistedPreference<string | undefined>(
  PREFERENCE_KEYS.scheduledTask,
  (raw) => raw || undefined,
  (value) => value ?? null,
  { scope: "daemon" },
);
