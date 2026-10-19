import { createPersistedPreference, type PersistedPreference } from "../persistedPreference";
import { PREFERENCE_KEYS } from "../preferenceKeys";

/** The panes the user can hide at and above the `docked` breakpoint. Below it the sidebar is a
 * drawer, whose open state `usePaneToggles` holds instead. The aside is not one: whether it shows
 * follows from its open panels (`panels/stackState.ts`). */
export type DockedPanel = "sidebar";

const visibility: Record<DockedPanel, PersistedPreference<boolean>> = {
  sidebar: createPersistedPreference(PREFERENCE_KEYS.sidebarVisible, (raw) => raw !== "false", String),
};

/** Whether the docked `panel` is shown, and a setter that persists the choice. Visible unless the
 * user hid it; the state is shared module-wide. */
export function useDockedPanelVisible(panel: DockedPanel): [boolean, (value: boolean) => void] {
  const preference = visibility[panel];
  return [preference.useValue(), (next) => preference.set(next)];
}
