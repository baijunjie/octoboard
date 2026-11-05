import { useFocusVisible } from "react-aria";
import { useEffect, useRef, useState } from "react";

import { keepsEscape } from "../components/focus/keepsEscape";
import { useLatest } from "../components/hooks/useLatest";
import { isImeKey } from "../input/imeKey";
import { CANNOT_HOVER } from "../input/touchInput";
import { useIsNarrow, useMediaQuery } from "../components/hooks/breakpoint";
import { useDockedPanelVisible } from "./panelVisibility";
import { type ConsolePeek, useConsolePeek } from "./useConsolePeek";
import { MODAL_OPEN } from "../components/focus/selectors";

/** The two panes that float in on hover. */
type Pane = "sidebar" | "aside";

/** Whether the pane holds keyboard focus: a tree row reached by Tab, or a control or frame in the aside. */
function holdsFocus(panel: Pane): boolean {
  const panes = document.querySelectorAll(`[data-pane=${panel}]`);
  return Array.from(panes).some((pane) => pane.contains(document.activeElement));
}

/** Hands focus to the terminal if the pane holds it, before the pane is hidden: hiding a focused
 * element leaves focus on `<body>`, where the terminal no longer receives keystrokes. */
function releaseFocus(panel: Pane, focusTerminal: () => void): void {
  if (holdsFocus(panel)) focusTerminal();
}

/** How long a floating pane waits after the pointer leaves it before sliding away, so a pointer
 * that briefly overshoots does not close it. */
const PEEK_HIDE_DELAY_MS = 200;

/** One side's hover reveal: a hidden pane floating in over the terminal on request. */
export interface PanePeek {
  /** Whether the pane is currently floating. Never true while it is docked. */
  active: boolean;
  /** Floats the pane in, as the pointer rests on whatever brings it up (a panel kind's toggle for
   * the aside; the sidebar's is its toggle and the rail's avatars, through `useConsolePeek`); does
   * nothing where the pane cannot float, and closes the other side's floating pane. It arms no slide-away timer: that
   * starts when the pointer leaves the pane or whatever brought it up (`leave`). */
  reveal: (force?: boolean) => void;
  /** The pointer is on the floating pane again: cancels a pending slide-away. */
  keep: () => void;
  /** The pointer left whatever kept the floating pane up: slides it away after a short delay. */
  leave: () => void;
}

/** The hidden docked sidebar's hover reveal: its `reveal` floats it in showing the current console,
 * and the console whose avatar on the rail the pointer rests on floats it in showing that one
 * (`useConsolePeek`). */
export type SidebarPeek = Omit<PanePeek, "reveal"> & ConsolePeek;

interface PeekControl extends PanePeek {
  peekable: boolean;
  /** Ends the peek at once. Focus inside the pane goes to the terminal first unless `handFocus` is
   * false, for a pane that is being docked and so stays on screen. */
  hide: (handFocus?: boolean) => void;
}

/** The state, timer and focus handoff of one pane's hover reveal; `peekable` is whether the pane
 * may float right now: the pane exists and is not docked, outside mobile interaction. */
function usePanePeek(panel: Pane, peekable: boolean, focusTerminal: () => void, keyboardInput?: () => boolean): PeekControl {
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
    // A menu opened from the floating pane lives in a popover outside it, so the pointer moving onto
    // the menu looks like leaving: while a popup trigger inside is expanded, keep waiting. So it does
    // while focus is inside the pane: closing it would move focus to the terminal mid-typing, and the
    // rest of the keystrokes would reach the agent. A dialog opened from the pane is outside it too,
    // with no popup trigger to show for it, so one open keeps it as well — such as the file viewer a
    // project's browser opens, which gives focus back to a row of the pane when it closes. So does
    // the pointer being on the pane, which a pane that slid in under a still pointer may never have
    // reported (`:hover` is up to date once it is there). Escape and the toggle still close it.
    // Focus that a press put in the pane (a tab, a button) keeps it only for a pane that does not
    // say whether the last input was the keyboard's (`keyboardInput`): the sidebar's floating card
    // stays up for it, the aside's, whose panels take focus on a press, goes when the pointer
    // leaves, as it did before the press; focus a key put there still keeps it.
    const focusKeeps = () => holdsFocus(panel) && (keyboardInput === undefined || keyboardInput());
    const tick = () => {
      const popupOpen = document.querySelector(`[data-pane=${panel}] [aria-haspopup][aria-expanded=true]`);
      const pointerOn = document.querySelector(`[data-pane=${panel}]:hover`);
      if (focusKeeps() || popupOpen || pointerOn || document.querySelector(MODAL_OPEN) || document.documentElement.classList.contains("pane-resizing")) {
        timer.current = setTimeout(tick, PEEK_HIDE_DELAY_MS);
      } else {
        hide();
      }
    };
    timer.current = setTimeout(tick, PEEK_HIDE_DELAY_MS);
  };
  const reveal = (force = false) => {
    clearTimeout(timer.current);
    if (!peekable && !force) return;
    setPeek(true);
  };

  // A floating pane only means something while no docked column takes its place. The aside
  // stops being peekable as a column takes its place (a maximized card restored, a panel docked),
  // and that column may hold the focus: it is not the floating pane's to hand on.
  useEffect(() => {
    if (!peekable) hide(panel !== "aside");
  }, [peekable]);
  useEffect(() => () => clearTimeout(timer.current), []);

  return { active, peekable, reveal, keep: () => clearTimeout(timer.current), leave, hide };
}

