import { createPersistedPreference, type PersistedPreference, type PreferenceScope } from "../preferences/persistedPreference";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";

/** Each side's remembered docking choice. Responsive floating does not rewrite it; the right
 * pane's open panel kinds are stored separately. */
export type DockedPanel = "sidebar" | "aside";

/** The panes' layout is each window's, so kept per daemon, and a daemon's first window starts from the application's.
 */
export const LAYOUT_SCOPE: PreferenceScope = { scope: "daemon", inherit: true };

export const visibility: Record<DockedPanel, PersistedPreference<boolean>> = {
  sidebar: createPersistedPreference(PREFERENCE_KEYS.sidebarVisible, (raw) => raw !== "false", String, LAYOUT_SCOPE),
  aside: createPersistedPreference(PREFERENCE_KEYS.asideDocked, (raw) => raw !== "false", String, LAYOUT_SCOPE),
};

/** Whether the docked `panel` is shown, and a setter that persists the choice. Visible unless the
 * user hid it; the state is shared module-wide. */
export function useDockedPanelVisible(panel: DockedPanel): [boolean, (value: boolean) => void] {
  const preference = visibility[panel];
  return [preference.useValue(), (next) => preference.set(next)];
}
