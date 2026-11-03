import { useCallback, useLayoutEffect, useRef, useState } from "react";

/**
 * The value of the latest committed render, for code that runs later to read when it runs: a
 * listener or a subscription made once, a timer, an effect, a callback handed to something built
 * once.
 *
 * Kept up to date in a layout effect rather than assigned during render: the React Compiler leaves
 * a component or hook that writes a ref during render uncompiled, and a render React throws away
 * would leak its value. Events, timers, passive effects and the parent's layout effects all run
 * after it and see the committed value. Only layout effects that run earlier in the same commit — a
 * descendant's, or one declared before this call — still see the previous one.
 */
export function useLatest<T>(value: T): { readonly current: T } {
  const ref = useRef(value);
  useLayoutEffect(() => {
    ref.current = value;
  });
  return ref;
}

/**
 * A function that keeps its identity for the component's lifetime and calls the latest committed
 * render's `handler`. For a handler passed down to a list whose rows should not all render again
 * whenever the handler's closure changes, which the React Compiler cannot prevent where the closure
 * captures something that changes with every daemon event (a store map, say).
 *
 * Only for what runs after the commit — an event, a menu action, a timer. Called during render, or
 * from a descendant's layout effect in the same commit, it calls the previous render's `handler`
 * (see `useLatest`).
 */
export function useStableHandler<A extends unknown[], R>(handler: (...args: A) => R): (...args: A) => R {
  const latest = useLatest(handler);
  return useCallback((...args: A) => latest.current(...args), [latest]);
}

/** A cell `useRenderLatest` keeps the latest render's value in. A method sets it, so that the write is no ref
 * access for the React Compiler to refuse; setting the value a render already holds changes nothing. */
class RenderCell<T> {
  value: T;
  constructor(value: T) {
    this.value = value;
  }
  set(value: T): void {
    this.value = value;
  }
  get = (): T => this.value;
}

/**
 * A function, stable for the component's lifetime, that returns the value of the latest render, set during
 * render: for a callback that children read while they render or from their own effects in the same commit,
 * and so must already see this render's value, which `useLatest` cannot give. A render React throws away
 * leaks its value here: use `useLatest` wherever a reader runs only after the commit. The function's identity never
 * changes, so it does not make a memoized reader render again: a child that must show a changed value needs that
 * value passed to it some other way.
 */
export function useRenderLatest<T>(value: T): () => T {
  const [cell] = useState(() => new RenderCell(value));
  cell.set(value);
  return cell.get;
}
