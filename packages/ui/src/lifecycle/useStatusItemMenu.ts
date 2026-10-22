import { useEffect, useRef } from "react";

import { useLatest } from "../components/useLatest";
import { useCurrentLanguage, useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import type { Session } from "../protocol";
import { ordersByStatus } from "../sidebar/order";
import { useDaemon } from "../store";
import { withIcons } from "./statusItemIcons";
import { buildStatusItemMenu } from "./statusItemMenu";
import { nextQuotaTick } from "./statusItemQuota";

/** The longest a timer may wait: `setTimeout` runs a longer delay at once. */
const MAX_TIMER_MS = 2 ** 31 - 1;

interface UseStatusItemMenuOptions {
  /** Pass `hosts !== undefined`, as for `useWaitingNotifications`: nothing is sent before the first
   * snapshot, whose sessions would otherwise be missing from the menu. */
  ready: boolean;
  selectedSessionId?: string;
  currentConsoleId?: string;
  onSessionChosen: (session: Session) => void;
}

/**
 * Keeps the shell's menu bar icon menu listing the live sessions and each available agent's quota
 * (`buildStatusItemMenu`), and selects a session chosen there. Best-effort, like the Dock badge: a
 * menu left a moment behind is not worth surfacing.
 *
 * A window's bar is drawn full once its reset time has passed and the readings' ages are worded to the
 * minute, so the menu is also pushed again when the next of those moments comes (`nextQuotaTick`).
 *
 * Opening the menu asks the daemon to refresh the quotas that are due. The menu is never replaced
 * while open, so what that brings lands in the menu's next update.
 *
 * It follows the store through a subscription of its own rather than through props, so the menu is
 * rebuilt on each change of the consoles, projects, sessions, accounts, added agents, the sidebar's
 * ordering setting, availability or quotas without rendering anything.
 */
export function useStatusItemMenu({ ready, selectedSessionId, currentConsoleId, onSessionChosen }: UseStatusItemMenuOptions): void {
  const { statusItem } = usePlatform();
  const { store, request } = useDaemon();
  const t = useT();
  const language = useCurrentLanguage();
  // Status flips are frequent and most change nothing the menu shows.
  const lastSentRef = useRef<string | undefined>(undefined);
  const pendingRef = useRef<Promise<void>>(Promise.resolve());

  useEffect(() => {
    if (!statusItem || !ready) return;
    let tickTimer: ReturnType<typeof setTimeout> | undefined;
    const push = () => {
      const { consoles, projects, sessions, settings, agentAvailability, accountQuotas } = store.getState();
      const now = Date.now();
      const draft = buildStatusItemMenu(t, {
        selectedSessionId,
        currentConsoleId,
        consoles: Array.from(consoles.values()),
        projects: Array.from(projects.values()),
        sessions: Array.from(sessions.values()),
        consoleMap: consoles,
        projectMap: projects,
        byStatus: ordersByStatus(settings),
        // Countdowns and ages are as of this push, which the clock alone also causes (`nextQuotaTick`).
        quota: { language, now, accounts: settings.accounts, addedAgents: settings.added_agents, defaultAccountNames: settings.default_account_names, agentAvailability, accountQuotas },
      });
      // Re-armed on every push, so it follows the quotas held now; the one pending is replaced.
      clearTimeout(tickTimer);
      const tick = nextQuotaTick(accountQuotas, now);
      if (tick !== undefined) tickTimer = setTimeout(push, Math.min(MAX_TIMER_MS, Math.max(0, tick - now)));
      const serialized = JSON.stringify(draft);
      if (serialized === lastSentRef.current) return;
      lastSentRef.current = serialized;
      // Chained so two pushes cannot settle out of order and leave the older menu up. The pictures are
      // drawn once per key, so a menu that differs only in its text costs nothing more.
      pendingRef.current = pendingRef.current.then(() =>
        withIcons(draft)
          .then((menu) => statusItem.setMenu(menu))
          .catch(() => {}),
      );
    };
    push();
    const unsubscribe = store.subscribe((state, previous) => {
      if (
        state.consoles !== previous.consoles ||
        state.projects !== previous.projects ||
        state.sessions !== previous.sessions ||
        state.settings.accounts !== previous.settings.accounts ||
        state.settings.sidebar_order_by_status !== previous.settings.sidebar_order_by_status ||
        state.settings.added_agents !== previous.settings.added_agents ||
        state.settings.default_account_names !== previous.settings.default_account_names ||
        state.agentAvailability !== previous.agentAvailability ||
        state.accountQuotas !== previous.accountQuotas
      ) {
        push();
      }
    });
    return () => {
      clearTimeout(tickTimer);
      unsubscribe();
    };
  }, [statusItem, ready, selectedSessionId, currentConsoleId, t, language, store]);

  // Read at call time: the subscription is made once.
  const chooseRef = useLatest(onSessionChosen);
  useEffect(
    () =>
      statusItem?.onSessionChosen((id) => {
        const session = store.getState().sessions.get(id);
        if (session) chooseRef.current(session);
      }),
    [statusItem, store],
  );

  // As the rail's panel and Settings ask; the daemon refreshes only the readings that are due.
  useEffect(
    () => (ready ? statusItem?.onOpened(() => void request({ type: "refresh_account_quotas" }).catch(() => {})) : undefined),
    [statusItem, ready, request],
  );
}
