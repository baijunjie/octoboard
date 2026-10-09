import { useEffect, useLayoutEffect, useRef, useState } from "react";

/** How long the docked sidebar eases open or closed, and how long the hover card slides.
 * The same length as the drawer's `duration-200`, and as `.sidebar-span-animate` in `style.css`. */
export const SIDEBAR_SPAN_MS = 200;

/** A little past the ease, so a transition that never reports its end (a backgrounded tab, a
 * browser that skips the event) cannot leave the column mid-gesture. */
const SPAN_END_SLACK_MS = 50;

/** Where the docked sidebar is in its show/hide ease. `open` and `closed` are settled; the other
 * two are the ease itself. Below the docked breakpoint none of this is on screen: the drawer owns
 * that layout, and every class these phases add is `docked:`. */
export type DockPhase = "open" | "closing" | "closed" | "opening";

export interface DockMotion {
  phase: DockPhase;
  /** The hover card's geometry, including while that card slides away. False while the sidebar is
   * the docked column, hidden or not. */
  inset: boolean;
  /** The hover card is translated on screen. False while it waits off to the side or slides away. */
  insetShown: boolean;
}

interface Machine extends DockMotion {
  shown: boolean;
  peekActive: boolean;
  /** A step that has to wait until the browser has painted the frame it starts from. */
  pending: "open" | "show-inset" | null;
}

export function initialDockMotion(shown: boolean, peekActive: boolean): Machine {
  if (shown) {
    return { shown: true, peekActive: false, phase: "open", inset: false, insetShown: false, pending: null };
  }
  // Already hovering as the view mounts: there is no column frame to slide the card in from.
  if (peekActive) {
    return { shown: false, peekActive: true, phase: "closed", inset: true, insetShown: true, pending: null };
  }
  return { shown: false, peekActive: false, phase: "closed", inset: false, insetShown: false, pending: null };
}

export type DockEvent =
  | { type: "target"; shown: boolean; peekActive: boolean; reduce: boolean }
  | { type: "column-end"; reduce: boolean }
  | { type: "inset-end" }
  | { type: "painted" };

/** The docked sidebar's show/hide and its hover card. The column eases between full width and
 * none before the card is allowed on screen, and the card never stands in for that ease: a hide
 * that turned into the card for the slide would change the sidebar's shape on the first frame. */
export function reduceDockMotion(state: Machine, event: DockEvent): Machine {
  switch (event.type) {
    case "target":
      return applyTarget(state, event);
    case "column-end":
      return applyColumnEnd(state, event.reduce);
    case "inset-end":
      return state.inset && !state.insetShown && !state.peekActive ? { ...state, inset: false } : state;
    case "painted":
      return applyPainted(state);
  }
}

function applyTarget(state: Machine, event: { shown: boolean; peekActive: boolean; reduce: boolean }): Machine {
  const { shown, peekActive, reduce } = event;

  if (shown && !state.shown) {
    // The card is `position: fixed`, so the clip has no width to ease from until it has been
    // painted back into the row at 0. A column that is already in the row eases at once.
    const fromInset = state.inset && !reduce;
    return {
      shown: true,
      peekActive: false,
      inset: false,
      insetShown: false,
      pending: fromInset ? "open" : null,
      phase: reduce ? "open" : fromInset ? "closed" : "opening",
    };
  }

  if (!shown && state.shown) {
    return {
      shown: false,
      peekActive,
      inset: false,
      insetShown: false,
      pending: null,
      phase: reduce || state.phase === "closed" ? "closed" : "closing",
    };
  }

  // A peek during the close waits: the card starts only once the column has left the row.
  if (!shown && peekActive && state.phase === "closed" && !state.inset) {
    return {
      ...state,
      shown: false,
      peekActive: true,
      inset: true,
      insetShown: reduce,
      pending: reduce ? null : "show-inset",
    };
  }

  if (!shown && peekActive && state.inset) {
    if (state.insetShown) return { ...state, peekActive: true, pending: null };
    return { ...state, peekActive: true, insetShown: reduce, pending: reduce ? null : "show-inset" };
  }

  // Nothing is on screen yet, so there is no slide to play on the way out.
  if (!shown && !peekActive && state.inset) {
    if (reduce || !state.insetShown) {
      return { ...state, peekActive: false, inset: false, insetShown: false, pending: null };
    }
    return { ...state, peekActive: false, insetShown: false, pending: null };
  }

  return { ...state, shown, peekActive };
}

function applyColumnEnd(state: Machine, reduce: boolean): Machine {
  if (state.phase === "opening" && state.shown) return { ...state, phase: "open" };
  if (state.phase !== "closing" || state.shown) return state;
  const closed: Machine = { ...state, phase: "closed" };
  if (!state.peekActive) return closed;
  return { ...closed, inset: true, insetShown: reduce, pending: reduce ? null : "show-inset" };
}

