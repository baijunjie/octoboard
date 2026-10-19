import { createPersistedPreference } from "../persistedPreference";
import { PREFERENCE_KEYS } from "../preferenceKeys";
import { parseStacks, serializeStacks, type OwnerKind, type PanelStack, type PanelStacks } from "./stackState";

/** The right pane's old visibility preference, which a pane the user had hidden carries over as no
 * panel open. Read once, when no stack is stored yet. */
function legacyHidden(): boolean {
  try {
    return localStorage.getItem(PREFERENCE_KEYS.asideVisible) === "false";
  } catch {
    return false;
  }
}

const stacksPreference = createPersistedPreference<PanelStacks>(
  PREFERENCE_KEYS.panelStacks,
  (raw) => parseStacks(raw, raw === null && legacyHidden()),
  serializeStacks,
);

/** The right pane's stacks of panels, one per owner, shared by everything that shows or changes
 * them (the top bar's toggles, the F6 cycle, the pane itself), and a setter. The open kinds and
 * their shares are kept across reloads, per client; `persist: false` is for a drag that sets on
 * every pointer move and writes once, at the end (`persist`). */
export function usePanelStacks(): {
  stacks: PanelStacks;
  setStack: (owner: OwnerKind, stack: PanelStack, options?: { persist?: boolean }) => void;
  persist: () => void;
} {
  const stacks = stacksPreference.useValue();
  return {
    stacks,
    setStack: (owner, stack, options) => stacksPreference.set({ ...stacksPreference.get(), [owner]: stack }, options),
    persist: stacksPreference.persist,
  };
}
