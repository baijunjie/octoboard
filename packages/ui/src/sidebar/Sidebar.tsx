import React, { useEffect, useState } from "react";
import { useShallow } from "zustand/react/shallow";

import { FadedScroller } from "../components/scroll/scrollFade";
import { STICKY_HEADING_HEIGHT } from "../components/rows/SectionHeading";
import type { Console, Project, Session } from "../protocol";
import type { PaneWidth } from "../layout/paneWidth";
import type { DockPhase } from "../layout/paneDockMotion";
import type { PanePeek } from "../layout/usePaneToggles";
import { indexSessions, sessionsOf } from "../sessions/sessionIndex";
import { useConsoleSessions, useDaemonStore, useOwnerSessions, useProjectSessions } from "../store";
import { ConsoleBody, ConsoleHeader } from "./ConsoleView";
import { ConsoleSessionFocusView, ProjectFocusView } from "./FocusView";
import { NO_FILTER, type ProjectFilter } from "./ProjectFilter";
import { pinAfterFoldAction, projectFoldControl, reconcileExpandPins } from "./projectFold";
import { SidebarFrame } from "./SidebarFrame";
import type { FocusTarget, SidebarHandlers } from "./types";

interface SidebarProps extends SidebarHandlers {
  projects: Project[];
  selectedSessionId?: string;
  /** The console the sidebar shows: the current one, or while the hidden sidebar floats in, the one
   * whose avatar the pointer is on. The caller resolves it to an existing one, and does not render
   * the sidebar while there is none. */
  shownConsole: Console;
  /** The project or console session in focus mode, if any; it belongs to `shownConsole`. */
  focus?: FocusTarget;
  /** Whether the drawer is open below the `docked` breakpoint; above it the docked sidebar is
   * shown or, with `peek`, hidden (`usePaneToggles` owns the state, resetting it once the window no
   * longer needs it). */
  open: boolean;
  /** The drawer is closed below the `docked` breakpoint, its exit included: out of reach. */
  drawerAway?: boolean;
  /** The hover reveal of the sidebar while the user has hidden it from the top bar. The card
   * (fixed, inset from the rail and the other chrome, rounded, edged) is only up
   * while this hover is, or while that card is sliding away. The rest of the time a hidden docked
   * sidebar is the same column as the shown one, clipped to nothing. `undefined` while the docked
   * sidebar is shown. */
  peek?: Pick<PanePeek, "active" | "keep" | "leave">;
  /** Hears the docked column's show/hide phase, including one settled before paint, so a sibling
   * such as the resize handle can follow it. */
  onColumnPhase?: (phase: DockPhase) => void;
  /** A press began inside the sidebar while it is hidden-docked (floating or sliding away), before
   * whatever was pressed acts: where the console it is showing, if that is not the current one,
   * becomes the current one. */
  onPeekPress?: () => void;
  /** The docked-mode width and its setters; `App` owns it (`app/usePaneWidths.ts`) so it can also
   * be read from outside the sidebar. */
  sidebarWidth: PaneWidth;
}

/** The sidebar: one console at a time, the current one picked on the rail, or the one a hover on the rail previews (see
 * docs/product/sidebar.md, and "Previewing a console from the rail" in docs/product/console-switcher.md), with its
 * console sessions and projects — or, in focus mode, one project or one console session alone. Expand/collapse state
 * is purely local UI state; the daemon has no notion of it. */
