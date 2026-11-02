import type { Translate } from "../i18n/catalog";
import type { StatusItemEntry, StatusItemMenu, StatusItemSection } from "../platform";
import type { Console, Project, Session } from "../protocol";
import { remainingPercentAt } from "../accounts/quotaFormat";
import { accountInUse, accountQuotaLabel, sessionQuotaBarOf } from "../quota/sessionQuota";
import { sessionWithLocation, statusLabel } from "../sessions/sessionLabel";
import { sessionShownStatusKind, SUMMARY_STATUSES } from "../sessions/sessionStatus";
import { sessionsInTreeOrder } from "../sessions/order";
import { agentIconKey, statusIconKey } from "./statusItemIconKeys";
import { buildQuotaSubmenus, type StatusItemQuotaSource } from "./statusItemQuota";

/** How many sessions a section names before the rest are summed up in one line, which keeps the
 * menu on screen however many sessions run: at most this many lines, and one for the rest, in each
 * section. */
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
  /** The ids of the sessions with delegated work in progress (`delegatingSessionIds`). */
  delegating: ReadonlySet<string>;
  /** Whether the sidebar ranks by status (`ordersByStatus`), which this menu's order follows. */
  byStatus: boolean;
  quota: StatusItemQuotaSource;
  selectedSessionId?: string;
  currentConsoleId?: string;
}

/** The sessions in summary statuses, in the sidebar's order,
 * each shown status under its own heading, then a quota submenu per available agent. The icon shows its
 * attention dot (`attention`) while any session waits for the user or is dormant. */
export function buildStatusItemMenu(t: Translate, source: StatusItemMenuSource): StatusItemMenuDraft {
  const { consoles, projects, sessions, consoleMap, projectMap, quota } = source;
  const perStatus = SUMMARY_STATUSES.map((status) => ({ status, listed: sessionsInTreeOrder(consoles, projects, sessions, (s) => sessionShownStatusKind(s, source.delegating) === status, source.delegating, source.byStatus) }));
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
    sections: sections.length > 0 ? sections : [{ heading: t("statusItem.noListedSessions"), items: [] }],
    submenus: buildQuotaSubmenus(t, quota, consoleMap.get(source.currentConsoleId ?? ""), accountInUse(selected)),
    open: t("statusItem.open"),
    quit: t("menu.quit"),
    attention: sessionsInTreeOrder(consoles, projects, sessions, (session) => {
      const status = sessionShownStatusKind(session, source.delegating);
      return status === "waiting_user" || status === "dormant";
    }, source.delegating, source.byStatus).length > 0,
  };
}
