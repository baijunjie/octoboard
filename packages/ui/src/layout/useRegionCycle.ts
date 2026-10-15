import { setInteractionModality } from "react-aria";
import { useCallback, useEffect, useRef } from "react";

import { FOCUSABLE, focusOpenDialog } from "../components/regionFocus";
import { useLatest } from "../components/useLatest";

/** The window's regions in visual order, each an element (or several) marked `data-region`. The
 * toast stack comes last and is a stop only while a toast is on screen; unlike the others it is not
 * for the caller to say: the stack is in the DOM while it holds a toast, and one that is closing
 * (kept mounted for its exit animation, `data-exiting`) is not a landing place. */
const REGIONS = ["topbar", "rail", "welcome", "sidebar", "archive", "terminal", "aside", "banner", "toast"] as const;
export type Region = (typeof REGIONS)[number];
type ShownRegion = Exclude<Region, "toast">;

/** A toast that is on screen, not one fading out. */
const LIVE_TOAST = '[data-slot="toast"]:not([data-exiting="true"]):not([data-hidden="true"])';

/** A dialog or a menu is open: ordinary regions stay unavailable. A toast is an `alertdialog`
 * too, but a non-modal one. */
export const MODAL_OPEN =
  '[role="dialog"]:not([aria-modal="false"]), [role="alertdialog"]:not([aria-modal="false"]), [role="menu"]';

function regionElements(region: Region): Element[] {
  return Array.from(document.querySelectorAll(`[data-region=${region}]`));
}

function currentRegion(): Region | undefined {
  return REGIONS.find((region) => regionElements(region).some((el) => el.contains(document.activeElement)));
}

/** Focuses where `region` takes focus and says whether it did: the sidebar goes to its selected
 * row and the rail to the current console, falling back to the first control, as the other regions
 * that are not the terminal do. The toast stack lists its newest toast first, so its first control
 * is that toast. */
function focusRegion(region: Region, focusTerminal: () => void): boolean {
  if (region === "terminal") {
    focusTerminal();
    return regionElements(region).some((el) => el.contains(document.activeElement));
  }
  const root = regionElements(region)[0];
  if (!root) return false;
  const target =
    region === "toast"
      ? root.querySelector<HTMLElement>(LIVE_TOAST)
      : ((region === "sidebar" || region === "rail") && root.querySelector<HTMLElement>('[aria-current="true"]')) ||
        root.querySelector<HTMLElement>(FOCUSABLE);
  if (!target) return false;
  target.focus();
  return target.ownerDocument.activeElement === target;
}

/**
 * F6 and Shift+F6 move keyboard focus to the next and previous region of the window, in a cycle:
 * the terminal takes Tab and Shift+Tab for the agent, so without this a keyboard user could never
 * leave it (WCAG 2.2 SC 2.1.2). `shown` says which regions are on screen right now; the others,
 * including a floating pane that is merely able to slide in, are skipped, as is a region with
 * nothing to focus. The connection banner is a stop, after the aside, while its strip is shown. The
 * toast stack joins the cycle, last, while a toast is shown.
 *
 * Registered on the window's capture phase so the key never reaches xterm.js, which would write
 * it to the agent, nor react-aria's own landmark navigation, which would move focus out from under
 * an open dialog or menu. A dialog allows only an excursion to Retry and back; menus allow none.
 * Ignored during an input method composition. `cycle` is also what the report page's key relay
 * calls, since a key pressed inside its frame never reaches this window; it follows the same
 * dialog and menu rules.
 */
export function useRegionCycle({
  shown,
  focusTerminal,
}: {
  shown: Record<ShownRegion, boolean>;
  focusTerminal: () => void;
}): { cycle: (backward: boolean) => void } {
  const latest = useLatest({ shown, focusTerminal });
  const dialogFocus = useRef<Element | null>(null);

  const cycle = useCallback((backward: boolean): void => {
    const { shown, focusTerminal } = latest.current;
    if (document.querySelector(MODAL_OPEN)) {
      if (document.querySelector('[role="menu"]') || !shown.banner) return;
      setInteractionModality("keyboard");
      if (currentRegion() === "banner") focusOpenDialog(dialogFocus.current);
      else {
        dialogFocus.current = document.activeElement;
        focusRegion("banner", focusTerminal);
      }
      return;
    }
    const from = currentRegion();
    // With focus in no region (on `<body>`, say), forward starts at the first and backward at the last.
    const start = from ? REGIONS.indexOf(from) : backward ? 0 : REGIONS.length - 1;
    const step = backward ? REGIONS.length - 1 : 1;
    for (let i = 1; i <= REGIONS.length; i++) {
      const region = REGIONS[(start + step * i) % REGIONS.length];
      if (region !== "toast" && !shown[region]) continue;
      // F6 is a keyboard action, but react-aria may still think the last input was a pointer
      // (a click inside the terminal, say) and then draws no focus ring on what lands.
      setInteractionModality("keyboard");
      if (focusRegion(region, focusTerminal)) break;
    }
  }, [latest]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "F6" || event.isComposing || event.defaultPrevented) return;
      // react-aria's landmark navigation acts on F6 whatever the modifiers, so under an open dialog
      // or menu a modified one is swallowed too.
      if (document.querySelector(MODAL_OPEN)) {
        if (!event.ctrlKey && !event.altKey && !event.metaKey) cycle(event.shiftKey);
        event.preventDefault();
        event.stopPropagation();
        return;
      }
      if (event.ctrlKey || event.altKey || event.metaKey) return;
      cycle(event.shiftKey);
      event.preventDefault();
      event.stopPropagation();
    };
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [cycle]);

  return { cycle };
}
