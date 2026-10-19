import { useFocusVisible } from "react-aria";
import { useEffect, useRef, useState } from "react";

import { useLatest } from "../components/useLatest";
import { isImeKey } from "../imeKey";
import { useIsNarrow } from "./breakpoint";
import { useDockedPanelVisible } from "./panelVisibility";
import { type ConsolePeek, useConsolePeek } from "./useConsolePeek";
import { MODAL_OPEN } from "./useRegionCycle";

/** Elements that take typed text, as the Escape listener sees them: xterm.js's hidden `textarea`
 * is not one, though it is one to the DOM, for the terminal reads its keys itself. */
const EDITABLE = 'input, textarea:not(.xterm-helper-textarea), [contenteditable]:not([contenteditable="false"])';

/** The input types that take typed text. Any other `input` (a checkbox, a radio, a range, a
 * button) has a `value` that is no text the user typed, and no Escape of its own to keep. `number`
 * is left out: its `value` is a number the field may have been given, so it would hold text
 * whatever was typed, and nothing in it clears on Escape. */
const TEXT_INPUT_TYPES = new Set(["text", "search", "email", "url", "tel", "password"]);

/** Whether `target` is a field with text in it, whose Escape comes before the pane's: the field
 * clears itself, and the pane is left for the next Escape, with the field empty. A field already
 * empty has nothing to clear, so the key goes on to the pane; so does one that cannot be edited
 * (read-only, disabled), which has no Escape of its own. */
export function holdsText(target: Element | null): boolean {
  const field = target?.closest(EDITABLE);
  if (!field) return false;
  if (field instanceof HTMLInputElement) {
    return TEXT_INPUT_TYPES.has(field.type) && !field.readOnly && !field.disabled && field.value !== "";
  }
  if (field instanceof HTMLTextAreaElement) return !field.readOnly && !field.disabled && field.value !== "";
  // Anything else matched is `contenteditable`.
  return field.textContent !== "";
}

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
  reveal: () => void;
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

/** The state, timer and focus hand-off of one pane's hover reveal; `peekable` is whether the pane
 * may float right now (the pane is not docked, and exists at all; the sidebar's also only in a wide
 * window). */
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
      if (focusKeeps() || popupOpen || pointerOn || document.querySelector(MODAL_OPEN)) {
        timer.current = setTimeout(tick, PEEK_HIDE_DELAY_MS);
      } else {
        hide();
      }
    };
    timer.current = setTimeout(tick, PEEK_HIDE_DELAY_MS);
  };
  const reveal = () => {
    clearTimeout(timer.current);
    if (!peekable) return;
    setPeek(true);
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
  /** Closes every overlay that is open, the sidebar's drawer or a floating pane, as Escape does;
   * does nothing while none is, so focus inside a docked pane stays put. */
  dismissOverlays: () => void;
}

/**
 * The state behind the sidebar's toggle in the top bar, and the hover reveal of the aside.
 * Below the `docked` breakpoint the sidebar is a closed-by-default overlay (its component's doc
 * comment has the layout); at and above it the toggle shows or hides the docked pane instead
 * (`panelVisibility.ts`). Both states live in one place since the toggle sits in the window chrome,
 * and widening the window past the breakpoint has to be able to close the overlay.
 *
 * The aside has no toggle or drawer of its own: it is docked at every width while a panel is open
 * (`panels/stackState.ts`), and with none open it floats in over the terminal while the pointer
 * rests on a panel kind's toggle (`asidePeek`), showing that panel; pressing the toggle docks it.
 * Likewise a hidden docked sidebar floats in from its toggle or a console's avatar on the rail
 * (`sidebarPeek`). Leaving a floating pane (or Escape) hides it again. The two never float
 * together: revealing one hides the other.
 *
 * `hasSidebar` is whether the sidebar is rendered at all (it is not while there is no console);
 * `asidePeekable` is whether the aside may float in now: something owns it and it has no panel
 * open; `focusTerminal` is where keyboard focus goes when the pane holding it is hidden.
 */
export function usePaneToggles({
  hasSidebar,
  asidePeekable,
  focusTerminal,
}: {
  hasSidebar: boolean;
  asidePeekable: boolean;
  focusTerminal: () => void;
}): PaneToggles {
  const isNarrow = useIsNarrow();
  const keyboardInput = useLatest(useFocusVisible().isFocusVisible);
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [sidebarDocked, setSidebarDocked] = useDockedPanelVisible("sidebar");
  const sidebarPeek = usePanePeek("sidebar", !isNarrow && !sidebarDocked && hasSidebar, focusTerminal);
  const asidePeek = usePanePeek("aside", asidePeekable, focusTerminal, () => keyboardInput.current);
  const revealPeek = (mine: PeekControl, other: PeekControl) => () => {
    if (!mine.peekable) return;
    other.hide();
    mine.reveal();
  };
  const consolePeek = useConsolePeek({ ...sidebarPeek, reveal: revealPeek(sidebarPeek, asidePeek) });

  // The drawer left open stops meaning anything once the window is wide enough to show its
  // content in the row instead — without this, widening past the breakpoint with it open
  // would leave the scrim gone (`docked:hidden`) but the drawer's own `open` state still true, so
  // reopening it narrow again would show it already open with no toggle press in between. A
  // drawer closing into a docked pane the user has hidden takes focus along with it, so that is
  // handed to the terminal first. This catches focus still inside the pane only because React
  // treats the media query's `change` event as discrete and runs this effect before the browser
  // applies the hidden pane's `docked:invisible` in that frame's style recalc.
  useEffect(() => {
    if (isNarrow) return;
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
    if (!sidebarOpen && !sidebarPeek.active && !asidePeek.active) return;
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
  // `textarea` that holds the terminal's focus: Escape there is the terminal's to forward, and
  // still closes the drawer. An input method's Escape in the terminal is its composition's, and
  // closes nothing.
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
    if (!sidebarOpen && !sidebarPeek.active && !asidePeek.active) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      // An element marking itself `data-escape-topmost` is a floating panel over the drawer that
      // closes first, on a listener of its own that runs after this one.
      if (document.querySelector("[data-escape-topmost]")) return;
      const target = event.target as Element | null;
      if (target && target !== document.body && !target.closest("[data-escape-scope]")) return;
      if (holdsText(target) || isImeKey(event)) return;
      // Consumed here rather than also reaching the terminal: with a drawer open, Escape closes
      // it, not whatever the running agent would have done with it.
      event.stopPropagation();
      dismissOverlays();
    };
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [sidebarOpen, sidebarPeek.active, asidePeek.active]);

  return {
    sidebarOpen,
    sidebarDocked,
    sidebarPeek: { active: sidebarPeek.active, keep: sidebarPeek.keep, leave: sidebarPeek.leave, ...consolePeek },
    // The aside's float would come out over the sidebar's open drawer, so the drawer makes way.
    asidePeek: {
      ...asidePeek,
      reveal: () => {
        if (!asidePeek.peekable) return;
        if (sidebarOpen) closeSidebar();
        revealPeek(asidePeek, sidebarPeek)();
      },
    },
    sidebarShown: isNarrow ? sidebarOpen : sidebarDocked,
    toggleSidebar,
    closeSidebar,
    dismissOverlays,
  };
}
