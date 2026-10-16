import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { useLatest } from "../components/useLatest";
import { prefersReducedMotion } from "../reducedMotion";

/** Matches the column's CSS width ease and the drawer's `duration-200`. */
export const PANE_SPAN_MS = 200;

/** A little past the ease, so a transition that never reports its end (a backgrounded tab, a
 * browser that skips the event) cannot leave the column mid-gesture. */
const SPAN_END_SLACK_MS = 50;

/** Matches the floating card's fade and its pointer-settle guard. */
export const PANE_INSET_MS = 240;

/** Where the docked pane is in its show/hide ease. `open` and `closed` are settled; the other
 * two are the ease itself. Below the docked breakpoint none of this is on screen: the drawer owns
 * that layout, and every class these phases add is `docked:`. */
export type DockPhase = "open" | "closing" | "closed" | "opening";

export interface DockMotion {
  phase: DockPhase;
  /** The hover card's geometry, including while that card fades away. False while the pane is
   * the docked column, hidden or not. */
  inset: boolean;
  /** The hover card is visible. False while it prepares to enter or fades away. */
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
  // Already hovering as the view mounts: there is no preceding hidden frame to animate from.
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

/** The docked pane's show/hide and its hover card. The column eases between full width and
 * none before the card is allowed on screen, and the card never stands in for that ease: a hide
 * that turned into the card for the exit would change the pane's shape on the first frame. */
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
      ...(reduce && peekActive ? { inset: true, insetShown: true } : {}),
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

  // Nothing is on screen yet, so there is no exit to animate.
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

/** The hover card takes no space in the row. */
export function columnSpan(phase: DockPhase, width: number): number {
  return phase === "open" || phase === "opening" ? width : 0;
}

export interface PaneDockMotion extends DockMotion {
  /** Hide the newly fixed card without animating from its preceding column geometry. */
  preparingInset: boolean;
  onColumnTransitionEnd: (event: React.TransitionEvent<HTMLElement>) => void;
  onColumnEnd: () => void;
  onInsetTransitionEnd: (event: React.TransitionEvent<HTMLElement>) => void;
}

/** Coordinates the column ease and floating card without unmounting the pane contents. */
export function usePaneDockMotion(
  shown: boolean,
  peekActive: boolean,
  onPhase?: (phase: DockPhase) => void,
): PaneDockMotion {
  const [state, setState] = useState(() => initialDockMotion(shown, peekActive));
  const shownRef = useRef(shown);
  const peekRef = useRef(peekActive);
  const onPhaseRef = useLatest(onPhase);

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

  useEffect(() => {
    if (state.pending === null) return;
    // React can flush effects before paint after a layout-effect update. Two frames leave the
    // hidden card (or zero-width column) painted before its transition target is applied.
    let frame = requestAnimationFrame(() => {
      frame = requestAnimationFrame(() => {
        setState((current) => (current.pending === null ? current : reduceDockMotion(current, { type: "painted" })));
      });
    });
    return () => cancelAnimationFrame(frame);
  }, [state.pending]);

  useEffect(() => {
    if (state.phase !== "opening" && state.phase !== "closing") return;
    const reduce = prefersReducedMotion();
    const timer = setTimeout(
      () => setState((current) => reduceDockMotion(current, { type: "column-end", reduce })),
      reduce ? 0 : PANE_SPAN_MS + SPAN_END_SLACK_MS,
    );
    return () => clearTimeout(timer);
  }, [state.phase]);

  useEffect(() => {
    if (!state.inset || state.insetShown || state.peekActive) return;
    const timer = setTimeout(
      () => setState((current) => reduceDockMotion(current, { type: "inset-end" })),
      PANE_INSET_MS + SPAN_END_SLACK_MS,
    );
    return () => clearTimeout(timer);
  }, [state.inset, state.insetShown, state.peekActive]);

  return {
    phase: state.phase,
    preparingInset: state.pending === "show-inset",
    inset: state.inset,
    insetShown: state.insetShown,
    onColumnEnd: () => setState((current) => reduceDockMotion(current, { type: "column-end", reduce: prefersReducedMotion() })),
    onColumnTransitionEnd: (event) => {
      if (event.target !== event.currentTarget || event.propertyName !== "width") return;
      setState((current) => reduceDockMotion(current, { type: "column-end", reduce: prefersReducedMotion() }));
    },
    onInsetTransitionEnd: (event) => {
      if (event.target !== event.currentTarget || event.propertyName !== "opacity") return;
      setState((current) => reduceDockMotion(current, { type: "inset-end" }));
    },
  };
}
