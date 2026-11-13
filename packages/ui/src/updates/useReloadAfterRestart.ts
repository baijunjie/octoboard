import { useEffect, useRef } from "react";

import { isDaemonRefusal } from "../errors/codes";
import { setRestartAwaited, useDaemon, useDaemonStore } from "../store";
import { reloadPage } from "../platform/reloadPage";

/** How long after it sent the restart the page keeps reconnecting by itself. The planned stop waits up to about 15
 * seconds for the holders and the daemon gives the next one 30 seconds to come up (see "Planned restart and planned stop"
 * in `docs/product/planned-restart-and-planned-stop.md`), and a service manager adds its own delay; past this, the
 * connection's own Retry is the way back. */
export const RESTART_RECONNECT_BUDGET_MS = 60_000;

/**
 * The page that asked for the daemon's restart reloads itself once the daemon is back on the version it was sent to, so
 * the page it serves is the new one; other pages are only told (the update notice's Reload). `restart(targetVersion, send)`
 * is called as the restart is sent (`send` makes the request), with the version it goes to (`null` where unknown: any other
 * version will do); the page is reloaded once the daemon that greeted last has also sent its status, so that both are
 * its own, and the version it names is another than the one that was asked and is the target when that is known. A daemon
 * that refuses the restart cancels the wait; one that comes back on another version than expected ends it, leaving the
 * page to the notice's Reload.
 *
 * While it waits, a connection whose own reconnect attempts are spent is retried until `RESTART_RECONNECT_BUDGET_MS`
 * after the restart was sent, since a planned restart can take longer than those attempts last. A daemon that comes back
 * speaking another protocol generation reloads the page too: it serves the page of its own build, which speaks its
 * generation, and the reloaded page waits for nothing, so it cannot reload again.
 */
export function useReloadAfterRestart(): { restart: <T>(targetVersion: string | null, send: () => Promise<T>) => Promise<T> } {
  const { store, reconnect } = useDaemon();
  const version = useDaemonStore((state) => (state.daemonUpdateStale ? undefined : state.daemon?.version));
  const connectionState = useDaemonStore((state) => state.connectionState);
  const awaited = useRef<{ from: string | undefined; target: string | null; until: number }>(undefined);
  useEffect(() => {
    const asked = awaited.current;
    // The version it was asked from may still greet while the daemon stops, and is waited past.
    if (!asked || version === undefined || version === asked.from) return;
    awaited.current = undefined;
    // Back on another version than the target: the restart this page waited for is over, and what follows (another
    // tab's restart included) is the notice's to tell.
    if (asked.target !== null && version !== asked.target) return;
    reloadPage();
  }, [version]);
  useEffect(() => {
    const asked = awaited.current;
    if (!asked) return;
    if (connectionState === "incompatible") {
      awaited.current = undefined;
      reloadPage();
    } else if (connectionState === "closed" && Date.now() < asked.until) {
      reconnect();
    }
  }, [connectionState, reconnect]);
  return {
    async restart(target, send) {
      const asked = { from: store.getState().daemon?.version, target, until: Date.now() + RESTART_RECONNECT_BUDGET_MS };
      awaited.current = asked;
      setRestartAwaited(store, true);
      try {
        return await send();
      } catch (error) {
        if (isDaemonRefusal(error) && awaited.current === asked) {
          awaited.current = undefined;
          setRestartAwaited(store, false);
        }
        throw error;
      }
    },
  };
}
