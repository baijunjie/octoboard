import { useEffect, useState } from "react";

import { useIsNarrow } from "./breakpoint";
import { useDockedPanelVisible, type DockedPanel } from "./panelVisibility";

/** If the pane holds keyboard focus (a tree row reached by Tab, the report's iframe), hands it to
 * the terminal before the pane is hidden: hiding a focused element leaves focus on `<body>`, where
 * the terminal no longer receives keystrokes. */
function releaseFocus(panel: DockedPanel, focusTerminal: () => void): void {
  const pane = document.querySelector(`[data-pane=${panel}]`);
  if (pane?.contains(document.activeElement)) focusTerminal();
}

export interface PaneToggles {
  sidebarOpen: boolean;
  reportOpen: boolean;
  sidebarDocked: boolean;
  reportDocked: boolean;
  sidebarShown: boolean;
  reportShown: boolean;
  toggleSidebar: () => void;
  toggleReport: () => void;
  closeSidebar: () => void;
  closeReport: () => void;
}

/**
 * The state behind the top bar's two panel toggles. Below the `docked` breakpoint the sidebar and
 * the report panel are closed-by-default overlays (their own components' doc comments have the
 * layout); at and above it the same toggles show or hide the docked panes instead
 * (`panelVisibility.ts`). Both states live in one place since the toggles sit in the top bar, and
 * widening the window past the breakpoint has to be able to close either overlay.
 *
 * `hasReportPanel` is whether the selected session has a report panel at all (only a hub session
 * does); `focusTerminal` is where keyboard focus goes when the pane holding it is hidden.
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

  // An overlay left open stops meaning anything once the window is wide enough to show its
  // content in the row instead — without this, widening past the breakpoint with a drawer open
  // would leave `docked:hidden`'s sibling, the scrim, also gone (it is hidden the same way), but
  // the drawer's own `open` state would still be true, so reopening it narrow again would show it
  // already open with no toggle press in between. A drawer closing into a docked pane the user has
  // hidden takes focus along with it, so that is handed to the terminal first. This catches focus
  // still inside the pane only because React treats the media query's `change` event as discrete
  // and runs this effect before the browser applies `docked:hidden` in that frame's style recalc.
  useEffect(() => {
    if (isNarrow) return;
    if (!sidebarDocked) releaseFocus("sidebar", focusTerminal);
    if (!reportDocked) releaseFocus("report", focusTerminal);
    setSidebarOpen(false);
    setReportOpen(false);
  }, [isNarrow]);

  // Selecting a session whose console has no report panel leaves `reportOpen` with nothing to
  // mean: the panel stops rendering (it only exists for a hub session), but a scrim rendered on
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

  // Opening either drawer closes the other — without this, both scrims can be on screen at once,
  // and since they stack in DOM order, a press only ever reaches the later one.
  const toggleSidebar = () => {
    if (!isNarrow) {
      if (sidebarDocked) releaseFocus("sidebar", focusTerminal);
      return setSidebarDocked(!sidebarDocked);
    }
    if (sidebarOpen) return closeSidebar();
    setSidebarOpen(true);
    closeReport();
  };
  const toggleReport = () => {
    if (!isNarrow) {
      if (reportDocked) releaseFocus("report", focusTerminal);
      return setReportDocked(!reportDocked);
    }
    if (reportOpen) return closeReport();
    setReportOpen(true);
    closeSidebar();
  };

  // Closes whichever drawer is open — both scrims already do this on a press, Escape needs its
  // own listener to do the same. Registered on the capture phase: xterm.js owns Escape too (it
  // forwards the keystroke to the running agent) and stops it bubbling once it has, so a bubble
  // listener here would never see the key while the terminal holds focus, which is most of the
  // time.
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
    if (!sidebarOpen && !reportOpen) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      const target = event.target as Element | null;
      if (target && target !== document.body && !target.closest("[data-escape-scope]")) return;
      // Consumed here rather than also reaching the terminal: with a drawer open, Escape closes
      // it, not whatever the running agent would have done with it.
      event.stopPropagation();
      closeSidebar();
      closeReport();
    };
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [sidebarOpen, reportOpen]);

  return {
    sidebarOpen,
    reportOpen,
    sidebarDocked,
    reportDocked,
    sidebarShown: isNarrow ? sidebarOpen : sidebarDocked,
    reportShown: isNarrow ? reportOpen : reportDocked,
    toggleSidebar,
    toggleReport,
    closeSidebar,
    closeReport,
  };
}
