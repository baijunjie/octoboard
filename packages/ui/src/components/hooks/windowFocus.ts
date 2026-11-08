// Whether the user is in this window, for what has to follow them leaving it for another application and coming
// back. The window's own `focus` and `blur` say so, in the desktop app's webview as in a browser, and
// `document.hasFocus()` already matches by the time either arrives. They reach a listener here only because
// `input/focusGuard.ts` leaves the window's focus events alone outside a press.

/** Whether the window has focus. Focus moved into a frame of this document is still focus in this window. */
export function windowHasFocus(): boolean {
  return document.hasFocus();
}

/** Calls `onChange` whenever `windowHasFocus` may have changed, returning the function that undoes it. */
export function subscribeWindowFocus(onChange: () => void): () => void {
  const stopFocus = onWindowEvent("focus", onChange);
  const stopBlur = onWindowEvent("blur", onChange);
  return () => {
    stopFocus();
    stopBlur();
  };
}

/** Calls `onReturn` each time the window gets focus back, returning the function that undoes it. */
export function onWindowFocusReturn(onReturn: () => void): () => void {
  return onWindowEvent("focus", onReturn);
}

/** Calls `onLoss` each time the window loses focus, returning the function that undoes it: what has to let go once the
 * user is elsewhere, such as a held key whose release this window never hears or a press it never sees end. */
export function onWindowFocusLoss(onLoss: () => void): () => void {
  return onWindowEvent("blur", onLoss);
}

function onWindowEvent(type: "focus" | "blur", callback: () => void): () => void {
  const listener = () => callback();
  window.addEventListener(type, listener);
  return () => window.removeEventListener(type, listener);
}
