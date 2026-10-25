import { createPersistedPreference } from "../persistedPreference";
import type { UpdateChannel } from "../platform";
import { PREFERENCE_KEYS } from "../preferenceKeys";

/** The release stream the application updates from (the Settings choice the desktop app offers). Beta until the
 * first stable release is out: a stored value other than `stable` reads as the default. The update store (run by
 * `UpdatesProvider`) looks for updates on whichever is chosen. */
export const updateChannel = createPersistedPreference<UpdateChannel>(
  PREFERENCE_KEYS.updateChannel,
  (raw) => (raw === "stable" ? "stable" : "beta"),
  (value) => value,
);
