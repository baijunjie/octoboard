import { useEffect, type RefObject } from "react";

import { useEscapeStack } from "./useEscapeStack";

/**
 * Closes an open popover (`Dropdown`, `ActionMenu`) on an outside pointerdown or on Escape. Shared
 * by both so they dismiss identically; Escape specifically goes through the shared stack so only
 * the topmost open layer reacts, rather than every mounted popover at once.
 */
export function useDismissOnOutsideOrEscape(
  open: boolean,
  ref: RefObject<HTMLElement>,
  onDismiss: () => void,
): void {
  useEscapeStack(open, onDismiss);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (ev: PointerEvent) => {
      if (ref.current && !ref.current.contains(ev.target as Node)) onDismiss();
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [open, ref, onDismiss]);
}
