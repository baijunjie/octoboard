import { useState, type RefObject } from "react";
import { flushSync } from "react-dom";

import { MODAL_CLOSE_SETTLE_MS } from "../components/focus/modalFocus";
import { MODAL_DIALOG } from "../components/focus/selectors";
import { tryEachFrame } from "../components/focus/tryEachFrame";
import { useStableHandler } from "../components/hooks/useLatest";
import type { QuoteTarget } from "../components/host/quoteTarget";
import type { TerminalPaneHandle } from "../terminal/TerminalPane";

/** The modal dialog in front: the one a quote made from a viewer is about to close. */
const foremostModal = (): Element | undefined =>
  Array.from(document.querySelectorAll(MODAL_DIALOG)).reverse().find((dialog) => !dialog.closest('[inert], [aria-hidden="true"]'));

/** The quote target of the window: the session terminal pane behind `terminalRef`, read when asked, so the value
 * keeps its identity for good and nothing that reads it renders as the session changes. Only from an event or a
 * microtask does `revealInput` run, not while rendering or from an effect (it commits synchronously). `focusTerminal` is the
 * window's, so a tap still leaves the terminal unfocused for the on-screen keyboard, and must keep its identity (it is
 * captured once). `uncoverTerminal` clears what covers the terminal's area (a maximized or floating panel); it may
 * change with every render, as it closes over the layout, and the latest one is read when `revealInput` runs. */
export function useTerminalQuoteTarget(
  terminalRef: RefObject<TerminalPaneHandle | null>,
  focusTerminal: () => void,
  uncoverTerminal: () => void,
): QuoteTarget {
  const uncover = useStableHandler(uncoverTerminal);
  const [target] = useState<QuoteTarget>(() => {
    let cancelFocus: (() => void) | undefined;
    return {
      blocked: () => (terminalRef.current ? terminalRef.current.quoteBlock() : "input"),
      quote: (text) => terminalRef.current?.quote(text),
      revealInput: (closeSurface) => {
        // The dialog being closed is the foremost one. It is closed in the same commit as what covers the terminal is
        // cleared, and focus is placed at once after that commit, in the same task: the dialog's opener may be given
        // focus as it closes, but a later restore (react-aria's, a frame after the dialog unmounts) acts only on focus left
        // on `<body>` and finds it placed. Where the dialog is still in the document (an exit transition), focus waits
        // for it to go, as the opener's own return runs before; another modal in front once it has gone is the user's
        // business, as one that comes up meanwhile: focus is left to it. The wait is bounded by what a modal's own close
        // is given to settle, a deadline rather than a count of frames, which varies with the refresh rate.
        const closing = foremostModal();
        const deadline = Date.now() + MODAL_CLOSE_SETTLE_MS;
        cancelFocus?.();
        flushSync(() => {
          uncover();
          closeSurface();
        });
        const land = () => {
          if (closing?.isConnected) return Date.now() >= deadline;
          if (!document.querySelector(MODAL_DIALOG)) focusTerminal();
          return true;
        };
        if (!land()) cancelFocus = tryEachFrame(land, Infinity);
      },
    };
  });
  return target;
}
