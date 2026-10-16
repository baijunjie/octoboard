import { useEffect, useRef } from "react";

import { useLatest } from "../components/useLatest";
import { t } from "../i18n/language";
import { usePlatform } from "../platform/react";
import type { Console, Project } from "../protocol";
import { sessionWithLocation } from "../sessionLabel";
import { useWaitingIds } from "../sessions/useSessions";
import { useDaemon } from "../store";

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
 *
 * It reads the sessions from the store itself, as the ids of the waiting ones, which are the same
 * array until a session starts or stops waiting: a status event that changes nothing about who
 * waits does not render the caller.
 */
export function useWaitingNotifications(
  consoles: Map<string, Console>,
  projects: Map<string, Project>,
  ready: boolean,
): void {
  const { notifications, badge } = usePlatform();
  const { store } = useDaemon();
  const waitingList = useWaitingIds();
  const previousWaitingRef = useRef<Set<string> | undefined>(undefined);
  const lastBadgeCountRef = useRef<number | undefined>(undefined);
  // `consoles`/`projects` only feed the notification body text, not the waiting/newly-waiting diff
  // itself; reading them through a ref keeps them out of the effect's deps, so a console or project
  // rename alone does not re-run the badge and notify pass for every session.
  const locationsRef = useLatest({ consoles, projects });

  useEffect(() => {
    if ((!notifications && !badge) || !ready) return;

    const waitingIds = new Set(waitingList);
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
    const newlyWaiting = waitingList
      .filter((id) => !previousIds.has(id))
      .flatMap((id) => store.getState().sessions.get(id) ?? []);
    if (newlyWaiting.length === 0) return;

    void (async () => {
      if (!(await notifications.ensurePermission())) return;
      const { consoles: currentConsoles, projects: currentProjects } = locationsRef.current;
      for (const session of newlyWaiting) {
        try {
          await notifications.notify({
            title: t("notification.waiting.title"),
            body: sessionWithLocation(t, session, currentConsoles, currentProjects),
          });
        } catch {
          // Best-effort — see the function doc.
        }
      }
    })();
  }, [waitingList, ready, notifications, badge]);
}
