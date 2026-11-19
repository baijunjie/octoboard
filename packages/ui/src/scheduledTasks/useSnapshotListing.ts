import { useEffect, useState } from "react";

import { useConnectionSnapshot } from "../components/hooks/useConnectionSnapshot";
import { useStableHandler } from "../components/hooks/useLatest";
import { CodedError } from "../errors/codedError";
import { type ErrorPresentation, errorPresentation } from "../errors/errorPresentation";

/** A listing that could not be read, for the page to show where the list would be, with Try again (`retry`). `attempt`
 * counts the failures in a row, so that one repeating in the same words is announced again. */
export interface ListingFailure {
  message: ErrorPresentation;
  attempt: number;
}

/** Sends `send` (a `list_…` request whose reply lands in the store) once for each snapshot of the connection, while
 * `active` and for each `key` (what the listing is of); not before the connection's own snapshot, so a reconnect lists
 * once, on the new one. A failure other than those `quiet` names, which the next snapshot or `key` makes up for, is
 * held as `failure` until a listing succeeds, and `retry` sends it again, `pending` while one is out after a failure
 * (the only time it is drawn, on the Try again of the failure). A listing made while none is held is not `pending`, so
 * the window does not render for the change it would make and unmake, at mount and at every snapshot. Nothing is shown
 * of it here: it is the page's to draw. */
export function useSnapshotListing(active: boolean, key: string, send: () => Promise<unknown>, quiet: readonly string[]): {
  failure: ListingFailure | undefined;
  pending: boolean;
  retry: () => void;
} {
  const snapshot = useConnectionSnapshot();
  const [failure, setFailure] = useState<ListingFailure>();
  const [pending, setPending] = useState(false);
  const [tries, setTries] = useState(0);
  const sendNow = useStableHandler(send);
  const listable = active && snapshot !== undefined;
  useEffect(() => {
    if (!listable) return;
    let current = true;
    const afterFailure = failure !== undefined;
    if (afterFailure) setPending(true);
    sendNow().then(
      () => {
        if (!current) return;
        if (afterFailure) {
          setPending(false);
          setFailure(undefined);
        }
      },
      (error: unknown) => {
        if (!current) return;
        if (afterFailure) setPending(false);
        if (error instanceof CodedError && quiet.includes(error.code)) return;
        setFailure((prior) => ({ message: errorPresentation(error), attempt: (prior?.attempt ?? -1) + 1 }));
      },
    );
    return () => {
      current = false;
    };
  }, [listable, snapshot, key, tries]);
  return { failure, pending, retry: () => setTries((count) => count + 1) };
}
