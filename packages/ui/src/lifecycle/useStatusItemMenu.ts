import { useEffect, useRef } from "react";

import { useLatest } from "../components/useLatest";
import { useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import type { Session } from "../protocol";
import { useDaemon } from "../store";
import { withIcons } from "./statusItemIcons";
import { buildStatusItemMenu } from "./statusItemMenu";

interface UseStatusItemMenuOptions {
  /** Pass `hosts !== undefined`, as for `useWaitingNotifications`: nothing is sent before the first
   * snapshot, whose sessions would otherwise be missing from the menu. */
  ready: boolean;
  onSessionChosen: (session: Session) => void;
}

/**
 * Keeps the shell's menu bar icon menu listing the live sessions (`buildStatusItemMenu`), and
 * selects a session chosen there. Best-effort, like the Dock badge: a menu left a moment behind is
 * not worth surfacing.
 *
 * It follows the store through a subscription of its own rather than through props, so the menu is
 * rebuilt on each change of the consoles, projects or sessions without rendering anything.
 */
export function useStatusItemMenu({ ready, onSessionChosen }: UseStatusItemMenuOptions): void {
  const { statusItem } = usePlatform();
  const { store } = useDaemon();
  const t = useT();
  // Status flips are frequent and most change nothing the menu shows.
  const lastSentRef = useRef<string | undefined>(undefined);
  const pendingRef = useRef<Promise<void>>(Promise.resolve());

  useEffect(() => {
    if (!statusItem || !ready) return;
    const push = () => {
      const { consoles, projects, sessions } = store.getState();
      const draft = buildStatusItemMenu(t, {
        consoles: Array.from(consoles.values()),
        projects: Array.from(projects.values()),
        sessions: Array.from(sessions.values()),
        consoleMap: consoles,
        projectMap: projects,
      });
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
    return store.subscribe((state, previous) => {
      if (state.consoles !== previous.consoles || state.projects !== previous.projects || state.sessions !== previous.sessions) push();
    });
  }, [statusItem, ready, t, store]);

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
}