export interface PaneToggles {
  sidebarOpen: boolean;
  sidebarDocked: boolean;
  /** The hidden docked sidebar floating over the terminal, from its toggle or a console's avatar. */
  sidebarPeek: SidebarPeek;
  /** The aside's floating, from a panel kind's toggle while no panel is open. */
  asidePeek: PanePeek & {
    /** Ends the float at once; focus inside the pane goes to the terminal first unless `handFocus`
     * is false, for a pane that is being docked and so stays on screen. */
    hide: (handFocus?: boolean) => void;
  };
  sidebarShown: boolean;
  toggleSidebar: () => void;
  closeSidebar: () => void;
  /** Closes every overlay that is open, the sidebar's drawer, a floating pane or the panel floating
   * over the terminal in a window too narrow to dock the aside, as Escape does;
   * does nothing while none is, so focus inside a docked pane stays put. */
  dismissOverlays: () => void;
}

/**
 * The state behind the sidebar's toggle in the top bar, and the hover reveal of the aside.
 * When the sidebar cannot fit, it is a closed-by-default overlay (its component's doc
 * comment has the layout); when it fits the toggle shows or hides the docked pane instead
 * (`panelVisibility.ts`). Both states live in one place since the toggle sits in the window chrome,
 * and widening the window until it fits has to be able to close the overlay.
 *
 * The aside's stored panel stack is independent of its docking choice. A floating group uses
 * `asidePeek` to reveal all its open cards; with none open it previews a hovered kind instead.
 * Mobile interaction (`useAsideFloats`) shows only one panel at a time.
 * Likewise a hidden docked sidebar floats in from its toggle or a console's avatar on the rail
 * (`sidebarPeek`). Leaving a floating pane (or Escape) hides it again. The two never float
 * together: revealing one hides the other.
 *
 * `isNarrow` means the sidebar cannot fit beside the terminal.
 * `hasSidebar` is whether the sidebar is rendered at all (it is not while there is no console);
 * `asidePeekable` is whether the aside may reveal its floating group or preview an empty docked
 * column, with an owner and outside mobile interaction. Neither pane floats in on a
 * device whose primary input cannot hover. `focusTerminal` is where keyboard focus goes when the
 * pane holding it is hidden.
 * `drawer` is the drawer's open state when the caller keeps it, for what has to know it before this
 * hook runs (`panels/useFloatingPanel.ts`); left out, the hook keeps its own.
 */
