import { useEffect } from "react";

import { usePlatform } from "../platform/react";
import { useDaemonStoreOrEmpty } from "../store";
import { sessionIsWorking } from "../sessions/sessionStatus";
import { keepAwake, keepAwakeWhen, shouldKeepAwake } from "./keepAwake";
import { useSerialPush } from "../components/hooks/useSerialPush";

/**
 * Pushes to the shell whether to hold the computer and its display awake, on mount and whenever the answer changes:
 * Prevent sleep is on and either it holds always or a session, in any console, is working (`sessionIsWorking`). The
 * shell holds nothing at launch. Best-effort: a shell left on its previous answer is not worth surfacing to the user.
 *
 * Mounted under the `DaemonProvider` but outside the error boundary, so it follows the sessions for as long as the
 * window lives, hidden or not. The screen shown when the daemon failed to start mounts it too: with no sessions only
 * "always" holds there, and it does, since the app is running.
 */
export function useKeepAwakeSync(): void {
  const { keepAwake: capability } = usePlatform();
  const enabled = keepAwake.useValue();
  const when = keepAwakeWhen.useValue();
  // Only a flip of the answer re-renders; the scan is skipped where the sessions cannot change it.
  const working = useDaemonStoreOrEmpty((state) => enabled && when === "working" && Array.from(state.sessions.values()).some(sessionIsWorking));
  const held = shouldKeepAwake(enabled, when, working);
  // Serial, so two in-flight pushes cannot settle out of order and leave the shell on the earlier answer.
  const push = useSerialPush();

  useEffect(() => {
    if (!capability) return;
    push(() => capability.set(held));
  }, [capability, held, push]);
}
