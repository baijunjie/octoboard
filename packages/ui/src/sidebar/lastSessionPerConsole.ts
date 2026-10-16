import { useEffect, useRef } from "react";

import type { Session } from "../protocol";

/** The session to bring back when `consoleId` is made current: the one last selected there, while
 * it exists and still belongs to that console. */
export function sessionToRestore(
  lastByConsole: ReadonlyMap<string, string>,
  consoleId: string,
  sessions: ReadonlyMap<string, Session>,
): Session | undefined {
  const id = lastByConsole.get(consoleId);
  const session = id === undefined ? undefined : sessions.get(id);
  return session?.console_id === consoleId ? session : undefined;
}

/** Remembers the session last selected in each console, in memory, whichever way it was selected.
 * Returns the lookup of the one to bring back when a console is made current. */
export function useLastSessionPerConsole(
  selected: Session | undefined,
  sessions: ReadonlyMap<string, Session>,
): (consoleId: string) => Session | undefined {
  const lastByConsole = useRef(new Map<string, string>());
  useEffect(() => {
    if (selected) lastByConsole.current.set(selected.console_id, selected.id);
  }, [selected?.id]);
  return (consoleId) => sessionToRestore(lastByConsole.current, consoleId, sessions);
}
