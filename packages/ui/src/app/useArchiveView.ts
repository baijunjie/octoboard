import { useLayoutEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { useShallow } from "zustand/react/shallow";

import type { ArchiveViewState } from "../archive/ArchiveView";
import { useLatest, useStableHandler } from "../components/hooks/useLatest";
import { ringForAutomaticFocus } from "../input/touchInput";
import { archiveKey } from "../navigation/history";
import type { Console, Project } from "../protocol";
import { indexSessions, NO_SESSIONS, sessionsOf } from "../sessions/sessionIndex";
import { useSession } from "../store";
import type { ArchiveScope } from "../sidebar/types";
import { useDaemonStore } from "../store";

/**
 * The archive view over the agent terminal's area: what it lists (`archiveScope`, the console, project or console
 * session it is for, and their archived sessions), whether it is open, and how far each scope's list was loaded and
 * scrolled for Back and Forward to put it back. `openArchive` opens it afresh and `closeArchive` closes it. Whichever way
 * it closes, focus it held goes on to the terminal (`handOffToTerminal`, which must also land from `<body>` and from the
 * closing view).
 */
export function useArchiveView({
  consoles,
  projects,
  revealArchive,
  sidebarOpen,
  closeSidebar,
  handOffToTerminal,
}: {
  consoles: ReadonlyMap<string, Console>;
  projects: ReadonlyMap<string, Project>;
  /** Clears what covers the agent terminal's area, where the archive view is. */
  revealArchive: () => void;
  sidebarOpen: boolean;
  closeSidebar: () => void;
  handOffToTerminal: () => void;
}) {
  const [archiveScope, setArchiveScope] = useState<ArchiveScope>();
  // How far each scope's archive list was loaded and scrolled, by `archiveKey`, for Back and
  // Forward to put it back. A visit opened fresh (`openArchive`) forgets it and starts at the top.
  const archiveViewStates = useRef(new Map<string, ArchiveViewState>());
  const [archiveRestore, setArchiveRestore] = useState<ArchiveViewState>();
  const archiveConsole = archiveScope ? consoles.get(archiveScope.console) : undefined;
  const archiveProjectId = archiveScope && "project" in archiveScope ? archiveScope.project : undefined;
  const archiveProject = archiveProjectId ? projects.get(archiveProjectId) : undefined;
  const archiveConsoleSessionId = archiveScope && "consoleSession" in archiveScope ? archiveScope.consoleSession : undefined;
  const archiveBoundTo = useSession(archiveConsoleSessionId);
  // The archive view closes by itself once what it lists is gone.
  const archiveOpen =
    archiveConsole !== undefined &&
    (archiveProjectId === undefined || archiveProject !== undefined) &&
    (archiveConsoleSessionId === undefined || archiveBoundTo !== undefined);
  // What the archive view lists, read here only while it is open, and the same array until one of
  // those sessions changes.
  const archiveSessions = useDaemonStore(
    useShallow((s) => {
      if (!archiveOpen || !archiveConsole) return NO_SESSIONS;
      const index = indexSessions(s.sessions);
      if (archiveProject) return sessionsOf(index.byProject, archiveProject.id);
      const inConsole = sessionsOf(index.byConsole, archiveConsole.id);
      return archiveBoundTo ? inConsole : inConsole.filter((session) => session.kind === "console");
    }),
  );

  const openArchive = (scope: ArchiveScope) => {
    // Asking for the archive already open changes nothing, so what Back would restore stays.
    const alreadyOpen = archiveOpen && archiveScope !== undefined && archiveKey(archiveScope) === archiveKey(scope);
    if (!alreadyOpen) {
      archiveViewStates.current.delete(archiveKey(scope));
      setArchiveRestore(undefined);
    }
    setArchiveScope(scope);
    revealArchive();
    // Below the `docked` breakpoint the sidebar is a drawer over the terminal, and would keep
    // covering the archive that was just asked for.
    if (sidebarOpen) closeSidebar();
  };

  // The closing view hands focus on from here, once the commit that closes it is done: this runs after the pane's own
  // effects, so the pane's handle and refs already hold what it shows now (the layer, rendered before the pane in the
  // tree, would run its effects before the pane's). Only focus the view held, or lost to `<body>` meanwhile, is handed
  // on. A close the user made (`setArchiveScopeByUser`, `closeArchive`) leaves the modality alone, so the ring follows
  // the input that closed it; one nobody made (what the view lists is gone) lets the ring follow the last input, as any
  // automatic move does. The flag is read and cleared here, in the one commit that carries the change.
  const closedByUser = useRef(false);
  const wasOpen = useRef(false);
  useLayoutEffect(() => {
    const closed = wasOpen.current && !archiveOpen;
    wasOpen.current = archiveOpen;
    const byUser = closedByUser.current;
    closedByUser.current = false;
    if (!closed) return;
    const active = document.activeElement;
    if (active !== document.body && !active?.closest("[data-region=archive]")) return;
    if (!byUser) ringForAutomaticFocus();
    handOffToTerminal();
  });
  const latestOpen = useLatest({ archiveOpen, archiveScope });
  // A change of the scope that the user made: Back and Forward, a console switch, a session opened or selected from
  // this window. Read from the latest render, as a session opened by a reply arrives after an await. The scope already
  // set commits nothing, which would leave the flag standing for a later close nobody made.
  const setArchiveScopeByUser = useStableHandler((scope: ArchiveScope | undefined) => {
    if (latestOpen.current.archiveOpen && scope !== latestOpen.current.archiveScope) closedByUser.current = true;
    setArchiveScope(scope);
  });
  // Closes the view in a render of its own, so the pane under it is reachable by the time focus is handed on.
  // Only from an event or a listener, not while rendering or from an effect.
  const closeArchive = () => flushSync(() => setArchiveScopeByUser(undefined));

  const onArchiveViewState = (state: ArchiveViewState) => {
    if (archiveScope) archiveViewStates.current.set(archiveKey(archiveScope), state);
  };
  const stableOpenArchive = useStableHandler(openArchive);
  return {
    archiveScope,
    setArchiveScopeByUser,
    archiveConsole,
    archiveProject,
    archiveBoundTo,
    archiveOpen,
    archiveSessions,
    archiveRestore,
    setArchiveRestore,
    archiveViewStates,
    closeArchive,
    onArchiveViewState,
    stableOpenArchive,
  };
}
