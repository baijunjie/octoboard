import type { ArchiveViewState } from "../archive/ArchiveView";
import { useStableHandler } from "../components/hooks/useLatest";
import { focusTopBar } from "../components/focus/regionFocus";
import { ownerAfterMove, ownerForSession, type AsideOwner } from "../layout/asideOwner";
import { archiveKey, isLive, type Location } from "../navigation/history";
import { useNavigationHistory } from "../navigation/useNavigationHistory";
import type { Console, Project, Session } from "../protocol";
import { focusFor, focusKey, resolveFocus } from "../sidebar/focus";
import type { ArchiveScope, FocusTarget } from "../sidebar/types";
import { useDaemon } from "../store";

/**
 * Back and Forward through what the content area has shown: the current console, the selected session, the focus
 * mode and the archive view. Going back or forward sets that state straight. A press in the previewing sidebar that
 * makes its console current is one visit with what the press goes on to select (`stableCommitPreview`). `ready` is
 * whether the first snapshot has arrived, before which nothing is recorded.
 */
export function useAppNavigation({
  ready,
  focus,
  setFocus,
  selectConsole,
  currentConsoleId,
  selectedSession,
  selectedSessionId,
  setSelectedSessionId,
  archiveOpen,
  archiveScope,
  setArchiveScopeByUser,
  setArchiveRestore,
  archiveViewStates,
  revealArchive,
  consoles,
  projects,
  asideOwner,
  setAsideOwner,
  holdForPress,
  releasePress,
  previewing,
  shownConsole,
  focusTerminal,
}: {
  ready: boolean;
  /** The sidebar's focus mode. */
  focus: FocusTarget | undefined;
  setFocus: (target: FocusTarget | undefined) => void;
  selectConsole: (consoleId: string) => void;
  currentConsoleId: string | undefined;
  selectedSession: Session | undefined;
  selectedSessionId: string | undefined;
  setSelectedSessionId: (id: string | undefined) => void;
  archiveOpen: boolean;
  archiveScope: ArchiveScope | undefined;
  /** Changes what the archive view shows or closes it, as a move the user made (`useArchiveView`). */
  setArchiveScopeByUser: (scope: ArchiveScope | undefined) => void;
  setArchiveRestore: (state: ArchiveViewState | undefined) => void;
  /** How far each scope's archive list was loaded and scrolled, by `archiveKey`. */
  archiveViewStates: { readonly current: Map<string, ArchiveViewState> };
  revealArchive: () => void;
  consoles: Map<string, Console>;
  projects: Map<string, Project>;
  asideOwner: AsideOwner | undefined;
  setAsideOwner: (owner: AsideOwner | undefined) => void;
  /** The console switch's follow-up waits for the press (`useConsoleSwitchFollowUp`). */
  holdForPress: () => void;
  releasePress: () => void;
  /** The sidebar shows a console other than the current one, which the press would make current. */
  previewing: boolean;
  shownConsole: Console | undefined;
  focusTerminal: () => void;
}) {
  const { store } = useDaemon();
  // What the content area shows, for Back and Forward. A session that is gone counts as none selected.
  const location: Location = {
    console: currentConsoleId,
    session: selectedSession?.id,
    focus: focus && focusKey(focus),
    archive: archiveOpen ? archiveScope : undefined,
  };
  const navigation = useNavigationHistory({
    location,
    ready,
    isLive: (target, sessions) => isLive(target, { consoles, projects, sessions }),
    // Sets the state straight, not through `selectSession`: going back to an interrupted session
    // must not resume it. A focus mode outside the session's context is left, as selecting does.
    apply: (target) => {
      if (target.console) selectConsole(target.console);
      const sessions = store.getState().sessions;
      const session = target.session ? sessions.get(target.session) : undefined;
      setFocus(focusFor(resolveFocus(target.focus, target.console, projects, sessions), session, sessions));
      setSelectedSessionId(target.session);
      // The aside follows the session moved to, as selecting it does, and a move that keeps the
      // session leaves it to whoever owns it; either way under the rule of making another console
      // current, as the place moved to may have another current console than its session's.
      const owner = target.session !== selectedSessionId ? ownerForSession(session) : asideOwner;
      setAsideOwner(ownerAfterMove(owner, session, target.console, projects));
      setArchiveRestore(target.archive && archiveViewStates.current.get(archiveKey(target.archive)));
      setArchiveScopeByUser(target.archive);
      if (target.archive) revealArchive();
    },
    // Focus a move leaves nowhere goes to the terminal, or, where that takes none (no session, or
    // a touch screen, `focusTerminal`), to the top bar's first control.
    focusTerminal: () => {
      focusTerminal();
      const active = document.activeElement;
      if (!active || active === document.body || active.matches(":disabled")) focusTopBar();
    },
  });

  // A press in the previewing sidebar makes its console current. The console change and that
  // selection, or the console change and the restore of what the console last showed (the follow-up
  // `releasePress` runs), are one visit in the history, and neither the last session nor the aside
  // of the console change shows for the length of the press. The follow-up's release comes first,
  // then the join ends.
  const commitPreview = () => {
    if (!previewing || !shownConsole) return;
    holdForPress();
    navigation.joinPressVisits(releasePress);
    selectConsole(shownConsole.id);
    setArchiveScopeByUser(undefined);
  };
  const stableCommitPreview = useStableHandler(commitPreview);
  const stableBack = useStableHandler(navigation.back);
  const stableForward = useStableHandler(navigation.forward);
  const { canGoBack, canGoForward } = navigation;
  return { canGoBack, canGoForward, stableBack, stableForward, stableCommitPreview };
}
