import { useEffect, useRef, useState } from "react";

import { PANE_EDGE_DWELL_MS } from "./paneOverlay";
import { useLatest } from "../components/hooks/useLatest";

/** How long a mouse pointer has to rest on a console's avatar before a hidden sidebar floats in, so
 * that one merely crossing the rail on its way elsewhere does not make it flash. Moving on to
 * another avatar while the sidebar is out waits for nothing. */
export const CONSOLE_PEEK_DWELL_MS = 130;

/** What the hook needs of the floating sidebar's own state (`usePanePeek`). */
interface SidebarPeekControl {
  /** Whether the sidebar may float right now. */
  peekable: boolean;
  active: boolean;
  reveal: () => void;
  keep: () => void;
  leave: () => void;
}

export interface ConsolePeek {
  /** Whether the sidebar can reveal on hover: automatically floating, or hidden in a wide window. */
  peekable: boolean;
  /** The console whose view the floating sidebar shows: the last one the pointer rested on. It is
   * kept after the sidebar has slid away, so the view does not change as it leaves, and is none
   * while the sidebar cannot float (docked, or in mobile interaction). */
  consoleId?: string;
  /** Floats the sidebar in at once, showing the current console: the pointer is on the sidebar's
   * toggle, which is aimed at and so has no dwell. Drops a preview of another console. Nothing
   * where the sidebar cannot float. */
  reveal: () => void;
  /** A mouse pointer is on that console's avatar: floats the sidebar in after the dwell, or while
   * it is out shows this console at once. Nothing where the sidebar cannot float. */
  hoverConsole: (consoleId: string) => void;
  /** The pointer left an avatar: drops a dwell in progress, and starts the slide-away timer. */
  leaveConsole: () => void;
  /** Resting on blank rail space reveals the current console after a short delay. */
  hoverRail: () => void;
  /** Cancels blank-space dwell when the pointer reaches a control or leaves the rail. */
  leaveRail: () => void;
}

/** Which console the hidden docked sidebar floats in for, and when: the pointer resting on a
 * console's avatar on the rail brings it up showing that console, the preview; its toggle brings it
 * up showing the current one. Hovering never makes
 * the console the current one; that is up to a press inside the panel. */
export function useConsolePeek(peek: SidebarPeekControl): ConsolePeek {
  const [consoleId, setConsoleId] = useState<string>();
  const dwell = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const railDwell = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const onRail = useRef(false);
  // The dwell's callback outlives the render that scheduled it.
  const latest = useLatest(peek);

  const leaveRail = () => {
    onRail.current = false;
    clearTimeout(railDwell.current);
  };
  const hoverRail = () => {
    if (onRail.current || !latest.current.peekable) return;
    onRail.current = true;
    clearTimeout(dwell.current);
    railDwell.current = setTimeout(() => {
      setConsoleId(undefined);
      latest.current.reveal();
    }, PANE_EDGE_DWELL_MS);
  };
  const hoverConsole = (id: string) => {
    leaveRail();
    clearTimeout(dwell.current);
    if (!latest.current.peekable) return;
    if (latest.current.active) {
      latest.current.keep();
      setConsoleId(id);
      return;
    }
    dwell.current = setTimeout(() => {
      setConsoleId(id);
      latest.current.reveal();
    }, CONSOLE_PEEK_DWELL_MS);
  };
  const reveal = () => {
    leaveRail();
    clearTimeout(dwell.current);
    if (!latest.current.peekable) return;
    setConsoleId(undefined);
    latest.current.reveal();
  };
  const leaveConsole = () => {
    clearTimeout(dwell.current);
    latest.current.leave();
  };

  useEffect(() => {
    if (!peek.peekable) {
      leaveRail();
      clearTimeout(dwell.current);
      setConsoleId(undefined);
    }
  }, [peek.peekable]);
  useEffect(() => () => { clearTimeout(dwell.current); clearTimeout(railDwell.current); }, []);

  return { peekable: peek.peekable, consoleId: peek.peekable ? consoleId : undefined, reveal, hoverConsole, leaveConsole, hoverRail, leaveRail };
}
