import { useRef, useState } from "react";

import { noteSessionSwitch } from "../project-browser/git/sessionFollow";
import { useSession } from "../store";

/** The session the terminal shows: its id as set, and its record while the store holds it (undefined once it is
 * gone). Every change of the id is noted for the project panes, which follow it even when one was not mounted while
 * it changed (`sessionFollow.ts`). */
export function useSelectedSession() {
  const [selectedSessionId, setSelectedSessionIdState] = useState<string>();
  const selectedSessionRef = useRef<string>(undefined);
  const setSelectedSessionId = (id: string | undefined) => {
    if (id !== selectedSessionRef.current) noteSessionSwitch();
    selectedSessionRef.current = id;
    setSelectedSessionIdState(id);
  };
  const selectedSession = useSession(selectedSessionId);
  return { selectedSessionId, selectedSession, setSelectedSessionId };
}
