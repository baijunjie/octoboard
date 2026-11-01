import type { Launch, Session } from "../protocol";
import { inheritedLaunch, sessionModelEffort } from "../sessions/sessionModel";
import { useDaemonStoreEqual } from "./react";

/** The model and effort a session is shown running with (`sessionModelEffort`), what it inherits being read from the store. */
export function useSessionLaunch(session: Session): Launch {
  const inherited = useDaemonStoreEqual((s) => inheritedLaunch(session, s.consoles, s.projects));
  return sessionModelEffort(session, inherited);
}
