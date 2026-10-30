import { useEffect, useRef } from "react";

import { errorAlerts } from "../errorAlerts";

/** Returns a function that runs `then` once the error alert about to be reported (or the ones open now) has been
 * acknowledged and none is left, for what a dialog should do only after its failure's alert has closed: while the alert
 * is open the dialog is hidden behind it, so a change made then would be neither seen nor announced, and the focus the
 * alert gives back would land on a control that had changed under it. Nothing runs once the dialog has unmounted. */
export function useAfterErrorAlert(): (then: () => void) => void {
  const pending = useRef(new Set<() => void>());
  useEffect(() => {
    const waiting = pending.current;
    return () => {
      waiting.forEach((stop) => stop());
      waiting.clear();
    };
  }, []);
  return (then) => {
    let seen = errorAlerts.getSnapshot().length > 0;
    const stop = errorAlerts.subscribe(() => {
      if (errorAlerts.getSnapshot().length > 0) {
        seen = true;
        return;
      }
      if (!seen) return;
      stop();
      pending.current.delete(stop);
      then();
    });
    pending.current.add(stop);
  };
}
