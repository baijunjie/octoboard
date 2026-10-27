import { useMemo } from "react";

import { createPersistedPreference } from "../persistedPreference";
import { PREFERENCE_KEYS } from "../preferenceKeys";
import { OWNER_KINDS, parseStacks, serializeStacks, shownKinds, type OwnerKind, type PanelStack, type PanelStacks } from "./stackState";

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

/** How many sessions' closed Report panels are remembered: the most recent ones, so that sessions
 * that are gone do not pile up in storage for good. */
const MAX_CLOSED_REPORTS = 200;

/** The project sessions whose Report panel the user closed, as ids, oldest first. A session not in
 * it has the panel open (the default, as it is for a console session). */
const closedReportsPreference = createPersistedPreference<readonly string[]>(
  PREFERENCE_KEYS.closedReportPanels,
  (raw) => {
    try {
      const value: unknown = raw === null ? [] : JSON.parse(raw);
      return Array.isArray(value) ? value.filter((id): id is string => typeof id === "string").slice(-MAX_CLOSED_REPORTS) : [];
    } catch {
      return [];
    }
  },
  (ids) => (ids.length > 0 ? JSON.stringify(ids) : null),
);

/**
 * The right pane's stacks of panels, one per owner, shared by everything that shows or changes
 * them (the top bar's toggles, the F6 cycle, the pane itself), and a setter. The open kinds and
 * their shares are kept across reloads, per client; `persist: false` is for a drag that sets on
 * every pointer move and writes once, at the end (`persist`).
 *
 * A project's stack is shared by all its sessions, except for the Report panel, which belongs to a
 * project session: `reportSession` is the one whose Report panel the project's stack carries now
 * (the selected one, while it has pages; none otherwise, and then the stack has no Report panel).
 * Whether that panel is open is kept for the session (`closedReportsPreference`), and the stack
 * seen and set here has it in its `open` like any other kind, split off again when set, so callers
 * treat all of a project's kinds alike. The stored stack never holds it.
 */
export function usePanelStacks(reportSession?: string): {
  stacks: PanelStacks;
  setStack: (owner: OwnerKind, stack: PanelStack, options?: { persist?: boolean }) => void;
  persist: () => void;
} {
  const stored = stacksPreference.useValue();
  const closedReports = closedReportsPreference.useValue();
  const reportOpen = reportSession !== undefined && !closedReports.includes(reportSession);
  const stacks = useMemo(() => {
    if (!reportOpen) return stored;
    const open = OWNER_KINDS.project.filter((kind) => kind === "report" || stored.project.open.includes(kind));
    return { ...stored, project: { ...stored.project, open } };
  }, [stored, reportOpen]);
  return {
    stacks,
    setStack: (owner, stack, options) => {
      let kept = stack;
      if (owner === "project" && reportSession !== undefined) {
        const open = shownKinds(stack, OWNER_KINDS.project).includes("report");
        if (open !== reportOpen) {
          const others = closedReportsPreference.get().filter((id) => id !== reportSession);
          closedReportsPreference.set(open ? others : [...others, reportSession].slice(-MAX_CLOSED_REPORTS));
        }
        kept = { ...stack, open: stack.open.filter((kind) => kind !== "report") };
      }
      stacksPreference.set({ ...stacksPreference.get(), [owner]: kept }, options);
    },
    persist: stacksPreference.persist,
  };
}
