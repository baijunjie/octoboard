import { useEffect, useRef } from "react";

import { t } from "../i18n/language";
import { usePlatform } from "../platform/react";
import type { Console, Project, Session } from "../protocol";
import { sessionLocation } from "../sessionLabel";

/**
 * Fires a system notification and keeps the Dock badge at the number of sessions currently
 * `waiting_user`, per the "The raised hand" section of docs/product/sessions.md.
 *
 * Only a transition into `waiting_user` notifies, each session once: comparing against the
 * previous pass's waiting set (rather than, say, a per-session "already notified" flag that never
 * clears) is what makes a session that waits, gets answered, and waits again notify a second time.
 * `ready` (pass `hosts !== undefined`, i.e. the first `snapshot` has arrived) gates the very first
 * comparison: `App` renders, and this hook with it, before any snapshot exists, so without the gate
 * the baseline would be captured against an empty session list and every session already
 * `waiting_user` when Octoboard started would read as newly waiting. Once `ready` is true it stays
 * true (`hosts` is never cleared again), so a reconnect's re-sent snapshot still diffs against the
 * pre-drop waiting set and correctly notifies nothing for sessions that were already waiting.
 *
 * Both the notification and the badge are best-effort capabilities of the platform adapter: where
 * the platform has none (a browser without notifications, any browser's badge) the feature is
 * absent, and with the permission declined or a call failing they are swallowed rather than
 * surfaced, since the tree's own raised-hand marker already carries the same information.
 */
export function useWaitingNotifications(
  sessions: Session[],
  consoles: Map<string, Console>,
  projects: Map<string, Project>,
  ready: boolean,
): void {
  const { notifications, badge } = usePlatform();
  const previousWaitingRef = useRef<Set<string> | undefined>(undefined);
  const lastBadgeCountRef = useRef<number | undefined>(undefined);
  // `consoles`/`projects` only feed the notification body text, not the waiting/newly-waiting diff
  // itself; reading them through a ref keeps them out of the effect's deps, so a console or project
  // rename alone does not re-run the badge and notify pass for every session.
  const locationsRef = useRef({ consoles, projects });
  locationsRef.current = { consoles, projects };

  useEffect(() => {
    if ((!notifications && !badge) || !ready) return;

    const waiting = sessions.filter((s) => s.status === "waiting_user");
    const waitingIds = new Set(waiting.map((s) => s.id));
    const previousIds = previousWaitingRef.current;
    previousWaitingRef.current = waitingIds;

    if (badge && waitingIds.size !== lastBadgeCountRef.current) {
      lastBadgeCountRef.current = waitingIds.size;
      void (async () => {
        try {
          await badge.set(waitingIds.size);
        } catch {
          // Best-effort — see the function doc.
        }
      })();
    }

    if (!notifications || previousIds === undefined) return;
    const newlyWaiting = waiting.filter((s) => !previousIds.has(s.id));
    if (newlyWaiting.length === 0) return;

    void (async () => {
      if (!(await notifications.ensurePermission())) return;
      const { consoles: currentConsoles, projects: currentProjects } = locationsRef.current;
      for (const session of newlyWaiting) {
        const location = sessionLocation(t, session, currentConsoles, currentProjects);
        try {
          await notifications.notify({
            title: t("notification.waiting.title"),
            // The location already repeats the session's own title for a session titled after its
            // project, so it is dropped rather than printed twice.
            body: location === session.title ? session.title : t("notification.waiting.body", { session: session.title, location }),
          });
        } catch {
          // Best-effort — see the function doc.
        }
      }
    })();
  }, [sessions, ready, notifications, badge]);
}
