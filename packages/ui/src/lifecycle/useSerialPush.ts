import { useCallback, useRef } from "react";

/**
 * Queues best-effort pushes to the native shell so that they reach it one at a time, in the order they were made: a
 * push starts when the one before it has settled, so two in flight cannot settle out of order and leave the shell on
 * the older value. A push that fails, whether by rejecting or by throwing as `send` is called, is dropped without
 * surfacing and does not hold up the ones after it.
 */
export function useSerialPush(): (send: () => Promise<unknown>) => void {
  const pendingRef = useRef<Promise<unknown>>(Promise.resolve());
  return useCallback((send) => {
    pendingRef.current = pendingRef.current.then(send).then(undefined, () => {});
  }, []);
}
