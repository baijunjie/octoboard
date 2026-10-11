import { useEffect, useRef } from "react";

import { MODAL_OPEN } from "../layout/useRegionCycle";
import { focusTopBar } from "./regionFocus";

/** Hands focus to the top bar after the first console is made from the welcome screen. The button
 * that made it unmounts with the screen and drops focus to `<body>`, and the top bar's sidebar
 * toggle is back by then, so it takes focus.
 *
 * `welcomeShown` is whether the welcome screen is on screen, which is only so once the first
 * snapshot has arrived: before that the store holds no console either, and treating that as "no
 * console" would owe a hand-off on every launch. `creating` is whether this window has the
 * new-console dialog open: the screen going away is only this window's doing if the dialog was open
 * while the screen was still there, and another window's console, which empties the screen here
 * too, must not pull focus. The hand-off is owed from the screen going away until focus has
 * actually been placed, and not sampled once: the daemon broadcasts the new console before it
 * answers the request (`coordinator.rs`), so the store can get it in an earlier render than the
 * dialog's `onClose`, while the dialog is still open and holding focus. The dialog then closes and
 * its restore target, the button, is already gone. Either order lands here once no dialog or menu
 * is open (asked of the document, so a modal that is not the new-console dialog counts too) and
 * focus is on `<body>`; a launch that already has consoles never shows the screen and so never owes
 * it, and focus that is somewhere by then is left alone. The check runs after every render, as the
 * document is what it asks. */
export function useFirstConsoleHandoff(welcomeShown: boolean, creating: boolean): void {
  const wasShown = useRef(welcomeShown);
  const creatingHere = useRef(false);
  const owed = useRef(false);
  useEffect(() => {
    if (welcomeShown) {
      owed.current = false;
      if (creating) creatingHere.current = true;
    } else if (wasShown.current) {
      owed.current = creatingHere.current;
      creatingHere.current = false;
    }
    wasShown.current = welcomeShown;
    if (!owed.current || document.querySelector(MODAL_OPEN)) return;
    owed.current = false;
    if (document.activeElement === document.body) focusTopBar();
  });
}
