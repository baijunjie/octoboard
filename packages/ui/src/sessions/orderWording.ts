import type { PlainMessageKey, Translate } from "../i18n/catalog";
import type { ActiveProjectSort, InactiveProjectSort, SessionOrder, SessionSort } from "./order";

export type SortKey = SessionSort | ActiveProjectSort | InactiveProjectSort;

/** The word the settings page and the import cards give a choice of the session order. */
export function sortLabel(t: Translate, sort: SortKey): string {
  switch (sort) {
    case "status": return t("settings.sort.option.status");
    case "started": return t("settings.sort.option.started");
    case "name": return t("settings.sort.option.name");
    case "used": return t("settings.sort.option.used");
  }
}

/** How the settings page names each part of the session order, and what it says of it. */
export const SESSION_ORDER_WORDS: Record<keyof SessionOrder, { label: PlainMessageKey; description: PlainMessageKey }> = {
  sessions: { label: "settings.sort.sessions.label", description: "settings.sort.sessions.description" },
  activeProjects: { label: "settings.sort.activeProjects.label", description: "settings.sort.activeProjects.description" },
  inactiveProjects: { label: "settings.sort.inactiveProjects.label", description: "settings.sort.inactiveProjects.description" },
};

/** The parts of the session order in the order the settings page lists them, each with its name. */
export const SESSION_ORDER_PARTS: readonly { field: keyof SessionOrder; label: PlainMessageKey }[] = (
  ["sessions", "activeProjects", "inactiveProjects"] as const
).map((field) => ({ field, label: SESSION_ORDER_WORDS[field].label }));
