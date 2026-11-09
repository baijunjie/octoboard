import { useEffect, useRef, useState } from "react";

/**
 * The sentence for a view's `StatusAnnouncer` when a flag turns on while the view is open, and
 * `undefined` otherwise: a flag found on at first sight, or already on when `subject` changed (another
 * terminal, a hidden view shown), is read with the view it qualifies and is not announced apart from it. The
 * sentence is dropped when the flag turns off.
 *
 * While `deferred` (another announcement holds the region) the sentence is held back: a flag that
 * turned on meanwhile is said once the deferral ends, and one already said is not said again then.
 */
export function useAnnouncedOnRise(subject: string | undefined, flag: boolean, sentence: string, deferred = false): string | undefined {
  const previous = useRef({ subject, flag, deferred });
  const [risen, setRisen] = useState<string>();
  useEffect(() => {
    const { subject: was, flag: wasOn, deferred: wasDeferred } = previous.current;
    previous.current = { subject, flag, deferred };
    if (!flag || was !== subject) setRisen(undefined);
    else if (!wasOn) setRisen(subject);
    else if (deferred && !wasDeferred) setRisen(undefined);
  }, [subject, flag, deferred]);
  return flag && !deferred && risen !== undefined && risen === subject ? sentence : undefined;
}
