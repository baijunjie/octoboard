import type { Translate } from "../i18n/catalog";
import type { StatusItemEntry, StatusItemMenu, StatusItemSection } from "../platform";
import type { Console, Project, Session } from "../protocol";
import { remainingPercentAt } from "../quota/quotaFormat";
import { accountQuotaLabel, sessionQuotaBarOf } from "../quota/sessionQuota";
import { sessionStatusKind, sessionWithLocation, statusLabel, type StatusKind } from "../sessionLabel";
import { sessionsInTreeOrder, STATUS_RANK } from "../sidebar/order";
import { agentIconKey, statusIconKey } from "./statusItemIconKeys";
import { buildQuotaSubmenus, type StatusItemQuotaSource } from "./statusItemQuota";

/** The live statuses the menu lists, each under its own heading, in the sidebar's status order
 * (`STATUS_RANK`). A session running in the background gets a heading of its own, as it has a glyph
 * of its own in the sidebar. */
const SECTION_ORDER = (["waiting_user", "idle", "background", "working"] satisfies StatusKind[]).sort((a, b) => STATUS_RANK[a] - STATUS_RANK[b]);

/** How many sessions a section names before the rest are summed up in one line, which keeps the
 * menu on screen however many sessions run. */
export const MAX_PER_SECTION = 10;

/** How wide a session's line may be, in display columns (East Asian wide characters count as two): the
 * system cuts a menu line that is wider than the screen in its middle, and a menu that wide is beyond what the
 * quota bars' placement can follow. */
export const MAX_SESSION_LABEL_COLUMNS = 60;

/** The menu before its pictures are drawn: lines name them by key (`statusItemIconKeys.ts`), and
 * `withIcons` (`statusItemIcons.ts`) draws the keys in use. */
export type StatusItemMenuDraft = Omit<StatusItemMenu, "icons">;

export interface StatusItemMenuSource {
  consoles: Console[];
  projects: Project[];
  sessions: Session[];
  consoleMap: Map<string, Console>;
  projectMap: Map<string, Project>;
  /** Whether the sidebar ranks by status (`ordersByStatus`), which this menu's order follows. */
  byStatus: boolean;
  quota: StatusItemQuotaSource;
  selectedSessionId?: string;
  currentConsoleId?: string;
}

/** The live sessions in the sidebar's order, each status under its own heading in the sidebar's
 * status order (`SECTION_ORDER`), then a quota submenu per available agent. The icon shows its
 * attention dot (`attention`) while any session waits for the user. */
export function buildStatusItemMenu(t: Translate, source: StatusItemMenuSource): StatusItemMenuDraft {
  const { consoles, projects, sessions, consoleMap, projectMap, quota } = source;
  const perStatus = SECTION_ORDER.map((status) => ({ status, listed: sessionsInTreeOrder(consoles, projects, sessions, (s) => sessionStatusKind(s) === status, source.byStatus) }));
  const sections = perStatus.flatMap(({ status, listed }): StatusItemSection[] => {
    if (listed.length === 0) return [];
    const items: StatusItemEntry[] = listed
      .slice(0, MAX_PER_SECTION)
      .map((session) => ({
        label: sessionWithLocation(t, session, consoleMap, projectMap, MAX_SESSION_LABEL_COLUMNS),
        icon: agentIconKey(session.agent),
        session: session.id,
      }));
    if (listed.length > MAX_PER_SECTION) items.push({ label: t("statusItem.more", { count: listed.length - MAX_PER_SECTION }) });
    return [{ heading: t("statusItem.section", { status: statusLabel(t, status), count: listed.length }), icon: statusIconKey(status), items }];
  });
  const selected = sessions.find((session) => session.id === source.selectedSessionId);
  const bar = sessionQuotaBarOf(selected, quota);
  const accountName = quota.accounts.find((account) => account.id === selected?.account_id)?.name;
  return {
    ...(selected && bar ? {
      quota_remaining: remainingPercentAt(bar.main, quota.now),
      ...(bar.thin ? { quota_weekly_remaining: remainingPercentAt(bar.thin, quota.now) } : {}),
      quota_label: accountQuotaLabel(t, selected.agent, accountName, bar, quota.defaultAccountNames, quota.now),
    } : {}),
    sections: sections.length > 0 ? sections : [{ heading: t("statusItem.noSessions"), items: [] }],
    submenus: buildQuotaSubmenus(t, quota, consoleMap.get(source.currentConsoleId ?? "")),
    open: t("statusItem.open"),
    quit: t("menu.quit"),
    // Taken from the listed sessions, so the dot never shows over a menu with no waiting section.
    attention: perStatus.some(({ status, listed }) => status === "waiting_user" && listed.length > 0),
  };
}
