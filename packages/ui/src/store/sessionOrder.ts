import { createPersistedPreference } from "../preferences/persistedPreference";
import { PREFERENCE_KEYS } from "../preferences/preferenceKeys";
import { ACTIVE_PROJECT_SORTS, DEFAULT_SESSION_ORDER, INACTIVE_PROJECT_SORTS, type SessionOrder, SESSION_SORTS } from "../sessions/order";

/** The stored text as an order, each part falling back to its default when it is missing or is a choice
 * this build does not know. */
export function parseSessionOrder(raw: string | null): SessionOrder {
  try {
    const parsed: unknown = raw === null ? undefined : JSON.parse(raw);
    const stored = typeof parsed === "object" && parsed !== null ? (parsed as Partial<Record<keyof SessionOrder, unknown>>) : {};
    return {
      sessions: SESSION_SORTS.find((sort) => sort === stored.sessions) ?? DEFAULT_SESSION_ORDER.sessions,
      activeProjects: ACTIVE_PROJECT_SORTS.find((sort) => sort === stored.activeProjects) ?? DEFAULT_SESSION_ORDER.activeProjects,
      inactiveProjects: INACTIVE_PROJECT_SORTS.find((sort) => sort === stored.inactiveProjects) ?? DEFAULT_SESSION_ORDER.inactiveProjects,
    };
  } catch {
    return DEFAULT_SESSION_ORDER;
  }
}

/** Only the parts that differ from the default are stored, so a later build that changes a default reaches
 * whoever never chose that part; nothing is stored when every part is the default. */
export function serializeSessionOrder(order: SessionOrder): string | null {
  const chosen = Object.fromEntries((Object.keys(DEFAULT_SESSION_ORDER) as (keyof SessionOrder)[]).filter((key) => order[key] !== DEFAULT_SESSION_ORDER[key]).map((key) => [key, order[key]]));
  return Object.keys(chosen).length === 0 ? null : JSON.stringify(chosen);
}

/** How sessions and projects are ordered, shared by every console and every view: the console view,
 * project focus, a console session's focus, and what orders them outside the views themselves (the menu bar
 * menu, the jump to the next waiting session, the lists in the delete dialogs). Set in Settings. */
export const sessionOrderPreference = createPersistedPreference(PREFERENCE_KEYS.sessionOrder, parseSessionOrder, serializeSessionOrder);
