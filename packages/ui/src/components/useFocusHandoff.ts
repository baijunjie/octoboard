import { useEffect, useRef } from "react";

import { focusTopBar } from "./focusTopBar";

/** Hands keyboard focus on when the control that held it goes away. A focused element that is
 * removed fires no blur and focus falls to `<body>`, so the caller spreads the returned
 * `onFocus`/`onBlur` on a wrapper of the control, which remembers the element focus is on, and
 * after every render this checks whether that element has left the document. A blur of the whole
 * window (the browser's own prompt taking focus) leaves the element the active element and keeps it
 * remembered. Focus moved by script while the window is unfocused fires nothing either, so the
 * hand-off also needs focus to have actually fallen to `<body>`. It then goes where `restore` puts
 * it — the terminal, say, or a control beside the one that went — or to the top bar's first
 * enabled button when that leaves it on `<body>` (`focusTopBar`).
 *
 * The caller must stay mounted while the control goes away (render `null`, not unmount): the check
 * runs in the caller's own effect.
 *
 * This is the post-hoc half of the pair. Where the code taking the control away can name where
 * focus should go instead, `handFocusOff` moves it before anything is removed. */
export function useFocusHandoff(restore: () => void): {
  onFocus: (event: React.FocusEvent) => void;
  onBlur: (event: React.FocusEvent) => void;
} {
  const held = useRef<Element | null>(null);
  useEffect(() => {
    const el = held.current;
    if (!el || el.isConnected) return;
    held.current = null;
    if (document.activeElement !== document.body) return;
    restore();
    if (document.activeElement === document.body) focusTopBar();
  });
  return {
    onFocus: (event) => {
      held.current = event.target;
    },
    onBlur: (event) => {
      if (document.activeElement !== event.target) held.current = null;
    },
  };
}
