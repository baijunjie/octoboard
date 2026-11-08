import { act } from "react";

import type { WindowFocusCapability } from "../../platform";

/** A shell's window focus (`WindowFocusCapability`) the test moves itself with the returned `focus`, which changes
 * nothing the document reports and dispatches no event on the window, as the macOS webview does when the app is
 * deactivated or reactivated. `answer` is the shell's first answer, which arrives on the first subscription;
 * `undefined` leaves it out. */
export function shellWindowFocus(answer: boolean | undefined = true): {
  capability: WindowFocusCapability;
  focus: (focused: boolean) => void;
  subscribers: () => number;
} {
  let focused: boolean | undefined;
  const callbacks = new Set<() => void>();
  const set = (next: boolean) => {
    focused = next;
    for (const notify of callbacks) notify();
  };
  const capability: WindowFocusCapability = {
    focused: () => focused,
    subscribe(callback) {
      callbacks.add(callback);
      if (callbacks.size === 1 && answer !== undefined) queueMicrotask(() => set(answer));
      return () => {
        callbacks.delete(callback);
        if (callbacks.size === 0) focused = undefined;
      };
    },
  };
  return { capability, focus: (next) => act(() => set(next)), subscribers: () => callbacks.size };
}
