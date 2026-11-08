import { useCallback, useRef } from "react";

import { createSerialQueue } from "./serialQueue";

/** A push queue (`createSerialQueue`) for a component, whose pushes are not waited on. */
export function useSerialPush(): (send: () => Promise<unknown>) => void {
  const queueRef = useRef<ReturnType<typeof createSerialQueue> | null>(null);
  return useCallback((send) => {
    queueRef.current ??= createSerialQueue();
    void queueRef.current(send);
  }, []);
}
