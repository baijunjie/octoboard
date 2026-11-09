import { useEffect } from "react";

import { usePlatform } from "../platform/react";
import { useDaemonStoreOrEmpty } from "../store";
import { useSerialPush } from "../components/hooks/useSerialPush";

/**
 * Pushes to the shell whether the daemon is keeping the computer awake (`keep_awake_held`), on mount and whenever it
 * changes, so the shell's menu bar icon can show it. The daemon holds the computer awake itself (Prevent sleep,
 * `crate::keep_awake` in the daemon), and the shell holds nothing. A connection that is not open counts as not held:
 * the hold goes with the daemon. Best-effort: an icon left on its previous look is not worth surfacing to the user.
 *
 * Mounted under the `DaemonProvider` but outside the error boundary, so the icon follows the daemon for as long as the
 * window lives, hidden or not. Only where the platform has a shell to tell (`platform.keepAwake`).
 */
export function useKeepAwakeSync(): void {
  const { keepAwake: capability } = usePlatform();
  const held = useDaemonStoreOrEmpty((state) => state.connectionState === "open" && state.keepAwakeHeld);
  // Serial, so two in-flight pushes cannot settle out of order and leave the icon on the earlier answer.
  const push = useSerialPush();

  useEffect(() => {
    if (!capability) return;
    push(() => capability.set(held));
  }, [capability, held, push]);
}
