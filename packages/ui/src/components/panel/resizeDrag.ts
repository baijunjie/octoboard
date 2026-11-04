import { useSyncExternalStore } from "react";

let activeDrags = 0;
const listeners = new Set<() => void>();
const snapshot = () => activeDrags > 0;
const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => listeners.delete(listener);
};
const notify = () => {
  for (const listener of listeners) listener();
};

/** Holds expensive content sizing until every captured separator drag has ended. */
export function startResizeDrag(): () => void {
  activeDrags++;
  if (activeDrags === 1) notify();
  let finished = false;
  return () => {
    if (finished) return;
    finished = true;
    activeDrags--;
    if (activeDrags === 0) notify();
  };
}

export function useResizeDrag(): boolean {
  return useSyncExternalStore(subscribe, snapshot);
}
