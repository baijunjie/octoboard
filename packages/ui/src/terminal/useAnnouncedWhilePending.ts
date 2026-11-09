import { useEffect, useRef, useState } from "react";

import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/announce/StatusAnnouncer";

/**
 * The sentence for a view's `StatusAnnouncer` while its terminal's process cannot be reached, once:
 * at once when it turns on under the open `subject`, and after the delay of a focus move when the
 * view was opened already pending (a session picked, a hidden tab shown), since that opening moves
 * focus into the terminal and its announcement would replace the sentence. Retries, renames and
 * other updates change neither `subject` nor `pending`, so the sentence is not said again.
 * `subject` is `undefined` while the view is not shown: nothing is said for it.
 *
 * The sentence is not said when `coverHasFocus` (read as it would be said) finds focus in the cover:
 * the focused region is described by the same copy, and saying both reads it twice.
 */
export function useAnnouncedWhilePending(
  subject: string | undefined,
  pending: boolean,
  sentence: string,
  coverHasFocus: () => boolean,
): string | undefined {
  const previous = useRef<{ subject: string | undefined; pending: boolean }>(undefined);
  const [saidFor, setSaidFor] = useState<string>();
  useEffect(() => {
    const was = previous.current;
    previous.current = { subject, pending };
    if (!pending || subject === undefined) {
      setSaidFor(undefined);
      return;
    }
    if (was?.subject === subject && !was.pending) {
      setSaidFor(coverHasFocus() ? undefined : subject);
      return;
    }
    setSaidFor(undefined);
    const timer = setTimeout(() => {
      if (!coverHasFocus()) setSaidFor(subject);
    }, AFTER_FOCUS_MOVE_DELAY_MS);
    return () => clearTimeout(timer);
  }, [subject, pending]);
  return pending && saidFor !== undefined && saidFor === subject ? sentence : undefined;
}
