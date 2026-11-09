import { useEffect, useState } from "react";

import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { useLatest } from "../components/hooks/useLatest";

/**
 * `sentence` for a `StatusAnnouncer` whose region can be hidden from assistive technology for a while: a modal open
 * over the window, or the area holding it made inert by a maximized or floating panel. A sentence that arrives or
 * changes while `hidden` would be written into a region nobody hears, so it is held, and written once the region is
 * exposed again and `AFTER_FOCUS_MOVE_DELAY_MS` has passed, after the focus move that exposing it makes. A sentence
 * already written stays in the region while hidden: clearing and rewriting it would say it a second time. Changes made
 * while hidden are not replayed one by one: what is written on exposure is the sentence current then, if any.
 * `stillDue` is read as that wait ends; when it says no (a focused region that already reads the same copy), the held
 * sentence counts as delivered without being written.
 */
export function useHeldWhileHidden(
  sentence: string | undefined,
  hidden: boolean,
  { stillDue }: { stillDue?: () => boolean } = {},
): string | undefined {
  const stillDueRef = useLatest(stillDue);
  const [state, setState] = useState<{ held: boolean; spoken: string | undefined; silent?: boolean }>({ held: false, spoken: undefined });
  useEffect(() => {
    if (!sentence) {
      setState((s) => (s.held || s.spoken !== undefined ? { held: false, spoken: undefined } : s));
      return;
    }
    if (hidden) {
      if (state.spoken !== sentence) setState({ held: true, spoken: state.spoken });
      return;
    }
    if (!state.held) {
      if (state.spoken !== sentence) setState({ held: false, spoken: sentence });
      return;
    }
    const timer = setTimeout(() => setState({ held: false, spoken: sentence, silent: !(stillDueRef.current?.() ?? true) }), AFTER_FOCUS_MOVE_DELAY_MS);
    return () => clearTimeout(timer);
  }, [sentence, hidden]);
  if (state.silent && state.spoken === sentence) return undefined;
  return hidden || state.held ? (state.spoken === sentence ? sentence : undefined) : sentence;
}