export function Sidebar({
  projects,
  selectedSessionId,
  shownConsole,
  focus,
  open,
  drawerAway = false,
  peek,
  onPeekPress,
  onColumnPhase,
  sidebarWidth,
  visibleProjectPanels,
  onSelectSession,
  onSwitchConsoleSession,
  onOpenConsoleSession,
  onOpenDialog,
  onQuickCreateSession,
  onFocus,
  onOpenArchive,
  onOpenProjectPanel,
  onOpenProjectDirectory,
  onOpenSystemTerminal,
  onOpenRepository,
  onCopyProjectName,
  onCopySessionId,
  onPullProjectBranch,
  onPushProjectBranch,
  onLoadProjectBranches,
  onSetSessionColour,
  onSetLocked,
  onSetPinned,
  onReopenSession,
  onArchiveSession,
  onOpenSettings,
}: SidebarProps): React.ReactElement {
  // Built from its members rather than gathered with a rest pattern, which makes a new object on
  // every render: every row takes it, and the React Compiler keeps this one while its members stay.
  const handlers = {
    visibleProjectPanels,
    onSelectSession,
    onSwitchConsoleSession,
    onOpenConsoleSession,
    onOpenDialog,
    onQuickCreateSession,
    onFocus,
    onOpenArchive,
    onOpenProjectPanel,
    onOpenProjectDirectory,
    onOpenSystemTerminal,
    onOpenRepository,
    onCopyProjectName,
    onCopySessionId,
    onPullProjectBranch,
    onPushProjectBranch,
    onLoadProjectBranches,
    onSetSessionColour,
    onSetLocked,
    onSetPinned,
    onReopenSession,
    onArchiveSession,
    onOpenSettings,
  } satisfies Record<keyof SidebarHandlers, unknown>;
  const inactive = useDaemonStore(useShallow((s) => {
    const { byProject } = indexSessions(s.sessions);
    return projects.map((project) => !sessionsOf(byProject, project.id).some((session) => session.status !== "archived"));
  }));
  // A project starts collapsed when it has no unarchived session, including one arriving after the
  // initial snapshot. After that its state moves only when that changes: gaining an unarchived
  // session expands it, losing its last one collapses it. Any other session update leaves a manual
  // fold as it was.
  const [folds, setFolds] = useState<Map<string, boolean>>(new Map());
  const [seenInactive, setSeenInactive] = useState<Map<string, boolean>>(new Map());
  if (projects.some((project, index) => seenInactive.get(project.id) !== inactive[index])) {
    const nextInactive = new Map(seenInactive);
    const nextFolds = new Map(folds);
    projects.forEach((project, index) => {
      if (nextInactive.get(project.id) === inactive[index]) return;
      nextInactive.set(project.id, inactive[index]!);
      nextFolds.set(project.id, inactive[index]!);
    });
    setSeenInactive(nextInactive);
    setFolds(nextFolds);
  }
  const collapsed = new Set(projects.filter((project) => folds.get(project.id)).map((project) => project.id));
  // Per console. Set when the Projects heading's fold button should keep offering Expand until
  // every project of that console is expanded again. A mixed list does not change it; only
  // collapsing or expanding all of them, or pressing the button, does.
  const [expandPinned, setExpandPinned] = useState<Map<string, boolean>>(new Map());
  // Each console's project filter (keyword and tags), held here rather than in the project list so
  // that focus mode, which replaces the list, and switching consoles both leave it in place.
  const [filters, setFilters] = useState<Map<string, ProjectFilter>>(new Map());

  // Opening or closing the last project, or a project arriving or leaving, can make a console
  // uniformly open or shut without a press of the heading's button. The pin follows that; a
  // mixed console is left as it was.
  useEffect(() => {
    setExpandPinned((prev) => reconcileExpandPins(prev, projects, collapsed) ?? prev);
  }, [projects, collapsed]);

  const toggle = (id: string) =>
    setFolds((prev) => new Map(prev).set(id, !prev.get(id)));

  // Collapse or expand a whole list of projects at once. It is given the ids rather than reading
  // the console's projects itself, so a filtered list only moves the projects it shows; the ones
  // the filter hides keep the state they had. Pressing it again once they are all there is an
  // ordinary thing to do, so unchanged state is returned as it was and the tree is left alone.
  const setCollapsedFor = (ids: string[], value: boolean) =>
    setFolds((prev) => {
      if (ids.every((id) => prev.get(id) === value)) return prev;
      const next = new Map(prev);
      for (const id of ids) next.set(id, value);
      return next;
    });

  let content: React.ReactNode;
  if (focus && "project" in focus) {
    content = (
      <ProjectFocus
        handlers={handlers}
        console={shownConsole}
        project={focus.project}
        selectedSessionId={selectedSessionId}
      />
    );
  } else if (focus) {
    content = (
      <ConsoleSessionFocus
        handlers={handlers}
        console={shownConsole}
        consoleSession={focus.consoleSession}
        projects={projects.filter((p) => p.console_id === shownConsole.id)}
        selectedSessionId={selectedSessionId}
      />
    );
  } else {
    const consoleProjects = projects.filter((p) => p.console_id === shownConsole.id);
    // Every project of the console, including any a filter is hiding: the button press changes
    // only the listed ones, but the action it offers follows the whole console.
    const foldControl = projectFoldControl(
      expandPinned.get(shownConsole.id) ?? false,
      consoleProjects.map((project) => project.id),
      collapsed,
    );
    content = (
      <>
        <ConsoleHeader handlers={handlers} current={shownConsole} />
        <FadedScroller stickyHeadingHeight={STICKY_HEADING_HEIGHT} className="min-h-0 flex-1 px-(--row-fill-inset) pb-2">
          <ConsoleBody
            key={shownConsole.id}
            handlers={handlers}
            console={shownConsole}
            projects={consoleProjects}
            selectedSessionId={selectedSessionId}
            collapsed={collapsed}
            toggle={toggle}
            setCollapsedFor={setCollapsedFor}
            foldControl={foldControl}
            onFold={() => {
              const consoleId = shownConsole.id;
              const pin = pinAfterFoldAction(foldControl);
              setExpandPinned((prev) => {
                if ((prev.get(consoleId) ?? false) === pin) return prev;
                const next = new Map(prev);
                next.set(consoleId, pin);
                return next;
              });
            }}
            filter={filters.get(shownConsole.id) ?? NO_FILTER}
            setFilter={(update) =>
              setFilters((prev) => new Map(prev).set(shownConsole.id, update(prev.get(shownConsole.id) ?? NO_FILTER)))
            }
          />
        </FadedScroller>
      </>
    );
  }

  return (
    <SidebarFrame
      shownConsoleId={shownConsole.id}
      focus={focus}
      open={open}
      drawerAway={drawerAway}
      peek={peek}
      onPeekPress={onPeekPress}
      onColumnPhase={onColumnPhase}
      sidebarWidth={sidebarWidth}
    >
      {content}
    </SidebarFrame>
  );
}

