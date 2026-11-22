import { useEffect, useRef } from "react";

import { ringForAutomaticFocus } from "../../input/lastInput";
import { afterFrames } from "./afterFrames";
import { focusFellToPage } from "./regionFocus";

/** Hands focus on once a dialog has closed, for the surface that opened it, where react-aria's own restore can find
 * nothing to return to (its opener gone, disabled, or never focused): returns the function to call when the dialog
 * closes, which two frames later (React Aria restores focus in the frame after the dialog unmounts) runs `restore` if
 * focus fell to the page, the way `useLoginFocusReturn` does for the login dialog. The user did not ask for the move, so
 * the ring follows the last input. Nothing runs once the surface is gone. */
export function useFocusReturnAfterClose(restore: () => void): () => void {
  const cancel = useRef<() => void>(undefined);
  useEffect(() => () => cancel.current?.(), []);
  return () => {
    cancel.current?.();
    cancel.current = afterFrames(2, () => {
      if (!focusFellToPage()) return;
      ringForAutomaticFocus();
      restore();
    });
  };
}
