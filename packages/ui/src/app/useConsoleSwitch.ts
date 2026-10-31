import { useEffect } from "react";
import { flushSync } from "react-dom";

import { useStableHandler } from "../components/hooks/useLatest";
import { ownerAfterMove, ownerForSession, sameOwner, type AsideOwner } from "../layout/asideOwner";
import { type HeldChoices, useConsoleSwitchFollowUp } from "../layout/useConsoleSwitchFollowUp";
import type { Project, Session } from "../protocol";
import { useLastSessionPerConsole } from "../sidebar/lastSessionPerConsole";
import type { ArchiveScope } from "../sidebar/types";
import { useDaemon } from "../store";

/**
 * What making another console current brings with it: the aside's owner follows the move, and the
 * console's last session and that session's aside come back, at once or, for a press in the
 * previewing sidebar, once the press is over (`followUp`). A selection or an owner the user sets
 * meanwhile goes through `setSelectionByUser` and `setOwnerByUser`, which settle what the press holds.
 * `ready` is whether the first snapshot has arrived: before it, an owner the store does not hold yet
 * is not gone.
 */
export function useConsoleSwitch({
  ready,
  projects,
  currentConsoleId,
  selectConsole,
  selectedSession,
  setSelectedSessionId,
  asideOwnerState,
  asideOwner,
  setAsideOwner,
  setArchiveScope,
}: {
  ready: boolean;
  projects: ReadonlyMap<string, Project>;
  currentConsoleId: string | undefined;
  selectConsole: (consoleId: string) => void;
  selectedSession: Session | undefined;
  setSelectedSessionId: (id: string | undefined) => void;
  /** The aside's owner as last set, which may name something gone. */
  asideOwnerState: AsideOwner | undefined;
  /** The aside's owner while what it names is there. */
  asideOwner: AsideOwner | undefined;
  setAsideOwner: (owner: AsideOwner | undefined) => void;
  setArchiveScope: (scope: ArchiveScope | undefined) => void;
}) {
  const { store } = useDaemon();
  // An owner that is gone leaves no aside, and is not brought back by anything that appears later.
  useEffect(() => {
    if (asideOwnerState && !asideOwner && ready) setAsideOwner(undefined);
  }, [asideOwnerState, asideOwner, ready]);
  const lastSessionIn = useLastSessionPerConsole(selectedSession, () => store.getState().sessions);
  /** Shows the session last selected in `consoleId`, if it is still there, and the aside of that
   * session; each part only where `choices` says so. Like Back and Forward it only shows it: an
   * interrupted session is not resumed, an archived one not reopened. */
  const showLastSession = (consoleId: string | undefined, { selection, owner }: HeldChoices) => {
    const session = consoleId ? lastSessionIn(consoleId) : undefined;
    if (selection) setSelectedSessionId(session?.id);
    if (owner) setAsideOwner(ownerForSession(session));
  };
  // Making another console current closes a browser of a project of the console left behind (a
  // console the floating sidebar only previews is not current), and coming back to the selected
  // session's console gives the aside back to the session's own owner. A console made current by a
  // press in the previewing sidebar then shows its last session and that session's aside once the
  // press has ended, except what the press chose itself (`commitPreview`).
  const followUp = useConsoleSwitchFollowUp(
    currentConsoleId,
    () => {
      const next = ownerAfterMove(asideOwnerState, selectedSession, currentConsoleId, projects);
      if (!sameOwner(next, asideOwnerState)) setAsideOwner(next);
    },
    // Rendered at once: the history's recording effect has to run before the navigation join ends
    // (`commitPreview`), or the restore would be a visit of its own.
    (choices) => flushSync(() => showLastSession(currentConsoleId, choices)),
  );
  /** The owner as the user just set it, which settles a console change held for the same press. */
  const setOwnerByUser = (owner: AsideOwner | undefined) => {
    followUp.claim("owner");
    setAsideOwner(owner);
  };
  /** The selection as the user just made it, which settles a console change held for the same press. */
  const setSelectionByUser = (id: string | undefined) => {
    followUp.claim("selection");
    setSelectedSessionId(id);
  };
  /** Makes a console current by a press on the rail, or a console this window created: the terminal
   * and the aside go with it, and an open archive view gives way. */
  const switchConsole = (consoleId: string) => {
    if (consoleId === currentConsoleId) return;
    selectConsole(consoleId);
    setArchiveScope(undefined);
    showLastSession(consoleId, { selection: true, owner: true });
  };
  const stableSwitchConsole = useStableHandler(switchConsole);
  return { followUp, setOwnerByUser, setSelectionByUser, switchConsole, stableSwitchConsole };
}
