import { useEffect, useRef } from "react";

import { useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import type { Console, Project, Session } from "../protocol";
import { buildStatusItemMenu } from "./statusItemMenu";

interface UseStatusItemMenuOptions {
  consoles: Console[];
  projects: Project[];
  sessions: Session[];
  consoleMap: Map<string, Console>;
  projectMap: Map<string, Project>;
  /** Pass `hosts !== undefined`, as for `useWaitingNotifications`: nothing is sent before the first
   * snapshot, whose sessions would otherwise be missing from the menu. */
  ready: boolean;
  onSessionChosen: (session: Session) => void;
}

/**
 * Keeps the shell's menu bar icon menu listing the live sessions (`buildStatusItemMenu`), and
 * selects a session chosen there. Best-effort, like the Dock badge: a menu left a moment behind is
 * not worth surfacing.
 */
export function useStatusItemMenu({
  consoles,
  projects,
  sessions,
  consoleMap,
  projectMap,
  ready,
  onSessionChosen,
}: UseStatusItemMenuOptions): void {
  const { statusItem } = usePlatform();
  const t = useT();
  // Status flips are frequent and most change nothing the menu shows.
  const lastSentRef = useRef<string | undefined>(undefined);
  const pendingRef = useRef<Promise<void>>(Promise.resolve());

  useEffect(() => {
    if (!statusItem || !ready) return;
    const menu = buildStatusItemMenu(t, { consoles, projects, sessions, consoleMap, projectMap });
    const serialized = JSON.stringify(menu);
    if (serialized === lastSentRef.current) return;
    lastSentRef.current = serialized;
    // Chained so two pushes cannot settle out of order and leave the older menu up.
    pendingRef.current = pendingRef.current.then(() => statusItem.setMenu(menu).catch(() => {}));
  }, [statusItem, ready, t, consoles, projects, sessions, consoleMap, projectMap]);

  // Read at call time: the subscription is made once.
  const chooseRef = useRef({ sessions, onSessionChosen });
  chooseRef.current = { sessions, onSessionChosen };
  useEffect(
    () =>
      statusItem?.onSessionChosen((id) => {
        const session = chooseRef.current.sessions.find((s) => s.id === id);
        if (session) chooseRef.current.onSessionChosen(session);
      }),
    [statusItem],
  );
}
