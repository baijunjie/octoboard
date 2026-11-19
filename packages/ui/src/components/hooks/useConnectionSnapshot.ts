import { useState } from "react";

import { useDaemonStore } from "../../store";

/** The epoch of the snapshot of the connection that is open now, or `undefined` until there is one: before it the store
 * holds nothing of this connection, or on a reconnect what the previous one last saw. `connectionState` turns "open" at
 * the hello while `snapshotEpoch` still counts the previous connection's snapshots, so the epoch the state began at is
 * remembered, taken during render so that no render sees the new state with the old baseline, and a larger one is
 * waited for. A hook mounted while the connection is already open has no snapshot to rule out, so its baseline is zero. */
export function useConnectionSnapshot(): number | undefined {
  const connectionState = useDaemonStore((s) => s.connectionState);
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  const [began, setBegan] = useState({ state: connectionState, epoch: connectionState === "open" ? 0 : snapshotEpoch });
  if (began.state !== connectionState) setBegan({ state: connectionState, epoch: snapshotEpoch });
  return connectionState === "open" && began.state === "open" && snapshotEpoch > began.epoch ? snapshotEpoch : undefined;
}
