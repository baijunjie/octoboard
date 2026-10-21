import { useSyncExternalStore } from "react";

/** How often the clock advances. The relative times it feeds change at minute granularity and read
 * "now" for the first minute, so a 30 s tick leaves no text more than 30 s behind the clock while
 * waking the page only twice a minute. */
const TICK_MS = 30_000;

const listeners = new Set<() => void>();
let ticker: ReturnType<typeof setInterval> | undefined;
// Null while nobody listens, so the next reader starts from the clock rather than a stale reading.
let now: number | null = null;

function subscribe(listener: () => void): () => void {
  if (listeners.size === 0) {
    // The reading the first reader rendered on is kept, so it is not rendered a second time for the
    // milliseconds that passed since; one left by a render that never committed is replaced once
    // it is a tick old (React compares the snapshot again right after subscribing).
    if (now === null || Date.now() - now >= TICK_MS) now = Date.now();
    ticker = setInterval(() => {
      now = Date.now();
      listeners.forEach((notify) => notify());
    }, TICK_MS);
  }
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0) {
      clearInterval(ticker);
      ticker = undefined;
      now = null;
    }
  };
}

function getSnapshot(): number {
  now ??= Date.now();
  return now;
}

/** The current time in milliseconds, re-read every 30 s while a component uses it. Passing it to
 * `formatRelativeTime` as `now` keeps that text moving: a call that reads `Date.now()` itself is
 * cached by the React Compiler on its other inputs and would never advance. One interval serves
 * every user and runs only while there is one, so a component mounted for the window's whole life
 * keeps it running for good; such a component arms its own timer for the next moment it needs. */
export function useNow(): number {
  return useSyncExternalStore(subscribe, getSnapshot);
}
