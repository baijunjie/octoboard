import { useSyncExternalStore } from "react";

import { subscribeWindowFocus, windowHasFocus } from "../hooks/windowFocus";

/** Whether the user is in this window: its document is visible and the window has focus. A window that is minimized,
 * hidden or behind another application is not attended, and what a status region says then goes unheard, as
 * VoiceOver speaks the window's focused control when the user comes back rather than an earlier status. Focus moved
 * into a frame of this document still counts as attended. */
export function useWindowAttended(): boolean {
  return useSyncExternalStore(subscribe, isAttended);
}

function isAttended(): boolean {
  return document.visibilityState === "visible" && windowHasFocus();
}

function subscribe(onChange: () => void): () => void {
  const undoFocus = subscribeWindowFocus(onChange);
  document.addEventListener("visibilitychange", onChange);
  return () => {
    undoFocus();
    document.removeEventListener("visibilitychange", onChange);
  };
}
