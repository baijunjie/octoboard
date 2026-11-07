import React, { useLayoutEffect, type ReactNode } from "react";

/** Installed only by a capture host. It observes Suspense commits, never daemon requests or intentional busy states. */
export interface LazyReadiness {
  mount: () => () => void;
  fail: (key: object, error: unknown) => void;
  caught: (error: unknown) => void;
  commit: (key: object) => void;
  settle: (paint: () => Promise<void>, timeoutMs: number) => Promise<void>;
  dispose: () => void;
}

let observer: LazyReadiness | undefined;

/** The marker has no DOM; its cleanup runs when content commits or a suspended view is discarded. */
function PendingFallback({ observer, children }: { observer: LazyReadiness; children: ReactNode }): React.ReactElement {
  useLayoutEffect(() => observer.mount(), [observer]);
  return <>{children}</>;
}

/** Production keeps its original fallback and React tree. A capture host also observes its mounted lifetime. */
export function trackSuspenseFallback(fallback: ReactNode): ReactNode {
  return observer ? <PendingFallback observer={observer}>{fallback}</PendingFallback> : fallback;
}

export function failedLazyLoad(key: object, error: unknown): void {
  observer?.fail(key, error);
}

export function committedLazyLoad(key: object): void {
  observer?.commit(key);
}

export function observeLazyReadiness(): LazyReadiness {
  const previous = observer;
  const pending = new Set<object>();
  const rejected = new Map<object, unknown>();
  const failures = new Map<object, string>();
  const listeners = new Set<() => void>();
  let revision = 0;
  const changed = () => { revision++; for (const listener of listeners) listener(); };
  const tracking: LazyReadiness = {
    mount: () => {
      const key = {};
      pending.add(key);
      changed();
      return () => { pending.delete(key); changed(); };
    },
    fail: (key, error) => { rejected.set(key, error); },
    caught: (error) => {
      // React reports a caught rejection only if that lazy component is still part of the rendered tree.
      for (const [key, rejectedError] of rejected) {
        if (rejectedError !== error) continue;
        failures.set(key, error instanceof Error ? error.message : String(error));
        changed();
      }
    },
    commit: (key) => {
      // Resetting React's lazy cache may leave an error boundary showing its fallback; only rendered content recovers.
      const wasRejected = rejected.delete(key);
      const hadFailure = failures.delete(key);
      if (wasRejected || hadFailure) changed();
    },
    settle: async (paint, timeoutMs) => {
      const deadline = Date.now() + timeoutMs;
      const bounded = async (promise: Promise<void>) => {
        let timer: ReturnType<typeof setTimeout> | undefined;
        try {
          await Promise.race([
            promise,
            new Promise<never>((_, reject) => { timer = setTimeout(() => reject(new Error(`Lazy content did not settle within ${timeoutMs}ms (${pending.size} Suspense boundaries pending)`)), Math.max(0, deadline - Date.now())); }),
          ]);
        } finally { clearTimeout(timer); }
      };
      for (;;) {
        if (failures.size) throw new Error(`Lazy content failed to load: ${[...failures.values()].join("; ")}`);
        if (pending.size) {
          let notify!: () => void;
          const next = new Promise<void>((resolve) => (notify = resolve));
          listeners.add(notify);
          try { await bounded(next); } finally { listeners.delete(notify); }
          continue;
        }
        const beforePaint = revision;
        await bounded(paint());
        // A committed component can reveal another lazy child; a quiet import alone is not a committed view.
        if (beforePaint === revision && !pending.size && !failures.size) return;
      }
    },
    dispose: () => { if (observer === tracking) observer = previous; },
  };
  observer = tracking;
  return tracking;
}
