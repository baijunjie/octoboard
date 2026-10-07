import { useEffect, useRef, useState } from "react";

import { useIsNarrow } from "./breakpoint";
import { useDockedPanelVisible, type DockedPanel } from "./panelVisibility";

/** Whether the pane holds keyboard focus: a tree row reached by Tab, or the report's iframe. */
function holdsFocus(panel: DockedPanel): boolean {
  const panes = document.querySelectorAll(`[data-pane=${panel}]`);
  return Array.from(panes).some((pane) => pane.contains(document.activeElement));
}

/** Hands focus to the terminal if the pane holds it, before the pane is hidden: hiding a focused
 * element leaves focus on `<body>`, where the terminal no longer receives keystrokes. */
function releaseFocus(panel: DockedPanel, focusTerminal: () => void): void {
  if (holdsFocus(panel)) focusTerminal();
}

/** How long a floating pane waits after the pointer leaves it before sliding away, so a pointer
 * that briefly overshoots does not close it. */
const PEEK_HIDE_DELAY_MS = 200;

/** One side's hover reveal: a hidden docked pane floating in over the terminal on request. */
export interface PanePeek {
  /** Whether the pane is currently floating. Never true below the breakpoint or while it is docked. */
  active: boolean;
  /** Floats the pane in, as the pointer reaches the window's edge or the toggle; does nothing
   * where the pane cannot float, and closes the other side's floating pane. `armHide` starts the
   * slide-away timer at once, for a pointer that is not on anything that would later report
   * leaving: the pane is still sliding in under it, and the pane's own enter cancels the timer. */
  reveal: (armHide?: boolean) => void;
  /** The pointer is on the floating pane again: cancels a pending slide-away. */
  keep: () => void;
  /** The pointer left whatever kept the floating pane up: slides it away after a short delay. */
  leave: () => void;
}

interface PeekControl extends PanePeek {
  peekable: boolean;
  /** Ends the peek at once. Focus inside the pane goes to the terminal first unless `handFocus` is
   * false, for a pane that is being docked and so stays on screen. */
  hide: (handFocus?: boolean) => void;
}

/** The state, timer and focus hand-off of one pane's hover reveal; `peekable` is whether the pane
 * may float right now (wide window, docked pane hidden, and the pane exists at all). */
function usePanePeek(panel: DockedPanel, peekable: boolean, focusTerminal: () => void): PeekControl {
  const [active, setActive] = useState(false);
  const activeRef = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  const setPeek = (value: boolean) => {
    activeRef.current = value;
    setActive(value);
  };
  const hide = (handFocus = true) => {
    clearTimeout(timer.current);
    if (!activeRef.current) return;
    if (handFocus) releaseFocus(panel, focusTerminal);
    setPeek(false);
  };
  const leave = () => {
    clearTimeout(timer.current);
    if (!activeRef.current) return;
    // A menu opened from the floating pane lives in a popover outside it, so the pointer
    // moving onto the menu looks like leaving: while a popup trigger inside is expanded, keep
    // waiting. So it does while focus is inside the pane: closing it would move focus to the
    // terminal mid-typing, and the rest of the keystrokes would reach the agent. Escape and the
    // toggle still close it.
    const tick = () => {
      const popupOpen = document.querySelector(`[data-pane=${panel}] [aria-haspopup][aria-expanded=true]`);
      if (holdsFocus(panel) || popupOpen) {
        timer.current = setTimeout(tick, PEEK_HIDE_DELAY_MS);
      } else {
        hide();
      }
    };
    timer.current = setTimeout(tick, PEEK_HIDE_DELAY_MS);
  };
  const reveal = (armHide = false) => {
    clearTimeout(timer.current);
    if (!peekable) return;
    setPeek(true);
    if (armHide) leave();
  };

  // A floating pane only means something while it is hidden at and above the breakpoint.
  useEffect(() => {
    if (!peekable) hide();
  }, [peekable]);
  useEffect(() => () => clearTimeout(timer.current), []);

  return { active, peekable, reveal, keep: () => clearTimeout(timer.current), leave, hide };
}

