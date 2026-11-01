import { createPersistedPreference } from "../preferences/persistedPreference";
import type { UpdateChannel } from "../platform";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";

/** The release stream the application updates from (the Settings choice the desktop app offers). Stable unless the
 * user chose Beta: a stored value other than `beta` reads as the default, so a saved choice of either channel is kept.
 * The update store (run by `UpdatesProvider`) looks for updates on whichever is chosen. */
export const updateChannel = createPersistedPreference<UpdateChannel>(
  PREFERENCE_KEYS.updateChannel,
  (raw) => (raw === "beta" ? "beta" : "stable"),
  (value) => value,
);
