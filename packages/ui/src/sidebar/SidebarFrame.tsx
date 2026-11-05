import React, { useEffect, useLayoutEffect, useRef } from "react";

import { onReducedMotion, prefersReducedMotion } from "../components/motion/reducedMotion";
import { ringForAutomaticFocus } from "../input/touchInput";
import { useT } from "../i18n/react";
import { COLUMN_EASE_CLASS, columnSpan, isDockEasing, usePaneDockMotion, type DockPhase } from "../layout/paneDockMotion";
import { drawerClass, PANE_ID, RailClip, usePanePeekPointer } from "../layout/paneOverlay";
import type { PaneWidth } from "../layout/paneWidth";
import { PaneResizeHandle } from "../layout/PaneResizeHandle";
import type { PanePeek } from "../layout/usePaneToggles";
import { focusTargetId } from "./focus";
import type { FocusTarget } from "./types";

/** The clip the docked column sits in. `min-w-0` lets the flex item reach 0 while the sidebar
 * inside keeps its full width, so the list does not reflow.
 *
 * While `easing`, the clip's own `width` transitions. A registered custom property must not drive
 * it: WebKit snaps a running transition of one to its end whenever a stylesheet is rewritten, as
 * xterm does on a refit. */
function columnClipClass(easing: boolean): string {
  return (
    "contents sidebar-docked:block sidebar-docked:h-full sidebar-docked:min-h-0 sidebar-docked:min-w-0 sidebar-docked:shrink-0 sidebar-docked:overflow-hidden" +
    (easing ? ` ${COLUMN_EASE_CLASS}` : "")
  );
}

interface SidebarFrameProps {
  /** The id of the console the sidebar shows, which a switch of consoles fades the view in for. */
  shownConsoleId: string;
  /** The project or console session in focus mode, if any. */
  focus?: FocusTarget;
  /** Whether the drawer is open below the sidebar's docking threshold; above it the docked sidebar is
   * shown or, with `peek`, hidden (`usePaneToggles` owns the state, resetting it once the window no
   * longer needs it). */
  open: boolean;
  /** The drawer is closed below the sidebar's docking threshold, its exit included: out of reach. */
  drawerAway?: boolean;
  /** The hover reveal of the sidebar while the user has hidden it from the top bar. The card
   * (fixed, inset from the rail and the other chrome, rounded, edged) is only up
   * while this hover is, or while that card is sliding away. The rest of the time a hidden docked
   * sidebar is the same column as the shown one, clipped to nothing. `undefined` while the docked
   * sidebar is shown. */
  peek?: Pick<PanePeek, "active" | "keep" | "leave">;
  /** Hears the docked column's show/hide phase, including one settled before paint, so a sibling
   * such as the resize handle can follow it. */
  onColumnPhase?: (phase: DockPhase) => void;
  /** A press began inside the sidebar while it is hidden-docked (floating or sliding away), before
   * whatever was pressed acts: where the console it is showing, if that is not the current one,
   * becomes the current one. */
  onPeekPress?: () => void;
  /** The docked-mode width and its setters; `App` owns it (`app/usePaneWidths.ts`) so it can also
   * be read from outside the sidebar. */
  sidebarWidth: PaneWidth;
  /** What the sidebar shows: the console view, or a focus mode. */
  children: React.ReactNode;
}

/** The sidebar's frame: the docked column and its clip, the drawer and the hover card, the handoff of
 * keyboard focus when the view changes, and the animation of the view arriving. It knows nothing of
 * what the view holds. */