export function usePaneToggles({
  hasSidebar,
  isNarrow,
  asidePeekable,
  focusTerminal,
  floatingPanel = false,
  closeFloatingPanel,
  drawer,
}: {
  hasSidebar: boolean;
  isNarrow: boolean;
  asidePeekable: boolean;
  focusTerminal: () => void;
  /** A panel floats over the agent terminal in a window too narrow to dock the aside: an overlay
   * like the drawer, which Escape and `dismissOverlays` close (`closeFloatingPanel`). */
  floatingPanel?: boolean;
  closeFloatingPanel?: () => void;
  drawer?: readonly [boolean, (open: boolean) => void];
}): PaneToggles {
  const closeFloating = useLatest(closeFloatingPanel);
  const mobile = useIsNarrow();
  // A device whose primary input cannot hover (a phone or a tablet's touch screen) floats no pane in:
  // a tap on a toggle or an avatar is a press, never a rest of the pointer.
  const canHover = !useMediaQuery(() => CANNOT_HOVER);
  const keyboardInput = useLatest(useFocusVisible().isFocusVisible);
  const ownDrawer = useState(false);
  const [sidebarOpen, setSidebarOpen] = drawer ?? ownDrawer;
  const [sidebarDocked, setSidebarDocked] = useDockedPanelVisible("sidebar");
  const sidebarPeek = usePanePeek("sidebar", canHover && !mobile && (isNarrow || !sidebarDocked) && hasSidebar, focusTerminal);
  const asidePeek = usePanePeek("aside", canHover && asidePeekable, focusTerminal, () => keyboardInput.current);
  const revealPeek = (mine: PeekControl, other: PeekControl) => () => {
    if (!mine.peekable) return;
    other.hide();
    mine.reveal();
  };
  const consolePeek = useConsolePeek({ ...sidebarPeek, reveal: revealPeek(sidebarPeek, asidePeek) });

  // The drawer left open stops meaning anything once the window is wide enough to show its
  // content in the row instead — without this, widening past the breakpoint with it open
  // would leave the scrim gone (`sidebar-docked:hidden`) but the drawer's own `open` state still true, so
  // reopening it narrow again would show it already open with no toggle press in between. A
  // drawer closing into a docked pane the user has hidden takes focus along with it, so that is
  // handed to the terminal first. This catches focus still inside the pane only because React
  // treats the media query's `change` event as discrete and runs this effect before the browser
  // applies the hidden pane's `sidebar-docked:invisible` in that frame's style recalc.
  //
  // Narrowing beyond the sidebar's capacity turns the docked sidebar into the closed drawer, out of reach
  // (inert), so focus inside it is handed to the terminal before it falls to `<body>`.
  useEffect(() => {
    if (isNarrow) {
      if (!sidebarOpen) releaseFocus("sidebar", focusTerminal);
      return;
    }
    if (!sidebarDocked) releaseFocus("sidebar", focusTerminal);
    setSidebarOpen(false);
  }, [isNarrow]);

  // The sidebar going away (the last console deleted, perhaps by another client) leaves
  // `sidebarOpen` with nothing to mean: a drawer left open would keep the Escape listener armed with
  // nothing to close, and would be open already when a console comes back.
  useEffect(() => {
    if (!hasSidebar) setSidebarOpen(false);
  }, [hasSidebar]);

  const closeSidebar = () => {
    releaseFocus("sidebar", focusTerminal);
    setSidebarOpen(false);
  };

  const dismissOverlays = () => {
    if (!sidebarOpen && !sidebarPeek.active && !asidePeek.active && !floatingPanel) return;
    if (floatingPanel) closeFloating.current?.();
    // Only a drawer that is open: closing one runs `releaseFocus`, which would pull focus out of a
    // docked pane that is not being hidden.
    if (sidebarOpen) closeSidebar();
    sidebarPeek.hide();
    asidePeek.hide();
  };

  // Opening the drawer hides the aside's floating pane — without this, both can be on screen at
  // once, and since they stack in DOM order, a press only ever reaches the later one.
  const toggleSidebar = () => {
    if (!isNarrow) {
      // Docking a floating pane keeps it on screen, where it is now in the row, so focus stays.
      if (sidebarDocked) releaseFocus("sidebar", focusTerminal);
      else sidebarPeek.hide(false);
      return setSidebarDocked(!sidebarDocked);
    }
    if (!mobile) {
      if (canHover) revealPeek(sidebarPeek, asidePeek)();
      else {
        setSidebarOpen(true);
        asidePeek.hide();
      }
      return;
    }
    if (sidebarOpen) return closeSidebar();
    setSidebarOpen(true);
    asidePeek.hide();
  };

  // Closes the drawer when it is open, or a floating pane: a scrim already does this on a press,
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
  // A key aimed at an editable element with text in it is left to it: a field inside a scope (the
  // change list's filter) has its own Escape (clearing it, cancelling an input method's
  // composition), which a capture listener on the window would otherwise pre-empt, so the pane is
  // dismissed by the next Escape, once the field is empty. Not xterm.js's hidden input, a
  // `textarea` that holds the terminal's focus: Escape there is the agent terminal's to forward,
  // and still closes the drawer — except in a shell terminal of the right pane, which keeps it
  // (`keepsEscape`). An input method's Escape in the terminal is its composition's, and closes
  // nothing.
  //
  // A target of `<body>` (or no target at all) is treated as in scope too: that is what a fresh
  // narrow-mode window has focused once a drawer is open but no session has ever been selected, so
  // `term.focus()` has never run. react-aria keeps focus inside an open dialog or menu, so an
  // overlay's Escape normally cannot arrive that way. The one exception is a dialog whose focused
  // control has just unmounted, which drops focus to `<body>`: Escape then closes the drawer under
  // the dialog rather than the dialog. That state already costs the dialog its own Escape, which is
  // what `useRefocusIfLost` in `components/dialog/Dialog.tsx` exists to prevent, so it is not worth a second
  // guard here.
  useEffect(() => {
    if (!sidebarOpen && !sidebarPeek.active && !asidePeek.active && !floatingPanel) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      // An element marking itself `data-escape-topmost` is a floating panel over the drawer that
      // closes first, on a listener of its own that runs after this one.
      if (document.querySelector("[data-escape-topmost]")) return;
      const target = event.target as Element | null;
      if (target && target !== document.body && !target.closest("[data-escape-scope]")) return;
      if (keepsEscape(target) || isImeKey(event)) return;
      // Consumed here rather than also reaching the terminal: with a drawer open, Escape closes
      // it, not whatever the running agent would have done with it.
      event.stopPropagation();
      dismissOverlays();
    };
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [sidebarOpen, sidebarPeek.active, asidePeek.active, floatingPanel]);

  return {
    sidebarOpen,
    sidebarDocked,
    sidebarPeek: { active: sidebarPeek.active, keep: sidebarPeek.keep, leave: sidebarPeek.leave, ...consolePeek },
    // The aside's float would come out over the sidebar's open drawer, so the drawer makes way.
    asidePeek: {
      ...asidePeek,
      reveal: (force = false) => {
        if (!asidePeek.peekable && !force) return;
        if (sidebarOpen) closeSidebar();
        sidebarPeek.hide();
        asidePeek.reveal(force);
      },
    },
    sidebarShown: isNarrow ? sidebarOpen || (!mobile && sidebarPeek.active) : sidebarDocked,
    toggleSidebar,
    closeSidebar,
    dismissOverlays,
  };
}
