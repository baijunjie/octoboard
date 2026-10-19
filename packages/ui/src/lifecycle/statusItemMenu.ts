import type { Translate } from "../i18n/catalog";
import type { StatusItemEntry, StatusItemMenu, StatusItemSection } from "../platform";
import type { Console, Project, Session, SessionStatus } from "../protocol";
import { sessionWithLocation, statusLabel } from "../sessionLabel";
import { rankedStatus, sessionsInTreeOrder } from "../sidebar/order";
import { agentIconKey, statusIconKey } from "./statusItemIconKeys";
import { buildQuotaSubmenus, type StatusItemQuotaSource } from "./statusItemQuota";

/** The live statuses the menu lists, each under its own heading, most pressing first. */
const SECTION_ORDER: SessionStatus[] = ["waiting_user", "working", "idle"];

/** How many sessions a section names before the rest are summed up in one line, which keeps the
 * menu on screen however many sessions run. */
export const MAX_PER_SECTION = 10;

/** The menu before its pictures are drawn: lines name them by key (`statusItemIconKeys.ts`), and
 * `withIcons` (`statusItemIcons.ts`) draws the keys in use. */
export type StatusItemMenuDraft = Omit<StatusItemMenu, "icons">;

export interface StatusItemMenuSource {
  consoles: Console[];
  projects: Project[];
  sessions: Session[];
  consoleMap: Map<string, Console>;
  projectMap: Map<string, Project>;
  quota: StatusItemQuotaSource;
}

/** The live sessions — waiting for the user, working, awaiting instructions — in the sidebar's
 * order, each status under its own heading, then a quota submenu per available agent. The icon
 * raises its hand (`attention`) while any session waits for the user. */
export function buildStatusItemMenu(t: Translate, source: StatusItemMenuSource): StatusItemMenuDraft {
  const { consoles, projects, sessions, consoleMap, projectMap, quota } = source;
  const byStatus = SECTION_ORDER.map((status) => ({ status, listed: sessionsInTreeOrder(consoles, projects, sessions, (s) => rankedStatus(s) === status) }));
  const sections = byStatus.flatMap(({ status, listed }): StatusItemSection[] => {
    if (listed.length === 0) return [];
    const items: StatusItemEntry[] = listed
      .slice(0, MAX_PER_SECTION)
      .map((session) => ({
        label: sessionWithLocation(t, session, consoleMap, projectMap),
        icon: agentIconKey(session.agent),
        session: session.id,
      }));
    if (listed.length > MAX_PER_SECTION) items.push({ label: t("statusItem.more", { count: listed.length - MAX_PER_SECTION }) });
    return [{ heading: t("statusItem.section", { status: statusLabel(t, status), count: listed.length }), icon: statusIconKey(status), items }];
  });
  return {
    sections: sections.length > 0 ? sections : [{ heading: t("statusItem.noSessions"), items: [] }],
    submenus: buildQuotaSubmenus(t, quota),
    open: t("statusItem.open"),
    quit: t("menu.quit"),
    // Taken from the listed sessions, so the hand is never up over a menu with no waiting section.
    attention: byStatus.some(({ status, listed }) => status === "waiting_user" && listed.length > 0),
  };
}
