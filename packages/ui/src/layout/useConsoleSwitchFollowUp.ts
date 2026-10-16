import { useEffect, useRef } from "react";

import { useLatest } from "../components/useLatest";

/** What the user can choose for themselves in the press that makes a console current. */
export type Choice = "selection" | "owner";

/** What a press inside the previewing sidebar left to the follow-up: each is true while the press
 * did not make that choice itself. */
export type HeldChoices = Record<Choice, boolean>;

export interface ConsoleSwitchFollowUp {
  /** A press is making another console current (a press inside the floating sidebar previewing
   * it): what follows the change waits until `releasePress`. */
  holdForPress: () => void;
  /** The press has ended and its own handlers have run: the held follow-up runs now. Called by the
   * owner of the press's end, which is also what ends the navigation join (`commitPreview`). */
  releasePress: () => void;
  /** The user has just selected a session or set the aside's owner: what a follow-up held for the
   * press this happened in would have shown for it is left to the user's. */
  claim: (what: Choice) => void;
}

/**
 * What follows a change of current console. `apply` is the aside's owner rule for it
 * (`ownerAfterMove` in `asideOwner.ts`), run whenever `consoleId` changes, however it came about.
 *
 * A console made current by a press inside the previewing sidebar is the exception: that press
 * usually goes on to select a session there, and showing the console's remembered session in
 * between would put it on screen for the length of the press, and take the aside away and give it
 * back a moment later. So with `holdForPress` the follow-up waits for `releasePress` and then runs
 * `restore` instead of `apply`, to show the console's remembered session and its aside. `restore`
 * is given the parts the press did not choose itself (`claim`): the session when none was
 * selected, the aside when its owner was not set (selecting a session does both). Nothing runs if
 * the press chose both. `holdForPress` must always be paired with `joinPressVisits(releasePress)`,
 * which is what ends the press.
 *
 * `apply` and `restore` are read when they run, so they see the state as it is then.
 */
export function useConsoleSwitchFollowUp(
  consoleId: string | undefined,
  apply: () => void,
  restore: (choices: HeldChoices) => void,
): ConsoleSwitchFollowUp {
  const latest = useLatest(apply);
  const latestRestore = useLatest(restore);
  // A press is being waited for / the console changed during it / what the press chose itself.
  const holding = useRef(false);
  const pending = useRef(false);
  const claimed = useRef(new Set<Choice>());

  useEffect(() => {
    if (holding.current) pending.current = true;
    else latest.current();
  }, [consoleId]);

  return {
    holdForPress: () => {
      holding.current = true;
      pending.current = false;
      claimed.current = new Set();
    },
    releasePress: () => {
      if (!holding.current) return;
      holding.current = false;
      const selection = !claimed.current.has("selection");
      const owner = !claimed.current.has("owner");
      if (pending.current && (selection || owner)) latestRestore.current({ selection, owner });
      pending.current = false;
      claimed.current = new Set();
    },
    claim: (what) => {
      if (holding.current) claimed.current.add(what);
    },
  };
}
