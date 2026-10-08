import React, { useRef } from "react";

/**
 * Tracks which element had focus when a pointer press opened a popup (a menu, a popover), so the
 * popup's owner can give focus back to it on close. Keyboard focus belongs on the terminal while
 * the mouse is used, but react-aria returns focus to the popup's trigger when it closes, which
 * would pull it off whatever had it.
 *
 * Put `onPointerDownCapture` on an element around the trigger, and call `opened` from the popup's
 * `onOpenChange`; a popup opened by a gesture that is not a press on the trigger (a right-click on
 * the row it belongs to) calls `notePress` just before opening. On open it settles the press into a
 * pointer-opened popup; on close it returns `{ previous }` for a pointer-opened one — `previous` the
 * element that had focus at the press, `null` when none did — and `undefined` for one opened from
 * the keyboard, which keeps react-aria's own return to the trigger.
 */
export function usePointerFocusReturn(): {
  onPointerDownCapture: (event: React.PointerEvent<HTMLElement>) => void;
  /** For a popup a pointer opens without a press on its trigger (a right-click on the row it
   * belongs to): notes the focus as it stands, to be settled by the `opened` that follows.
   * `previous` stands in for the focus when it is known to be elsewhere than where it now sits. */
  notePress: (previous?: HTMLElement) => void;
  opened: (isOpen: boolean) => { previous: HTMLElement | null } | undefined;
} {
  // Held only until the press ends; if the press opened the popup it moves to `restoreRef`, so a
  // press that never opens it (dragged off the trigger) leaves nothing behind.
  const pressRef = useRef<{ previous: HTMLElement | null } | null>(null);
  const restoreRef = useRef<{ previous: HTMLElement | null } | null>(null);

  const notePress = (previous?: HTMLElement) => {
    const active = document.activeElement;
    pressRef.current = {
      previous: previous ?? (active instanceof HTMLElement && active !== document.body ? active : null),
    };
  };

  const onPointerDownCapture = (event: React.PointerEvent<HTMLElement>) => {
    // Only a press on the trigger itself: a portalled popup's presses bubble through here as well.
    if (!event.currentTarget.contains(event.target as Node)) return;
    notePress();
    // The press opens the popup during this gesture (on pointerdown for a mouse, on release for
    // touch), before this timeout runs.
    const listeners = new AbortController();
    const end = () => {
      listeners.abort();
      setTimeout(() => (pressRef.current = null), 0);
    };
    window.addEventListener("pointerup", end, { signal: listeners.signal });
    window.addEventListener("pointercancel", end, { signal: listeners.signal });
  };

  const opened = (isOpen: boolean) => {
    if (isOpen) {
      restoreRef.current = pressRef.current;
      pressRef.current = null;
      return undefined;
    }
    const restore = restoreRef.current;
    restoreRef.current = null;
    return restore ?? undefined;
  };

  return { onPointerDownCapture, notePress, opened };
}