export interface PaneToggles {
  sidebarOpen: boolean;
  reportOpen: boolean;
  sidebarDocked: boolean;
  reportDocked: boolean;
  /** The hidden docked sidebar floating over the terminal because the pointer asked for it. */
  sidebarPeek: PanePeek;
  /** The same for the report panel, from the end edge or the report toggle. */
  reportPeek: PanePeek;
  sidebarShown: boolean;
  reportShown: boolean;
  toggleSidebar: () => void;
  toggleReport: () => void;
  closeSidebar: () => void;
  closeReport: () => void;
  /** Closes every overlay that is open, a narrow-mode drawer or a floating pane, as Escape does;
   * does nothing while none is, so focus inside a docked pane stays put. */
  dismissOverlays: () => void;
}

/**
 * The state behind the top bar's two panel toggles. Below the `docked` breakpoint the sidebar and
 * the report panel are closed-by-default overlays (their own components' doc comments have the
 * layout); at and above it the same toggles show or hide the docked panes instead
 * (`panelVisibility.ts`). Both states live in one place since the toggles sit in the top bar, and
 * widening the window past the breakpoint has to be able to close either overlay.
 *
 * While a docked pane is hidden, the pointer can float it in over the terminal without resizing
 * it (`sidebarPeek`, `reportPeek`): the window's edge on its side and its toggle reveal it,
 * leaving it (or Escape) hides it again, and pressing the toggle docks it for good. The two never
 * float together: revealing one hides the other.
 *
 * `hasReportPanel` is whether the selected session has a report panel at all (only a console
 * session does); `focusTerminal` is where keyboard focus goes when the pane holding it is hidden.
 */
