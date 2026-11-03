/** The end of the wait `skipKeyContextMenu` is running, if one is. */
let endSkip: (() => void) | null = null;

/** Ends the wait set up by `skipKeyContextMenu`, if one is running: the menu is closing or going away. */
export function endKeyContextMenuSkip(): void {
  endSkip?.();
}

/** Skips the `contextmenu` event a browser may still raise for a Menu key or Shift+F10 that a menu has
 * already opened from: some raise it only as the key comes back up (Chromium on Windows, for the Menu
 * key), when cancelling the keydown no longer prevents it, and it would open the menu a second time or
 * the browser's own menu over it. The wait ends with that event, the next keydown or pointer press, the
 * menu closing or going away, or a second. */
export function skipKeyContextMenu(): void {
  endSkip?.();
  const listeners = new AbortController();
  const end = () => {
    listeners.abort();
    clearTimeout(timer);
    if (endSkip === end) endSkip = null;
  };
  const skip = (event: Event) => {
    event.preventDefault();
    event.stopImmediatePropagation();
    end();
  };
  window.addEventListener("contextmenu", skip, { capture: true, signal: listeners.signal });
  // A held key repeats its keydown before the keyup that raises the late event, so repeats leave the skip on.
  window.addEventListener("keydown", (event) => { if (!event.repeat) end(); }, { capture: true, signal: listeners.signal });
  window.addEventListener("pointerdown", end, { capture: true, signal: listeners.signal });
  const timer = setTimeout(end, 1000);
  endSkip = end;
}
