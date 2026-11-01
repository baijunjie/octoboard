import { useEffect } from "react";

import { usePlatform } from "../platform/react";
import { runInBackground } from "./runInBackground";
import { useSerialPush } from "./useSerialPush";

/**
 * Pushes the run-in-background choice to the shell on mount, before the window can be closed, and
 * on every change, so the shell answers the close button as the user last chose. Best-effort: a
 * shell left on its previous choice is not worth surfacing to the user.
 */
export function useBackgroundRunSync(): void {
  const { backgroundRun } = usePlatform();
  const enabled = runInBackground.useValue();
  // Serial, so two in-flight pushes cannot settle out of order and leave the shell on the earlier choice.
  const push = useSerialPush();

  useEffect(() => {
    if (!backgroundRun) return;
    push(() => backgroundRun.set(enabled));
  }, [backgroundRun, enabled, push]);
}
