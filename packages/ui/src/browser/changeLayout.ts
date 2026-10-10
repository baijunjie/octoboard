import { createPersistedPreference } from "../persistedPreference";
import { PREFERENCE_KEYS } from "../preferenceKeys";

/** How the Git mode's change lists show their changes: one row per change (`flat`), or grouped by
 * directory (`tree`). */
export type ChangeLayout = "flat" | "tree";

/** The layout the change lists open in, for the Uncommitted and the Compare view alike: the user's
 * last choice in the Git mode's toggle, kept across projects and restarts. Flat until they choose. */
export const changeLayout = createPersistedPreference<ChangeLayout>(
  PREFERENCE_KEYS.changeLayout,
  (raw) => (raw === "tree" ? "tree" : "flat"),
  (value) => value,
);
