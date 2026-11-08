// Whether the user is in this window, for what has to follow them leaving it for another application and coming
// back. Where a shell owns the window, its own focus is the answer: the macOS webview fires no `focus` or `blur` on
// the window when the app is deactivated or reactivated, and its `document.hasFocus()` changes some time after the
// window does. Only a browser, which has no such capability, falls back to the document's events.
import type { WindowFocusCapability } from "../../platform";

/** Whether the window has focus: the shell's answer where `windowFocus` has one, the document's otherwise. Focus
 * moved into a frame of this document is still focus in this window either way. The shell only has an answer while
 * something is subscribed (`subscribeWindowFocus`), so a caller that reads this without a subscription of its own
 * may get the document's lagging answer. */
export function windowHasFocus(windowFocus: WindowFocusCapability | undefined): boolean {
  return windowFocus?.focused() ?? document.hasFocus();
}

/** Calls `onChange` whenever `windowHasFocus` may have changed, returning the function that undoes it. */
export function subscribeWindowFocus(windowFocus: WindowFocusCapability | undefined, onChange: () => void): () => void {
  if (windowFocus) return windowFocus.subscribe(onChange);
  window.addEventListener("focus", onChange);
  window.addEventListener("blur", onChange);
  return () => {
    window.removeEventListener("focus", onChange);
    window.removeEventListener("blur", onChange);
  };
}

/** Calls `onReturn` each time the window gets focus back after losing it, returning the function that undoes it. The
 * shell's first answer after subscribing is where the window stands, not a return, so it calls nothing. */
export function onWindowFocusReturn(windowFocus: WindowFocusCapability | undefined, onReturn: () => void): () => void {
  if (!windowFocus) {
    window.addEventListener("focus", onReturn);
    return () => window.removeEventListener("focus", onReturn);
  }
  let last = windowFocus.focused();
  return windowFocus.subscribe(() => {
    const now = windowFocus.focused();
    if (now === true && last === false) onReturn();
    last = now;
  });
}