/** A project's focus mode over the project's sessions and the console's console sessions. */
function ProjectFocus({
  handlers,
  console: thisConsole,
  project,
  selectedSessionId,
}: {
  handlers: SidebarHandlers;
  console: Console;
  project: Project;
  selectedSessionId?: string;
}): React.ReactElement {
  const sessions = useProjectSessions(project.id);
  const ownerSessions = useOwnerSessions(thisConsole.id);
  // Every project row takes it, so it is kept while the console sessions stay the same.
  const owners = new Map(ownerSessions.map((s) => [s.id, s]));
  return (
    <ProjectFocusView
      handlers={handlers}
      console={thisConsole}
      project={project}
      sessions={sessions}
      owners={owners}
      selectedSessionId={selectedSessionId}
    />
  );
}

/** A console session's focus mode over every session of the console. */
function ConsoleSessionFocus({
  handlers,
  console: thisConsole,
  consoleSession,
  projects,
  selectedSessionId,
}: {
  handlers: SidebarHandlers;
  console: Console;
  consoleSession: Session;
  projects: Project[];
  selectedSessionId?: string;
}): React.ReactElement {
  const sessions = useConsoleSessions(thisConsole.id);
  return (
    <ConsoleSessionFocusView
      handlers={handlers}
      console={thisConsole}
      consoleSession={consoleSession}
      projects={projects}
      sessions={sessions}
      selectedSessionId={selectedSessionId}
    />
  );
}