export function usePaneToggles({
  hasReportPanel,
  focusTerminal,
}: {
  hasReportPanel: boolean;
  focusTerminal: () => void;
}): PaneToggles {
  const isNarrow = useIsNarrow();
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [reportOpen, setReportOpen] = useState(false);
  const [sidebarDocked, setSidebarDocked] = useDockedPanelVisible("sidebar");
  const [reportDocked, setReportDocked] = useDockedPanelVisible("report");
  const sidebarPeek = usePanePeek("sidebar", !isNarrow && !sidebarDocked, focusTerminal);
  const reportPeek = usePanePeek("report", !isNarrow && !reportDocked && hasReportPanel, focusTerminal);
  const revealPeek = (mine: PeekControl, other: PeekControl) => (armHide?: boolean) => {
    if (!mine.peekable) return;
    other.hide();
    mine.reveal(armHide);
  };

  // An overlay left open stops meaning anything once the window is wide enough to show its
  // content in the row instead — without this, widening past the breakpoint with a drawer open
  // would leave the scrim gone (`docked:hidden`) but the drawer's own `open` state still true, so
  // reopening it narrow again would show it already open with no toggle press in between. A
  // drawer closing into a docked pane the user has hidden takes focus along with it, so that is
  // handed to the terminal first. This catches focus still inside the pane only because React
  // treats the media query's `change` event as discrete and runs this effect before the browser
  // applies the hidden pane's `docked:invisible` in that frame's style recalc.
  useEffect(() => {
    if (isNarrow) return;
    if (!sidebarDocked) releaseFocus("sidebar", focusTerminal);
    if (!reportDocked) releaseFocus("report", focusTerminal);
    setSidebarOpen(false);
    setReportOpen(false);
  }, [isNarrow]);

  // Selecting a session whose console has no report panel leaves `reportOpen` with nothing to
  // mean: the panel stops rendering (it only exists for a console session), but a scrim rendered on
  // `reportOpen` alone would still dim the whole viewport with no toggle left to close it.
  useEffect(() => {
    if (!hasReportPanel) setReportOpen(false);
  }, [hasReportPanel]);

  const closeSidebar = () => {
    releaseFocus("sidebar", focusTerminal);
    setSidebarOpen(false);
  };
  const closeReport = () => {
    releaseFocus("report", focusTerminal);
    setReportOpen(false);
  };

  const dismissOverlays = () => {
    if (!sidebarOpen && !reportOpen && !sidebarPeek.active && !reportPeek.active) return;
    // Only a drawer that is open: closing one runs `releaseFocus`, which would pull focus out of a
    // docked pane that is not being hidden.
    if (sidebarOpen) closeSidebar();
    if (reportOpen) closeReport();
    sidebarPeek.hide();
    reportPeek.hide();
  };

  // Opening either drawer closes the other — without this, both scrims can be on screen at once,
  // and since they stack in DOM order, a press only ever reaches the later one.
  const toggleSidebar = () => {
    if (!isNarrow) {
      // Docking a floating pane keeps it on screen, where it is now in the row, so focus stays.
      if (sidebarDocked) releaseFocus("sidebar", focusTerminal);
      else sidebarPeek.hide(false);
      return setSidebarDocked(!sidebarDocked);
    }
    if (sidebarOpen) return closeSidebar();
    setSidebarOpen(true);
    closeReport();
  };
  const toggleReport = () => {
    if (!isNarrow) {
      if (reportDocked) releaseFocus("report", focusTerminal);
      else reportPeek.hide(false);
      return setReportDocked(!reportDocked);
    }
    if (reportOpen) return closeReport();
    setReportOpen(true);
    closeSidebar();
  };

  // Closes whichever drawer is open, or a floating pane: a scrim already does this on a press,
  // Escape needs its own listener to do the same. Registered on the capture phase: xterm.js owns
  // Escape too (it forwards the keystroke to the running agent) and stops it bubbling once it has,
  // so a bubble listener here would never see the key while the terminal holds focus, which is
  // most of the time.
  //
  // Scoped to origins marked `data-escape-scope` (the terminal pane and the drawers themselves)
  // rather than every keydown: a dialog or a menu popover is portalled to `<body>`, outside both,
  // and closes itself from its own bubble-phase `onKeyDown` — stopping propagation unconditionally
  // here reached the key first and ate it before react-aria's own handler ever saw it.
  //
  // A target of `<body>` (or no target at all) is treated as in scope too: that is what a fresh
  // narrow-mode window has focused once a drawer is open but no session has ever been selected, so
  // `term.focus()` has never run. react-aria keeps focus inside an open dialog or menu, so an
  // overlay's Escape normally cannot arrive that way. The one exception is a dialog whose focused
  // control has just unmounted, which drops focus to `<body>`: Escape then closes the drawer under
  // the dialog rather than the dialog. That state already costs the dialog its own Escape, which is
  // what `useRefocusIfLost` in `dialogs/Dialog.tsx` exists to prevent, so it is not worth a second
  // guard here.
  useEffect(() => {
    if (!sidebarOpen && !reportOpen && !sidebarPeek.active && !reportPeek.active) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      const target = event.target as Element | null;
      if (target && target !== document.body && !target.closest("[data-escape-scope]")) return;
      // Consumed here rather than also reaching the terminal: with a drawer open, Escape closes
      // it, not whatever the running agent would have done with it.
      event.stopPropagation();
      dismissOverlays();
    };
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [sidebarOpen, reportOpen, sidebarPeek.active, reportPeek.active]);

  return {
    sidebarOpen,
    reportOpen,
    sidebarDocked,
    reportDocked,
    sidebarPeek: { ...sidebarPeek, reveal: revealPeek(sidebarPeek, reportPeek) },
    reportPeek: { ...reportPeek, reveal: revealPeek(reportPeek, sidebarPeek) },
    sidebarShown: isNarrow ? sidebarOpen : sidebarDocked,
    reportShown: isNarrow ? reportOpen : reportDocked,
    toggleSidebar,
    toggleReport,
    closeSidebar,
    closeReport,
    dismissOverlays,
  };
}
