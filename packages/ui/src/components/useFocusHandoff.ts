import { ringForAutomaticFocus } from "../touchInput";
import { useEffect, useRef } from "react";

import { focusTopBar } from "./regionFocus";

/** Hands keyboard focus on when the control that held it goes away, or is disabled. A focused
 * element that is removed fires no blur and focus falls to `<body>`, so the caller spreads the
 * returned `onFocus`/`onBlur` on a wrapper of the control, which remembers the element focus is
 * on, and after every render this checks whether that element has left the document or become
 * `:disabled` (WebKit can leave such a control the active element, and its blur is not the user's
 * leaving, so it stays remembered). A blur of the whole window (the browser's own prompt taking
 * focus) leaves the element the active element and keeps it remembered. Focus moved by script
 * while the window is unfocused fires nothing either, so the hand-off also needs focus to have
 * actually fallen to `<body>` (or to be on the disabled element). It then goes where `restore`
 * puts it — the terminal, say, or a control beside the one that went — or to the top bar's first
 * control when that leaves it on `<body>`. The user did not ask for the move, so the modality is
 * set to keyboard first: the previous input was often a pointer press, which would leave the
 * destination without a ring (see "Focus moved by a change the user did not make" in
 * `docs/ui-accessibility.md`).
 *
 * The caller must stay mounted while the control goes away (render `null`, not unmount): the check
 * runs in the caller's own effect. */
export function useFocusHandoff(restore: () => void): {
  onFocus: (event: React.FocusEvent) => void;
  onBlur: (event: React.FocusEvent) => void;
} {
  const held = useRef<Element | null>(null);
  useEffect(() => {
    const el = held.current;
    if (!el || (el.isConnected && !el.matches(":disabled"))) return;
    held.current = null;
    if (document.activeElement !== document.body && document.activeElement !== el) return;
    ringForAutomaticFocus();
    restore();
    if (document.activeElement === document.body) focusTopBar();
  });
  return {
    onFocus: (event) => {
      held.current = event.target;
    },
    onBlur: (event) => {
      if (document.activeElement !== event.target && !(event.target as Element).matches(":disabled")) held.current = null;
    },
  };
}
