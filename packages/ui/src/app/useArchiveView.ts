import { useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";

import type { ArchiveViewState } from "../archive/ArchiveView";
import { useStableHandler } from "../components/hooks/useLatest";
import { archiveKey } from "../navigation/history";
import type { Console, Project } from "../protocol";
import { indexSessions, NO_SESSIONS, sessionsOf } from "../sessions/sessionIndex";
import { useSession } from "../sessions/useSessions";
import type { ArchiveScope } from "../sidebar/types";
import { useDaemonStore } from "../store";

/**
 * The archive view over the agent terminal's area: what it lists (`archiveScope`, the console, project or console
 * session it is for, and their archived sessions), whether it is open, and how far each scope's list was loaded and
 * scrolled for Back and Forward to put it back. `openArchive` opens it afresh and `closeArchive` closes it, handing
 * focus to the terminal.
 */
export function useArchiveView({
  consoles,
  projects,
  revealArchive,
  sidebarOpen,
  closeSidebar,
  focusTerminal,
}: {
  consoles: ReadonlyMap<string, Console>;
  projects: ReadonlyMap<string, Project>;
  /** Clears what covers the agent terminal's area, where the archive view is. */
  revealArchive: () => void;
  sidebarOpen: boolean;
  closeSidebar: () => void;
  focusTerminal: () => void;
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
      return archiveBoundTo ? inConsole : inConsole.filter((session) => session.role === "console");
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

  const closeArchive = () => {
    setArchiveScope(undefined);
    focusTerminal();
  };

  const onArchiveViewState = (state: ArchiveViewState) => {
    if (archiveScope) archiveViewStates.current.set(archiveKey(archiveScope), state);
  };
  const stableOpenArchive = useStableHandler(openArchive);
  return {
    archiveScope,
    setArchiveScope,
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
