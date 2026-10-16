import { createPersistedPreference } from "../persistedPreference";
import { PREFERENCE_KEYS } from "../preferenceKeys";

/** One row per file (`flat`), or files grouped by directory (`tree`). */
export type ChangeLayout = "flat" | "tree";

/** The legacy shared Git layout preference, used only to seed a project's independent Git tab
 * layouts until they are chosen explicitly. */
export const changeLayout = createPersistedPreference<ChangeLayout>(
  PREFERENCE_KEYS.changeLayout,
  (raw) => (raw === "tree" ? "tree" : "flat"),
  (value) => value,
);
