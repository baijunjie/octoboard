import { useEffect, useRef } from "react";

/** How long after a press ends its own handlers are given to set the owner before the held rule
 * runs: the click that selects a row follows the release, as with `joinPressVisits` in
 * `useNavigationHistory`. */
export const PRESS_SETTLE_MS = 50;

export interface ConsoleOwnerRule {
  /** A press is making another console current (a press inside the floating sidebar previewing
   * it): the rule for that change waits until the press has ended. */
  holdForPress: () => void;
  /** The user has just set the aside's owner (selecting a session, Browse files): a rule held for
   * the press this happened in has nothing left to do. */
  ownerSet: () => void;
}

/**
 * Runs `apply`, the aside's owner rule for a change of current console (`ownerAfterMove` in
 * `asideOwner.ts`), whenever `consoleId` changes. A console made current by a press inside the
 * previewing sidebar is the exception: that press usually goes on to select a session there, which
 * sets the owner itself, and applying the rule in between would take the aside away and give it
 * back a moment later — resizing the terminal twice, or mounting a report panel only to drop it.
 * So with `holdForPress` the rule waits for the press to end (its release, a cancelled pointer, or
 * the window losing focus, plus `PRESS_SETTLE_MS`) and is dropped if the owner was set meanwhile.
 * `apply` is read when it runs, so it sees the state as it is then.
 */
export function useConsoleOwnerRule(consoleId: string | undefined, apply: () => void): ConsoleOwnerRule {
  const latest = useRef(apply);
  latest.current = apply;
  // A press is being waited for / the console changed during it / the owner was set during it.
  const holding = useRef(false);
  const pending = useRef(false);
  const claimed = useRef(false);
  const stop = useRef<(() => void) | undefined>(undefined);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useEffect(() => {
    if (holding.current) pending.current = true;
    else latest.current();
  }, [consoleId]);

  const holdForPress = () => {
    stop.current?.();
    clearTimeout(timer.current);
    holding.current = true;
    pending.current = claimed.current = false;
    const release = () => {
      stop.current?.();
      timer.current = setTimeout(() => {
        holding.current = false;
        if (pending.current && !claimed.current) latest.current();
        pending.current = claimed.current = false;
      }, PRESS_SETTLE_MS);
    };
    for (const type of ["pointerup", "pointercancel"]) window.addEventListener(type, release, true);
    window.addEventListener("blur", release);
    stop.current = () => {
      for (const type of ["pointerup", "pointercancel"]) window.removeEventListener(type, release, true);
      window.removeEventListener("blur", release);
      stop.current = undefined;
    };
  };

  useEffect(
    () => () => {
      stop.current?.();
      clearTimeout(timer.current);
    },
    [],
  );

  return {
    holdForPress,
    ownerSet: () => {
      if (holding.current) claimed.current = true;
    },
  };
}
