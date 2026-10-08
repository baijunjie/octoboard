import { useEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";

import { refocusIfLost } from "../components/refocusIfLost";
import { MODAL_OPEN } from "../layout/useRegionCycle";
import { usePlatform } from "../platform/react";
import { IS_MAC, useWindowShortcut } from "../useWindowShortcut";
import { canGo, EMPTY_HISTORY, go, type Location, locationKey, push, replaceCurrent } from "./history";

/** ⌘[ and ⌘]: Back and Forward, matched on the physical key so a keyboard layout does not move
 * them. Only ⌘ with nothing else held; Alt+arrow is not used, being the word motions in a terminal. */
function shortcutDirection(event: KeyboardEvent): -1 | 1 | undefined {
  if (!IS_MAC || !event.metaKey || event.ctrlKey || event.altKey || event.shiftKey) return undefined;
  if (event.code === "BracketLeft") return -1;
  if (event.code === "BracketRight") return 1;
  return undefined;
}

export interface Navigation {
  canGoBack: boolean;
  canGoForward: boolean;
  back: () => void;
  forward: () => void;
  /** Back or Forward as the shortcut does it, for a key press that reaches the window another way
   * (the report page's relay): nothing where the shortcut does not exist, or while a dialog or
   * menu is open. */
  moveByShortcut: (backward: boolean) => void;
}

/**
 * Back and Forward through the locations the content area has shown. `location` is what it shows
 * now, derived from the UI's own state each render: every change of it that Back and Forward did not
 * cause is recorded as a visit, so each way of navigating (the sidebar, the rail, the archive view,
 * focus mode, the waiting-count button) lands in the history without any of them knowing about it.
 * The history starts once `ready`, when the first snapshot has given the location its meaning.
 *
 * A change the user did not make takes the place of the entry on screen instead of adding one: when
 * what that entry names has gone (the selected session deleted, the console shown deleted, a focus
 * mode that can no longer be shown), the UI falls back by itself and the fallback is what the entry
 * now stands for. `isLive` says whether an entry still names things that exist; those that do not
 * are passed over when moving.
 *
 * Moving calls `apply` with the location to show, which sets the UI's state to it without
 * selecting anything the way a click would (an interrupted session is not resumed by going back to
 * it). It is rendered at once, and the entry then takes the place the UI ended up at, which is not
 * the asked one when `apply` cannot express it or the UI settles elsewhere; that never counts as a
 * visit, so Forward stays available. That render is the only one that is a move's: `apply` has to
 * reach the final location in it (the state it sets, and anything derived from it, in that one
 * render), since a corrective change in a later effect arrives after the move has ended and is
 * recorded as a visit. Focus that the move left on nothing (or on a Back or Forward button that
 * has just become disabled) goes to the terminal, through `focusTerminal`. The history is in
 * memory only.
 *
 * ⌘[ and ⌘] work in the macOS application, where `windowChrome` is present (`useWindowShortcut`).
 */
export function useNavigationHistory({
  location,
  ready,
  isLive,
  apply,
  focusTerminal,
}: {
  location: Location;
  ready: boolean;
  isLive: (location: Location) => boolean;
  apply: (location: Location) => void;
  focusTerminal: () => void;
}): Navigation {
  const { windowChrome } = usePlatform();
  const [history, setHistory] = useState(EMPTY_HISTORY);
  // Set while a move is being applied, so that the location it arrives at is not a visit.
  const moving = useRef(false);
  const recorded = useRef<string>(undefined);

  // After every render, so that the render a move causes is also the one that ends it, whether or
  // not the location changed.
  const key = locationKey(location);
  useEffect(() => {
    const moved = moving.current;
    moving.current = false;
    if (!ready || (key === recorded.current && !moved)) return;
    recorded.current = key;
    setHistory((h) => {
      const current = h.entries[h.index];
      if (moved) return current && locationKey(current) === key ? h : replaceCurrent(h, location);
      return current && !isLive(current) ? replaceCurrent(h, location) : push(h, location);
    });
  });

  const move = (direction: -1 | 1) => {
    const target = go(history, direction, isLive);
    if (!target) return;
    moving.current = true;
    flushSync(() => {
      setHistory(target.history);
      apply(target.location);
    });
    refocusIfLost(focusTerminal);
  };

  const moveByShortcut = (backward: boolean) => {
    if (!windowChrome || document.querySelector(MODAL_OPEN)) return;
    move(backward ? -1 : 1);
  };
  useWindowShortcut(shortcutDirection, (direction) => moveByShortcut(direction === -1), windowChrome !== undefined);

  return {
    canGoBack: canGo(history, -1, isLive),
    canGoForward: canGo(history, 1, isLive),
    back: () => move(-1),
    forward: () => move(1),
    moveByShortcut,
  };
}