function applyPainted(state: Machine): Machine {
  if (state.pending === "open" && state.shown) {
    return { ...state, pending: null, phase: "opening", inset: false, insetShown: false };
  }
  if (state.pending === "show-inset" && state.peekActive && state.inset && !state.shown) {
    return { ...state, pending: null, insetShown: true };
  }
  return state.pending === null ? state : { ...state, pending: null };
}

function prefersReducedMotion(): boolean {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** The width the docked column reserves in the row. The hover card reserves none. */
export function columnSpan(phase: DockPhase, width: number): number {
  return phase === "open" || phase === "opening" ? width : 0;
}

/** The clip the docked column sits in. `min-w-0` lets the flex item reach 0 while the sidebar
 * inside keeps its full width, so the list does not reflow. The inset shadow is the separator the
 * nav's own end border draws once the column is open: a real border would keep a 1px line when the
 * clip's width is 0. `--sidebar-span` itself is eased on the document element (see the hook). */
export function columnClipClass(): string {
  return (
    "contents docked:block docked:h-full docked:min-h-0 docked:min-w-0 docked:shrink-0 docked:overflow-hidden " +
    "docked:shadow-[inset_-1px_0_0_0_var(--separator)] docked:rtl:shadow-[inset_1px_0_0_0_var(--separator)]"
  );
}

export interface SidebarDockMotion extends DockMotion {
  onInsetTransitionEnd: (event: React.TransitionEvent<HTMLElement>) => void;
}

/** `shown` is the docked sidebar's target (hidden passes the hover `peek`). `peekActive` is that
 * hover being up. `width` is the column's settled width, published as `--sidebar-span` on the
 * document element so the clip and the top bar read one eased length. `onPhase` hears every phase,
 * including the one settled before paint. */
export function useSidebarDockMotion(
  shown: boolean,
  peekActive: boolean,
  width: number,
  onPhase?: (phase: DockPhase) => void,
): SidebarDockMotion {
  const [state, setState] = useState(() => initialDockMotion(shown, peekActive));
  const shownRef = useRef(shown);
  const peekRef = useRef(peekActive);
  const onPhaseRef = useRef(onPhase);
  onPhaseRef.current = onPhase;

  useLayoutEffect(() => {
    if (shownRef.current === shown && peekRef.current === peekActive) return;
    shownRef.current = shown;
    peekRef.current = peekActive;
    const reduce = prefersReducedMotion();
    setState((current) => reduceDockMotion(current, { type: "target", shown, peekActive, reduce }));
  }, [shown, peekActive]);

  useLayoutEffect(() => {
    onPhaseRef.current?.(state.phase);
  }, [state.phase]);

  // One length for the clip and the top bar. The transition lives on this element (`style.css`);
  // descendants inherit the animated value. Set before paint so the first frame is not a jump.
  useLayoutEffect(() => {
    const root = document.documentElement;
    root.style.setProperty("--sidebar-span", `${columnSpan(state.phase, width)}px`);
    root.classList.toggle("sidebar-span-animate", state.phase === "opening" || state.phase === "closing");
  }, [state.phase, width]);

  useEffect(() => {
    const root = document.documentElement;
    const onEnd = (event: TransitionEvent) => {
      if (event.target !== root || event.propertyName !== "--sidebar-span") return;
      setState((current) => reduceDockMotion(current, { type: "column-end", reduce: prefersReducedMotion() }));
    };
    root.addEventListener("transitionend", onEnd);
    return () => {
      root.removeEventListener("transitionend", onEnd);
      root.style.removeProperty("--sidebar-span");
      root.classList.remove("sidebar-span-animate");
    };
  }, []);

  useEffect(() => {
    if (state.pending === null) return;
    setState((current) => (current.pending === null ? current : reduceDockMotion(current, { type: "painted" })));
  }, [state.pending]);

  useEffect(() => {
    if (state.phase !== "opening" && state.phase !== "closing") return;
    const reduce = prefersReducedMotion();
    const timer = setTimeout(
      () => setState((current) => reduceDockMotion(current, { type: "column-end", reduce })),
      reduce ? 0 : SIDEBAR_SPAN_MS + SPAN_END_SLACK_MS,
    );
    return () => clearTimeout(timer);
  }, [state.phase]);

  useEffect(() => {
    if (!state.inset || state.insetShown || state.peekActive) return;
    const timer = setTimeout(
      () => setState((current) => reduceDockMotion(current, { type: "inset-end" })),
      SIDEBAR_SPAN_MS + SPAN_END_SLACK_MS,
    );
    return () => clearTimeout(timer);
  }, [state.inset, state.insetShown, state.peekActive]);

  return {
    phase: state.phase,
    inset: state.inset,
    insetShown: state.insetShown,
    onInsetTransitionEnd: (event) => {
      if (event.target !== event.currentTarget || event.propertyName !== "translate") return;
      setState((current) => reduceDockMotion(current, { type: "inset-end" }));
    },
  };
}
