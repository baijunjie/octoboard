import { useEffect, useRef } from "react";

import { isDaemonRefusal } from "../errors/codes";
import { useDaemon, useDaemonStore } from "../store";
import { reloadPage } from "./reloadPage";

/**
 * The page that asked for the daemon's restart reloads itself once the daemon is back on the version it was sent to, so
 * the page it serves is the new one; other pages are only told (the update notice's Reload). `restart(targetVersion, send)`
 * is called as the restart is sent (`send` makes the request), with the version it goes to (`null` where unknown: any other
 * version will do); the page is reloaded once the daemon that greeted last has also sent its status, so that both are
 * its own, and the version it names is another than the one that was asked and is the target when that is known. A daemon
 * that refuses the restart cancels the wait; one that comes back on another version than expected leaves the page to the
 * notice's Reload.
 */
export function useReloadAfterRestart(): { restart: <T>(targetVersion: string | null, send: () => Promise<T>) => Promise<T> } {
  const { store } = useDaemon();
  const version = useDaemonStore((state) => (state.daemonUpdateStale ? undefined : state.daemon?.version));
  const awaited = useRef<{ from: string | undefined; target: string | null }>(undefined);
  useEffect(() => {
    const asked = awaited.current;
    if (!asked || version === undefined || version === asked.from) return;
    if (asked.target !== null && version !== asked.target) return;
    awaited.current = undefined;
    reloadPage();
  }, [version]);
  return {
    async restart(target, send) {
      const asked = { from: store.getState().daemon?.version, target };
      awaited.current = asked;
      try {
        return await send();
      } catch (error) {
        if (isDaemonRefusal(error) && awaited.current === asked) awaited.current = undefined;
        throw error;
      }
    },
  };
}
