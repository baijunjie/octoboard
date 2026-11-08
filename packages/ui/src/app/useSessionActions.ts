import { useStableHandler } from "../components/hooks/useLatest";
import { ownerForSession, type AsideOwner } from "../layout/asideOwner";
import type { Console, Project, Session } from "../protocol";
import { indexSessions } from "../sessions/sessionIndex";
import { holdsAccountSwitch } from "../sessions/sessionStatus";
import { focusAfterSelect } from "../sidebar/focus";
import type { ArchiveScope, FocusTarget } from "../sidebar/types";
import { sessionOrderPreference, useDaemon } from "../store";
import { nextWaitingSession, waitingSessionsInTreeOrder } from "../sessions/waiting";

/**
 * Putting a session on screen, whichever control asked: selecting it (which resumes an interrupted one), reopening an
 * archived one, a chip of a console session's switch strip, the next waiting session, and a session this window just
 * opened. The terminal, the aside's owner and the sidebar's console and focus mode follow it, and an open archive view
 * gives way. The stable members are for the sidebar's, the rail's and the archive's rows.
 */
export function useSessionActions({
  focus,
  setFocus,
  selectConsole,
  setSelectionByUser,
  setOwnerByUser,
  archiveOpen,
  setArchiveScope,
  focusTerminal,
  resumeSession,
  asideFloats,
  sidebarOpen,
  closeSidebar,
  consoleList,
  projectList,
  selectedSessionId,
}: {
  /** The sidebar's focus mode. */
  focus: FocusTarget | undefined;
  setFocus: (target: FocusTarget | undefined) => void;
  selectConsole: (consoleId: string) => void;
  setSelectionByUser: (id: string | undefined) => void;
  setOwnerByUser: (owner: AsideOwner | undefined) => void;
  archiveOpen: boolean;
  setArchiveScope: (scope: ArchiveScope | undefined) => void;
  focusTerminal: () => void;
  resumeSession: (sessionId: string) => Promise<boolean>;
  asideFloats: boolean;
  sidebarOpen: boolean;
  closeSidebar: () => void;
  consoleList: Console[];
  projectList: Project[];
  selectedSessionId: string | undefined;
}) {
  const { store } = useDaemon();

  /** Puts a session this window just opened on screen; it is in the console the sidebar shows
   * (a dialog holds the window still, and a quick create does not call this once the console has
   * changed under it), so only the archive view, were it open, has to give way. */
  const showOpenedSession = (sessionId: string) => {
    setSelectionByUser(sessionId);
    // The reply that names the session has already been applied to the store by the time this runs.
    setOwnerByUser(ownerForSession(store.getState().sessions.get(sessionId)));
    setArchiveScope(undefined);
  };

  const selectSession = (session: Session, { enterFocus = false, resume = true } = {}) => {
    setSelectionByUser(session.id);
    // A console session brings its report into the aside, a project session its project's browser.
    setOwnerByUser(ownerForSession(session));
    // Whatever led here — the sidebar, the archive, the waiting-count button — the sidebar follows
    // the session: its console is the one shown, and focus mode on something the session does not
    // belong to is left, unless `enterFocus` asks for the focus mode of this console session itself.
    selectConsole(session.console_id);
    const nextFocus = focusAfterSelect(focus, session, enterFocus, store.getState().sessions);
    if (nextFocus !== focus) setFocus(nextFocus);
    if (archiveOpen) {
      setArchiveScope(undefined);
      focusTerminal();
    }
    // An interrupted session is resumed by selecting it, unless `resume` is off; an archived one only
    // shows its history, and is reopened by the Reopen button or an explicit Reopen (`reopenSession`).
    // One an account switch holds reads interrupted while its process is down, and the switch relaunches it.
    const switching = holdsAccountSwitch(store.getState().sessions.get(session.id) ?? session, store.getState().queuedAccountSwitches);
    if (resume && session.status === "interrupted" && !switching) void resumeSession(session.id);
  };

  const reopenSession = (session: Session) => {
    selectSession(session);
    if (session.status === "archived") void resumeSession(session.id);
  };

  // Pressing a chip is a selection like pressing the session's row, which also enters its focus mode.
  const switchConsoleSession = (session: Session) => selectSession(session, { enterFocus: true });

  const selectNextWaiting = () => {
    const state = store.getState();
    const sessions = Array.from(state.sessions.values());
    const next = nextWaitingSession(waitingSessionsInTreeOrder(consoleList, projectList, sessions, indexSessions(state.sessions).delegatingIds, state.queuedAccountSwitches, sessionOrderPreference.get()), selectedSessionId);
    if (!next) return;
    selectSession(next);
    // Picked from the rail inside the open drawer, it closes it as a pick in the sidebar does.
    if (asideFloats && sidebarOpen) closeSidebar();
  };

  // Handed to the sidebar's and the rail's rows and the archive view's rows, which are then rendered
  // again only for their own records: these closures capture the focus and the archive's state, so
  // each is one function for good.
  const stableSelectSession = useStableHandler((session: Session) => selectSession(session));
  // On a phone the drawer covers most of the terminal: picking a session in it closes it, so the
  // session's terminal is what shows (focused, except after a tap: `interactionIsTouch`). A wider
  // window leaves it open, beside the terminal.
  const stableSelectFromSidebar = useStableHandler((session: Session) => {
    selectSession(session);
    if (asideFloats && sidebarOpen) closeSidebar();
  });
  const stableSwitchConsoleSession = useStableHandler(switchConsoleSession);
  const stableReopenSession = useStableHandler(reopenSession);
  const stableNextWaiting = useStableHandler(selectNextWaiting);
  return {
    showOpenedSession,
    selectSession,
    stableSelectSession,
    stableSelectFromSidebar,
    stableSwitchConsoleSession,
    stableReopenSession,
    stableNextWaiting,
  };
}
