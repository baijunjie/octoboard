import { useEffect, useRef, useState } from "react";

import { useAttendedDelivery } from "./useAttendedDelivery";

/**
 * The sentence for a view's `StatusAnnouncer` when a flag turns on while the view is open, and
 * `undefined` otherwise: a flag found on at first sight, or already on when `subject` changed (another
 * terminal, a hidden view shown), is read with the view it qualifies and is not announced apart from it.
 *
 * While `deferred` (a cover's announcement holds the view, or the view is out of reach, as the agent area is while a
 * maximized or floating panel makes it inert) the sentence is held back: a flag that
 * turned on meanwhile is queued once the deferral ends, as is one queued but not yet said when the
 * deferral began, and one already said is not said again then.
 * A queued sentence is written through `useAttendedDelivery`, after the wait of a focus move in attended
 * time, so it comes after the cover lifting and any focus it moves, and not while the user is away; it is
 * dropped unsaid when the flag turns off or `subject` changes (or goes `undefined`, the view hidden) first.
 */
export function useAnnouncedOnRise(subject: string | undefined, flag: boolean, sentence: string, deferred = false): string | undefined {
  const previous = useRef({ subject, flag, deferred });
  const [risen, setRisen] = useState<string>();
  // Whether the current rise has been written: set once the delivery's sentence is committed, read by the rise effect.
  const saidRef = useRef(false);
  useEffect(() => {
    const { subject: was, flag: wasOn, deferred: wasDeferred } = previous.current;
    previous.current = { subject, flag, deferred };
    let next = risen;
    if (!flag || was !== subject) next = undefined;
    else if (!wasOn) {
      saidRef.current = false;
      next = subject;
    } else if (deferred && !wasDeferred && saidRef.current) next = undefined;
    if (next !== risen) setRisen(next);
  }, [subject, flag, deferred]);
  const queued = flag && !deferred && risen !== undefined && risen === subject ? subject : undefined;
  const said = useAttendedDelivery(queued, sentence);
  useEffect(() => {
    if (said !== undefined) saidRef.current = true;
  }, [said]);
  return said;
}
