import { useCallback, useSyncExternalStore } from "react";

import { useOptionalPlatform } from "../../platform/react";
import { subscribeWindowFocus, windowHasFocus } from "../hooks/windowFocus";

/** Whether the user is in this window: its document is visible and the window has focus. A window that is minimized,
 * hidden or behind another application is not attended, and what a status region says then goes unheard, as
 * VoiceOver speaks the window's focused control when the user comes back rather than an earlier status. Focus moved
 * into a frame of this document still counts as attended. */
export function useWindowAttended(): boolean {
  const windowFocus = useOptionalPlatform()?.windowFocus;
  const subscribe = useCallback((onChange: () => void) => {
    const undoFocus = subscribeWindowFocus(windowFocus, onChange);
    document.addEventListener("visibilitychange", onChange);
    return () => {
      undoFocus();
      document.removeEventListener("visibilitychange", onChange);
    };
  }, [windowFocus]);
  return useSyncExternalStore(subscribe, () => document.visibilityState === "visible" && windowHasFocus(windowFocus));
}
