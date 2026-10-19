import { setInteractionModality } from "react-aria";
import { useCallback, useEffect, useRef } from "react";

import { focusOpenDialog, focusRegion, regionElements, type Region } from "../components/regionFocus";
import { useLatest } from "../components/useLatest";

/** The window's regions in visual order, each an element (or several) marked `data-region`. The
 * panels of the right pane, each a stop of its own, come after the terminal: the caller names
 * them in `panels`, in order. The toast stack comes last and is a
 * stop only while a toast is on screen; unlike the others it is not for the caller to say: the stack
 * is in the DOM while it holds a toast, and one that is closing (kept mounted for its exit animation,
 * `data-exiting`) is not a landing place. */
const BEFORE_PANELS = ["topbar", "rail", "welcome", "sidebar", "archive", "terminal"] as const;
const AFTER_PANELS = ["banner", "toast"] as const;
type FixedRegion = (typeof BEFORE_PANELS)[number] | (typeof AFTER_PANELS)[number];
type ShownRegion = Exclude<FixedRegion, "toast">;

function windowRegions(panels: readonly string[]): readonly Region[] {
  return [...BEFORE_PANELS, ...panels, ...AFTER_PANELS];
}

/** A dialog or a menu is open: ordinary regions stay unavailable. A toast is an `alertdialog`
 * too, but a non-modal one. */
export const MODAL_OPEN =
  '[role="dialog"]:not([aria-modal="false"]), [role="alertdialog"]:not([aria-modal="false"]), [role="menu"]';

function currentRegion(regions: readonly Region[]): Region | undefined {
  return regions.find((region) => regionElements(region).some((el) => el.contains(document.activeElement)));
}

/**
 * F6 and Shift+F6 move keyboard focus to the next and previous region of the window, in a cycle:
 * the terminal takes Tab and Shift+Tab for the agent, so without this a keyboard user could never
 * leave it (WCAG 2.2 SC 2.1.2). `shown` says which regions are on screen right now; the others,
 * including a floating pane that is merely able to slide in, are skipped, as is a region with
 * nothing to focus. The connection banner is a stop, after the panels, while its strip is shown. The
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
  panels = {},
  focusTerminal,
}: {
  shown: Record<ShownRegion, boolean>;
  /** Whether each panel region is on screen, keyed by its `data-region`, in window order. */
  panels?: Readonly<Record<string, boolean>>;
  focusTerminal: () => void;
}): { cycle: (backward: boolean) => void } {
  const latest = useLatest({ shown, panels, focusTerminal });
  const dialogFocus = useRef<Element | null>(null);

  const cycle = useCallback((backward: boolean): void => {
    const { shown, panels, focusTerminal } = latest.current;
    const regions = windowRegions(Object.keys(panels));
    if (document.querySelector(MODAL_OPEN)) {
      if (document.querySelector('[role="menu"]') || !shown.banner) return;
      setInteractionModality("keyboard");
      if (currentRegion(regions) === "banner") focusOpenDialog(dialogFocus.current);
      else {
        dialogFocus.current = document.activeElement;
        focusRegion("banner", focusTerminal);
      }
      return;
    }
    const from = currentRegion(regions);
    // With focus in no region (on `<body>`, say), forward starts at the first and backward at the last.
    const start = from ? regions.indexOf(from) : backward ? 0 : regions.length - 1;
    const step = backward ? regions.length - 1 : 1;
    for (let i = 1; i <= regions.length; i++) {
      const region = regions[(start + step * i) % regions.length];
      if (region !== "toast" && !(region in panels ? panels[region] : shown[region as ShownRegion])) continue;
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