export function SidebarFrame({
  shownConsoleId,
  focus,
  open,
  drawerAway = false,
  peek,
  onPeekPress,
  onColumnPhase,
  sidebarWidth,
  children,
}: SidebarFrameProps): React.ReactElement {
  const t = useT();
  // Entering or leaving focus mode swaps the sidebar's content, which unmounts the control that
  // started it and drops a keyboard user's focus to `<body>` (no blur event fires). `holdsFocus`
  // remembers that focus was in the sidebar (React's focus events also bubble up from its menus'
  // portals), so focus can go to the new view's first control; a mouse press leaves it on the
  // terminal, which this does not touch.
  const navRef = useRef<HTMLElement>(null);
  const holdsFocus = useRef(false);
  const focusId = focus && focusTargetId(focus);
  const shownFocusId = useRef(focusId);
  const shownConsoleSessionFocus = useRef(focus !== undefined && "consoleSession" in focus);
  useEffect(() => {
    const previousId = shownFocusId.current;
    const fromConsoleSession = shownConsoleSessionFocus.current;
    shownFocusId.current = focusId;
    shownConsoleSessionFocus.current = focus !== undefined && "consoleSession" in focus;
    if (previousId === focusId) return;
    // After a short delay: a menu item that switched the view has its menu hand focus back (to the
    // trigger that just unmounted) a task later, which would otherwise land after this.
    const timer = setTimeout(() => {
      if (!holdsFocus.current || document.activeElement !== document.body) return;
      // Only where nothing else took focus: a switch that selects a session normally hands it to that
      // session's terminal. Between two console sessions' focus modes, the strip's chip that was
      // pressed has been replaced by the new current one, the nearest place to the keyboard user.
      // From a project's focus mode or from none, the first control is the way out.
      const target = focus
        ? (fromConsoleSession ? navRef.current?.querySelector<HTMLElement>("[data-switch-current]") : null) ??
          navRef.current?.querySelector<HTMLElement>("[data-focus-exit]")
        : navRef.current?.querySelector<HTMLElement>(`[data-list-motion="${CSS.escape(previousId ?? "")}"] [role=button]`);
      if (!target) return;
      ringForAutomaticFocus();
      target.focus();
    }, 50);
    return () => clearTimeout(timer);
  }, [focusId]);

  // The console avatars on the rail also keep the floating card up, being outside the gap around it.
  usePanePeekPointer("sidebar", peek, "[data-sidebar-peek-keep]");
  // `peek` being passed means the docked sidebar is targeted hidden. The column eases to and from
  // that; the inset card is a later state, only once the column is out of the row.
  const dock = usePaneDockMotion(peek === undefined, peek?.active ?? false, onColumnPhase);
  const drawerClassName = dock.inset
    ? `sidebar-docked:rounded-xl ${drawerClass("floating", open, dock.insetShown)}${dock.preparingInset ? " sidebar-docked:transition-none" : ""}`
    : `${drawerClass("drawer", open)} sidebar-docked:h-full`;
  const handleOverflow = dock.inset ? "tablet:overflow-visible!" : "tablet:sidebar-floating:overflow-visible!";

  // What the sidebar is showing, and how it arrived there: going down into a focus mode slides the
  // new view in from the end, coming back up slides it in from the start, and switching to another
  // console fades it in.
  const viewKey = focusId ? `focus:${focusId}` : `console:${shownConsoleId}`;
  const viewRef = useRef<HTMLDivElement>(null);
  const previousViewRef = useRef(viewKey);
  const viewAnimationRef = useRef<Animation>(undefined);
  // Played in place rather than by remounting the view, so a console switch leaves what is mounted
  // in it (the header and its menu trigger among it) as it is.
  useLayoutEffect(() => {
    const previous = previousViewRef.current;
    previousViewRef.current = viewKey;
    if (previous === viewKey || prefersReducedMotion()) return;
    // The card owns its entry animation; animate content changes only once it is visible.
    if (peek?.active && !dock.insetShown) return;
    // Going into or out of a focus mode slides sideways; between two views of the same level (two
    // focus modes, or two consoles) there is no level to go down or up, so the view rises in place.
    const enters = viewKey.startsWith("focus:");
    const leaves = previous.startsWith("focus:");
    const from =
      enters === leaves
        ? { transform: "translateY(6px)" }
        : enters
          ? { transform: "translateX(var(--view-shift))" }
          : { transform: "translateX(calc(-1 * var(--view-shift)))" };
    viewAnimationRef.current = viewRef.current?.animate([{ ...from, opacity: 0 }, {}], { duration: 240, easing: "cubic-bezier(0.2, 0, 0, 1)" });
  }, [viewKey]);
  // Reduced motion requested while the view is arriving: it is where it ends up at once.
  useEffect(() => onReducedMotion(() => viewAnimationRef.current?.cancel()), []);

  return (
    <RailClip floating={dock.inset}>
      {/* The clip is `display: contents` below the breakpoint and while the hover card is up, so
          the fixed pane is laid out as if this wrapper were not there. At the docked breakpoint it
          is the column's width. The sidebar inside stays at its full width, against the rail, and
          the clip uncovers it: the list does not reflow and the column does not become the card.
          The clip's width eases between the sidebar's and 0; the top bar's start segment eases
          to the same target (`App`). */}
      <div
        className={dock.inset ? "contents" : columnClipClass(isDockEasing(dock.phase))}
        style={dock.inset ? undefined : { width: columnSpan(dock.phase, sidebarWidth.width) }}
        onTransitionEnd={dock.onColumnTransitionEnd}
      >
        <nav
          // Below the sidebar's docking threshold this is a fixed overlay, closed by default, faded up into
          // place and back down with `open`, and out of reach (inert) once closed, its exit
          // included; at or above it the `sidebar-docked:` variants in `drawerClass` put it back exactly
          // where it always was, a plain row sibling, regardless of `open`. With the docked sidebar
          // hidden, that column is clipped away; only a hover on a console's avatar (`dock.inset`)
          // makes it the fixed card. Drawers share that card's inset from the chrome. At tablet width and
          // below the card also holds the rail, which lies over its padded start.
          // Starting below `--top-chrome-height` leaves the top bar, and
          // with it the sidebar toggle, visible while the drawer is open; ending above
          // `--bottom-chrome-height` does the same for the connection banner and the update notice.
          //
          // `data-escape-scope`: one of the origins `usePaneToggles`'s capture-phase Escape listener
          // closes a drawer for. `data-pane` is how it finds this element to see whether it holds
          // focus.
          data-escape-scope
          inert={drawerAway}
          ref={navRef}
          onFocus={(e) => {
            // A portalled menu's focus bubbles here through React and may never blur, so it counts only
            // when it came from inside the sidebar (a menu opened from the keyboard), not from the
            // terminal (one opened with the mouse).
            if (e.currentTarget.contains(e.target as Node) || e.currentTarget.contains(e.relatedTarget as Node | null)) {
              holdsFocus.current = true;
            }
          }}
          onBlur={(e) => {
            // Focus moving around inside a portalled menu blurs here too; only leaving the sidebar's own
            // DOM counts.
            if (e.currentTarget.contains(e.target as Node) && !e.currentTarget.contains(e.relatedTarget)) {
              holdsFocus.current = false;
            }
          }}
          id={PANE_ID.sidebar}
          data-pane="sidebar"
          data-region="sidebar"
          // The width cap keeps a strip at the end to tap to dismiss. At tablet width and below the card carries the
          // rail inside its width, so the cap holds the rail and the sidebar together.
          className={`flex max-tablet:w-[calc(var(--spacing)*70+var(--rail-width))]! max-tablet:ps-(--rail-width) sidebar-floating:max-w-[calc(100vw-var(--rail-width)-2*var(--peek-gap))] flex-col overflow-hidden sidebar-fills bg-panel-sidebar shrink-0 ${handleOverflow} ${drawerClassName}${dock.phase === "closed" && !dock.inset ? " sidebar-docked:invisible" : ""}`}
          onPointerEnter={peek?.keep}
          onPointerLeave={peek?.leave}
          onPointerDownCapture={peek ? onPeekPress : undefined}
          onTransitionEnd={dock.inset ? dock.onInsetTransitionEnd : undefined}
          style={{ width: sidebarWidth.width }}
          aria-label={t("sidebar.sessions")}
        >
          <div key={focusId ?? "console"} ref={viewRef} className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-[inherit]">
            {children}
          </div>
          <div className={dock.inset ? "hidden tablet:contents" : "hidden tablet:contents sidebar-docked:hidden"}>
            <PaneResizeHandle side="sidebar" paneWidth={sidebarWidth} floating />
          </div>
        </nav>
      </div>
    </RailClip>
  );
}
