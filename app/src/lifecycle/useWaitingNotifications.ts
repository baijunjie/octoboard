import { useEffect, useRef } from "react";

import type { Console, Project, Session } from "../protocol";
import { sessionLocation } from "../sessionLabel";

/** Set once we are actually running inside the Tauri shell — lets `npm run dev` in a plain browser
 * skip the notification/badge calls entirely instead of throwing on missing APIs, matching the same
 * check in `useAppExit`. */
const runningInTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Cached across every call so only the first session that ever waits triggers the permission
 * prompt; a later one just reads the cached answer. Only a granted answer is cached: macOS shows
 * its own prompt once per app regardless, so re-asking after a decline nags nobody, but caching
 * `false` would leave notifications off for the rest of the process even after the user grants the
 * permission in System Settings. */
let permissionRequest: Promise<boolean> | undefined;

function ensureNotificationPermission(): Promise<boolean> {
  if (!permissionRequest) {
    permissionRequest = (async () => {
      const { isPermissionGranted, requestPermission } = await import("@tauri-apps/plugin-notification");
      if (await isPermissionGranted()) return true;
      return (await requestPermission()) === "granted";
    })().catch(() => false);
    void permissionRequest.then((granted) => {
      if (!granted) permissionRequest = undefined;
    });
  }
  return permissionRequest;
}

/**
 * Fires a system notification and keeps the Dock badge at the number of sessions currently
 * `waiting_user`, per the "Waiting for the user (raised hand)" section of docs/mvp.md.
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
 * Both the notification and the badge are best-effort: outside the Tauri shell (`vite dev`) or with
 * the permission declined, the calls fail and are swallowed rather than surfaced, since the tree's
 * own raised-hand marker already carries the same information.
 */
export function useWaitingNotifications(
  sessions: Session[],
  consoles: Map<string, Console>,
  projects: Map<string, Project>,
  ready: boolean,
): void {
  const previousWaitingRef = useRef<Set<string>>();
  const lastBadgeCountRef = useRef<number>();
  // `consoles`/`projects` only feed the notification body text, not the waiting/newly-waiting diff
  // itself; reading them through a ref keeps them out of the effect's deps, so a console or project
  // rename alone does not re-run the badge and notify pass for every session.
  const locationsRef = useRef({ consoles, projects });
  locationsRef.current = { consoles, projects };

  useEffect(() => {
    if (!runningInTauri || !ready) return;

    const waiting = sessions.filter((s) => s.status === "waiting_user");
    const waitingIds = new Set(waiting.map((s) => s.id));
    const previousIds = previousWaitingRef.current;
    previousWaitingRef.current = waitingIds;

    if (waitingIds.size !== lastBadgeCountRef.current) {
      lastBadgeCountRef.current = waitingIds.size;
      void (async () => {
        try {
          const { getCurrentWindow } = await import("@tauri-apps/api/window");
          await getCurrentWindow().setBadgeCount(waitingIds.size > 0 ? waitingIds.size : undefined);
        } catch {
          // Best-effort — see the function doc.
        }
      })();
    }

    if (previousIds === undefined) return;
    const newlyWaiting = waiting.filter((s) => !previousIds.has(s.id));
    if (newlyWaiting.length === 0) return;

    void (async () => {
      if (!(await ensureNotificationPermission())) return;
      const { sendNotification } = await import("@tauri-apps/plugin-notification");
      const { consoles: currentConsoles, projects: currentProjects } = locationsRef.current;
      for (const session of newlyWaiting) {
        const location = sessionLocation(session, currentConsoles, currentProjects);
        try {
          sendNotification({
            title: "Waiting for you",
            // The location already repeats the session's own title for a session titled after its
            // project, so it is dropped rather than printed twice.
            body: location === session.title ? session.title : `${session.title} — ${location}`,
          });
        } catch {
          // Best-effort — see the function doc.
        }
      }
    })();
  }, [sessions, ready]);
}
