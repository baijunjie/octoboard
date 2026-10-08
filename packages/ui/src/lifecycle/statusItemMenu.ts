import type { Translate } from "../i18n/catalog";
import type { StatusItemEntry, StatusItemMenu, StatusItemSection } from "../platform";
import type { Console, Project, Session, SessionStatus } from "../protocol";
import { sessionWithLocation, statusLabel } from "../sessionLabel";
import { sessionsInTreeOrder } from "../sidebar/order";

/** The live statuses the menu lists, each under its own heading, most pressing first. */
const SECTION_ORDER: SessionStatus[] = ["waiting_user", "working", "idle"];

/** How many sessions a section names before the rest are summed up in one line, which keeps the
 * menu on screen however many sessions run. */
export const MAX_PER_SECTION = 10;

export interface StatusItemMenuSource {
  consoles: Console[];
  projects: Project[];
  sessions: Session[];
  consoleMap: Map<string, Console>;
  projectMap: Map<string, Project>;
}

/** The live sessions — waiting for the user, working, awaiting instructions — in the sidebar's
 * order, each status under its own heading. */
export function buildStatusItemMenu(t: Translate, source: StatusItemMenuSource): StatusItemMenu {
  const { consoles, projects, sessions, consoleMap, projectMap } = source;
  const sections = SECTION_ORDER.flatMap((status): StatusItemSection[] => {
    const listed = sessionsInTreeOrder(consoles, projects, sessions, (s) => s.status === status);
    if (listed.length === 0) return [];
    const items: StatusItemEntry[] = listed
      .slice(0, MAX_PER_SECTION)
      .map((session) => ({ label: sessionWithLocation(t, session, consoleMap, projectMap), session: session.id }));
    if (listed.length > MAX_PER_SECTION) items.push({ label: t("statusItem.more", { count: listed.length - MAX_PER_SECTION }) });
    return [{ heading: t("statusItem.section", { status: statusLabel(t, status), count: listed.length }), items }];
  });
  return {
    sections: sections.length > 0 ? sections : [{ heading: t("statusItem.noSessions"), items: [] }],
    open: t("statusItem.open"),
    quit: t("menu.quit"),
  };
}
