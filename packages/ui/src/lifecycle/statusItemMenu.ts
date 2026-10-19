import type { Translate } from "../i18n/catalog";
import type { StatusItemEntry, StatusItemMenu, StatusItemSection } from "../platform";
import type { Console, Project, Session } from "../protocol";
import { remainingPercent } from "../quota/quotaFormat";
import { accountQuotaLabel, sessionQuotaWindowOf } from "../quota/sessionQuota";
import { sessionStatusKind, sessionWithLocation, statusLabel, type StatusKind } from "../sessionLabel";
import { sessionsInTreeOrder } from "../sidebar/order";
import { agentIconKey, statusIconKey } from "./statusItemIconKeys";
import { buildQuotaSubmenus, type StatusItemQuotaSource } from "./statusItemQuota";

/** The live statuses the menu lists, each under its own heading, most pressing first. A session
 * running in the background gets a heading of its own, as it has a glyph of its own in the sidebar. */
const SECTION_ORDER: StatusKind[] = ["waiting_user", "working", "background", "idle"];

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
  selectedSessionId?: string;
  currentConsoleId?: string;
}

/** The live sessions — waiting for the user, working, running in background, awaiting
 * instructions — in the sidebar's order, each status under its own heading, then a quota submenu
 * per available agent. The icon shows its attention dot (`attention`) while any session waits for
 * the user. */
export function buildStatusItemMenu(t: Translate, source: StatusItemMenuSource): StatusItemMenuDraft {
  const { consoles, projects, sessions, consoleMap, projectMap, quota } = source;
  const byStatus = SECTION_ORDER.map((status) => ({ status, listed: sessionsInTreeOrder(consoles, projects, sessions, (s) => sessionStatusKind(s) === status) }));
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
  const selected = sessions.find((session) => session.id === source.selectedSessionId);
  const window = sessionQuotaWindowOf(selected, quota);
  const accountName = quota.accounts.find((account) => account.id === selected?.account_id)?.name;
  return {
    ...(selected && window ? {
      quota_remaining: remainingPercent(window),
      quota_label: accountQuotaLabel(t, selected.agent, accountName, window, quota.defaultAccountNames),
    } : {}),
    sections: sections.length > 0 ? sections : [{ heading: t("statusItem.noSessions"), items: [] }],
    submenus: buildQuotaSubmenus(t, quota, consoleMap.get(source.currentConsoleId ?? "")),
    open: t("statusItem.open"),
    quit: t("menu.quit"),
    // Taken from the listed sessions, so the dot never shows over a menu with no waiting section.
    attention: byStatus.some(({ status, listed }) => status === "waiting_user" && listed.length > 0),
  };
}
