import { useEffect } from "react";

import { useDaemonStore, useNativeAppsRefresh } from "../store";

/** Probes which agents' desktop applications the daemon's host has installed whenever the window connects, which is
 * when a reconnect or a lag recovery's snapshot arrives too. The menus and the archive view that offer opening a
 * session in one probe again as they open. */
export function useNativeAppsSync(): void {
  const refresh = useNativeAppsRefresh();
  const connectionState = useDaemonStore((s) => s.connectionState);
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  useEffect(() => {
    if (connectionState === "open") refresh();
    // `refresh` is rebuilt on every render, and what it asks does not depend on it.
  }, [connectionState, snapshotEpoch]);
}
