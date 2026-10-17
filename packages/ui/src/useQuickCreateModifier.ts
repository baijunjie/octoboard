import { useSyncExternalStore } from "react";

import { isPrimaryModifier } from "./useWindowShortcut";

const listeners = new Set<() => void>();
let held = false;

function setHeld(next: boolean): void {
  if (next === held) return;
  held = next;
  listeners.forEach((notify) => notify());
}

const onKey = (event: KeyboardEvent) => setHeld(isPrimaryModifier(event));
const onPointerMove = (event: PointerEvent) => setHeld(isPrimaryModifier(event));
const release = () => setHeld(false);
const onVisibilityChange = () => {
  if (document.visibilityState === "hidden") release();
};

let installed = false;

function install(): void {
  if (installed) return;
  installed = true;
  // The capture phase, so a key the terminal or a dialog swallows still counts. The keyup of a
  // modifier is not delivered once the window has lost focus, hence `blur` and `visibilitychange`
  // letting go; the pointer's own modifiers catch up a key pressed while the window was away.
  window.addEventListener("keydown", onKey, { capture: true });
  window.addEventListener("keyup", onKey, { capture: true });
  window.addEventListener("pointermove", onPointerMove, { capture: true, passive: true });
  window.addEventListener("blur", release);
  document.addEventListener("visibilitychange", onVisibilityChange);
}

function subscribe(listener: () => void): () => void {
  install();
  listeners.add(listener);
  return () => void listeners.delete(listener);
}

/** Whether the platform's shortcut modifier (⌘ on an Apple keyboard, Ctrl elsewhere) is held down
 * right now, which turns every new-session "+" into its quick-create form. One set of window
 * listeners, installed on the first read and kept, serves every reader, so the hold survives a
 * view being swapped for another while the key stays down. */
export function useQuickCreateModifier(): boolean {
  return useSyncExternalStore(subscribe, () => held);
}
