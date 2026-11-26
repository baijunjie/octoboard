import { useEffect, useRef, useState } from "react";

import { useAttendedDelivery } from "./useAttendedDelivery";

/**
 * The sentence for a view's `StatusAnnouncer` when an item arrives while the view shows `subject`, and `undefined`
 * otherwise: `keys` are the items the view shows, newest last, and `sentence` words the newest. Items found at first
 * sight, or already there when `subject` changed (another session shown), are read with the view and not announced
 * apart from it; each item that arrives afterwards is announced once, the newest winning when several come before one
 * is said.
 *
 * It is `useAnnouncedOnRise` for a list rather than a flag, and delivers the same way: through `useAttendedDelivery`,
 * after the wait of a focus move in attended time, and held while `deferred` (the view is out of reach, as the agent
 * area is while a maximized or floating panel makes it inert), then said once the deferral ends unless it was said
 * already. An arrival not yet said is dropped when its item leaves `keys` (dismissed, withdrawn) or `subject` changes.
 */
export function useAnnouncedOnArrival(subject: string | undefined, keys: readonly string[], sentence: string, deferred = false): string | undefined {
  const known = useRef<{ subject: string | undefined; keys: ReadonlySet<string> }>({ subject, keys: new Set(keys) });
  const [arrived, setArrived] = useState<string>();
  // The arrival last said and its sentence, held in state (a ref read during render would keep the hook from
  // compiling): an arrival said is not queued again when a deferral ends, and the region keeps its sentence for its own
  // read time (`clearAfterRead`) though it is no longer queued.
  const [spoken, setSpoken] = useState<{ key: string; text: string }>();
  const joined = keys.join("\n");
  useEffect(() => {
    const previous = known.current;
    known.current = { subject, keys: new Set(keys) };
    if (previous.subject !== subject) {
      setArrived(undefined);
      setSpoken(undefined);
      return;
    }
    const newest = [...keys].reverse().find((key) => !previous.keys.has(key));
    if (newest !== undefined) setArrived(newest);
    else if (arrived !== undefined && !keys.includes(arrived)) setArrived(undefined);
  }, [subject, joined]);
  const current = arrived !== undefined && keys.includes(arrived) ? arrived : undefined;
  const queued = !deferred && current !== undefined && spoken?.key !== current ? current : undefined;
  const delivered = useAttendedDelivery(queued, sentence);
  useEffect(() => {
    if (delivered !== undefined && queued !== undefined) setSpoken({ key: queued, text: delivered });
  }, [delivered, queued]);
  return spoken !== undefined && spoken.key === current ? spoken.text : undefined;
}
