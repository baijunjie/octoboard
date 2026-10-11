import { useEffect, useRef } from "react";

import { usePlatform } from "../platform/react";
import { runInBackground } from "./runInBackground";

/**
 * Pushes the run-in-background choice to the shell on mount, before the window can be closed, and
 * on every change, so the shell answers the close button as the user last chose. Best-effort: a
 * shell left on its previous choice is not worth surfacing to the user.
 */
export function useBackgroundRunSync(): void {
  const { backgroundRun } = usePlatform();
  const enabled = runInBackground.useValue();
  // Chained for the same reason as in `useNativeWindowTheme`: two in-flight pushes must not settle
  // out of order and leave the shell on the earlier choice.
  const pendingRef = useRef<Promise<void>>(Promise.resolve());

  useEffect(() => {
    if (!backgroundRun) return;
    pendingRef.current = pendingRef.current.then(() => backgroundRun.set(enabled).catch(() => {}));
  }, [backgroundRun, enabled]);
}
