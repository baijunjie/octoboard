import {
  ChevronDown,
  ChevronRight,
  FolderPlus,
  ListChevronsDownUp,
  ListChevronsUpDown,
  MessageSquarePlus,
  SearchX,
  TriangleAlert,
} from "lucide-react";
import { setInteractionModality } from "react-aria";
import React, { memo, useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";

import { noAgentAvailable } from "../agents";
import { ActionMenu } from "../components/ActionMenu";
import { ActivityMarker } from "../components/ActivityMarker";
import { AgentIcon } from "../components/AgentIcon";
import { EmptyPanel } from "../components/EmptyPanel";
import { handFocusOff } from "../components/handFocusOff";
import { PATH_VAR_PARAMS } from "../components/literalParams";
import { RowControls, RowIconGroup, RowIconSlot } from "../components/RowControls";
import { FadedScroller } from "../components/scrollFade";
import { hasVisibleUsage, SessionUsageLabel } from "../components/SessionMetadata";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { StatusIcon } from "../components/StatusIcon";
import type { NewSessionRequest } from "../dialogs/dialogRequest";
import { withGitBadge } from "../gitStatusLabel";
import { isolate } from "../i18n/isolate";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import { drawerClass, PANE_ID, RailClip, usePanePeekPointer } from "../layout/paneOverlay";
import type { PaneWidth } from "../layout/paneWidth";
import { COLUMN_EASE_CLASS, columnSpan, isDockEasing, usePaneDockMotion, type DockPhase } from "../layout/paneDockMotion";
import type { PanePeek } from "../layout/usePaneToggles";
import { effectiveTags, matchesFilter, tagVocabulary } from "../projectFiltering";
import type { Console, Project, Session } from "../protocol";
import { activityLabelKey, sessionAgentLabel, sessionAriaLabel, sessionDisplayTitle, sessionStatusKind } from "../sessionLabel";
import { indexSessions, sessionsOf } from "../sessions/sessionIndex";
import { useDaemonStore } from "../store";
import { BindingBadge } from "./BindingBadge";
import { ConsoleSessionFocusView, ProjectFocusView } from "./FocusView";
import { GitBadge } from "./GitBadge";
import { consoleMenu, projectMenu } from "./menus";
import { NewSessionButton } from "./NewSessionButton";
import { SessionActions } from "./SessionActions";
import { archivedSessions, consoleActivity, INACTIVE_PROJECT_RANK, liveSessionRows, liveSessions, projectRank, sortProjectsByRank } from "./order";
import { type FilterUpdate, isFiltering, NO_FILTER, type ProjectFilter, ProjectFilterButton, ProjectFilterSummary } from "./ProjectFilter";
import { pinAfterFoldAction, projectFoldControl, reconcileExpandPins, type ProjectFoldControl } from "./projectFold";
import { NESTED_RAIL_CLASS, NESTED_ROW_CLASS, NestedBranch, LockButton, PinButton, RowIconButton, RowLabel, STICKY_HEADING_HEIGHT, StickySectionHeading, TreeRow } from "./rows";
import { focusTargetId } from "./focus";
import type { FocusTarget, SidebarHandlers } from "./types";
import { useFlip } from "./useFlip";
import { useArchivedOwnerSessions, useConsoleSessions, useHasBoundSessions, useOwnerSessions, useProjectSessions, useSession, useSessionProjectId } from "../sessions/useSessions";

/** The clip the docked column sits in. `min-w-0` lets the flex item reach 0 while the sidebar
 * inside keeps its full width, so the list does not reflow.
 *
 * While `easing`, the clip's own `width` transitions. A registered custom property must not drive
 * it: WebKit snaps a running transition of one to its end whenever a stylesheet is rewritten, as
 * xterm does on a refit. */
function columnClipClass(easing: boolean): string {
  return (
    "contents docked:block docked:h-full docked:min-h-0 docked:min-w-0 docked:shrink-0 docked:overflow-hidden" +
    (easing ? ` ${COLUMN_EASE_CLASS}` : "")
  );
}

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
  /** The docked-mode width and its setters; `App.tsx` owns it (`usePaneWidth`) so it can also
   * be read from outside the sidebar. */
  sidebarWidth: PaneWidth;
}

/** The sidebar: one console at a time, the current one picked on the rail, or the one a hover on the rail previews (see
 * docs/product/sidebar.md), with its console sessions and projects — or, in focus mode, one project
 * or one console session alone. Expand/collapse state is purely local UI state; the daemon has no notion of
 * it. */
export function Sidebar({
  projects,
  selectedSessionId,
  shownConsole,
  focus,
  open,
  peek,
  onPeekPress,
  onColumnPhase,
  sidebarWidth,
  visibleBrowserProjectId,
  onSelectSession,
  onSwitchConsoleSession,
  onOpenConsoleSession,
  onOpenDialog,
  onQuickCreateSession,
  onFocus,
  onOpenArchive,
  onBrowseProject,
  onOpenProjectDirectory,
  onOpenProjectTerminal,
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
  const t = useT();
  // Built from its members rather than gathered with a rest pattern, which makes a new object on
  // every render: every row takes it, and the React Compiler keeps this one while its members stay.
  const handlers = {
    visibleBrowserProjectId,
    onSelectSession,
    onSwitchConsoleSession,
    onOpenConsoleSession,
    onOpenDialog,
    onQuickCreateSession,
    onFocus,
    onOpenArchive,
    onBrowseProject,
    onOpenProjectDirectory,
    onOpenProjectTerminal,
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
  const initiallyCollapsed = useDaemonStore(useShallow((s) => {
    const { byProject } = indexSessions(s.sessions);
    return projects.map((project) => !sessionsOf(byProject, project.id).some((session) => session.status !== "archived"));
  }));
  // Seed each project once, including projects arriving after the initial snapshot. Session updates
  // must not undo a manual fold or change a project's state after it has first appeared.
  const [folds, setFolds] = useState<Map<string, boolean>>(new Map());
  if (projects.some((project) => !folds.has(project.id))) {
    const next = new Map(folds);
    projects.forEach((project, index) => {
      if (!next.has(project.id)) next.set(project.id, initiallyCollapsed[index]!);
    });
    setFolds(next);
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

  // Entering or leaving focus mode swaps the sidebar's content, which unmounts the control that
  // started it and drops a keyboard user's focus to `<body>` (no blur event fires). `holdsFocus`
  // remembers that focus was in the sidebar (React's focus events also bubble up from its menus'
  // portals), so focus can go to the new view's first control; a mouse press leaves it on the
  // terminal, which this does not touch.
  const navRef = useRef<HTMLElement>(null);
  const holdsFocus = useRef(false);
  const focusId = focus && focusTargetId(focus);
  const shownFocusId = useRef(focusId);
  const shownConsoleSessionFocus = useRef(focus !== undefined && "consoleSession" in focus);
  useEffect(() => {
    const previousId = shownFocusId.current;
    const fromConsoleSession = shownConsoleSessionFocus.current;
    shownFocusId.current = focusId;
    shownConsoleSessionFocus.current = focus !== undefined && "consoleSession" in focus;
    if (previousId === focusId) return;
    // After a short delay: a menu item that switched the view has its menu hand focus back (to the
    // trigger that just unmounted) a task later, which would otherwise land after this.
    const timer = setTimeout(() => {
      if (!holdsFocus.current || document.activeElement !== document.body) return;
      // Only where nothing else took focus: a switch that selects a session normally hands it to that
      // session's terminal. Between two console sessions' focus modes, the strip's chip that was
      // pressed has been replaced by the new current one, the nearest place to the keyboard user.
      // From a project's focus mode or from none, the first control is the way out.
      const target = focus
        ? (fromConsoleSession ? navRef.current?.querySelector<HTMLElement>("[data-switch-current]") : null) ??
          navRef.current?.querySelector<HTMLElement>("[data-focus-exit]")
        : navRef.current?.querySelector<HTMLElement>(`[data-flip="${CSS.escape(previousId ?? "")}"] [role=button]`);
      if (!target) return;
      setInteractionModality("keyboard");
      target.focus();
    }, 50);
    return () => clearTimeout(timer);
  }, [focusId]);

  // The console avatars on the rail also keep the floating card up, being outside the gap around it.
  usePanePeekPointer("sidebar", peek, "[data-sidebar-peek-keep]");
  // `peek` being passed means the docked sidebar is targeted hidden. The column eases to and from
  // that; the inset card is a later state, only once the column is out of the row.
  const dock = usePaneDockMotion(peek === undefined, peek?.active ?? false, onColumnPhase);
  const drawerClassName = dock.inset
    ? `docked:rounded-xl ${drawerClass("start", "floating", open, dock.insetShown)}${dock.preparingInset ? " docked:transition-none" : ""}`
    : `${drawerClass("start", "drawer", open)} docked:h-full`;

  // What the sidebar is showing, and how it arrived there: going down into a focus mode slides the
  // new view in from the end, coming back up slides it in from the start, and switching to another
  // console fades it in.
  const viewKey = focusId ? `focus:${focusId}` : `console:${shownConsole.id}`;
  const viewRef = useRef<HTMLDivElement>(null);
  const previousViewRef = useRef(viewKey);
  // Played in place rather than by remounting the view, so a console switch leaves what is mounted
  // in it (the header and its menu trigger among it) as it is.
  useLayoutEffect(() => {
    const previous = previousViewRef.current;
    previousViewRef.current = viewKey;
    if (previous === viewKey || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    // The card owns its entry animation; animate content changes only once it is visible.
    if (peek?.active && !dock.insetShown) return;
    // Going into or out of a focus mode slides sideways; between two views of the same level (two
    // focus modes, or two consoles) there is no level to go down or up, so the view rises in place.
    const enters = viewKey.startsWith("focus:");
    const leaves = previous.startsWith("focus:");
    const from =
      enters === leaves
        ? { transform: "translateY(6px)" }
        : enters
          ? { transform: "translateX(var(--view-shift))" }
          : { transform: "translateX(calc(-1 * var(--view-shift)))" };
    viewRef.current?.animate([{ ...from, opacity: 0 }, {}], { duration: 240, easing: "cubic-bezier(0.2, 0, 0, 1)" });
  }, [viewKey]);

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
    <RailClip floating={dock.inset}>
      {/* The clip is `display: contents` below the breakpoint and while the hover card is up, so
          the fixed pane is laid out as if this wrapper were not there. At the docked breakpoint it
          is the column's width. The sidebar inside stays at its full width, against the rail, and
          the clip uncovers it: the list does not reflow and the column does not become the card.
          The clip's width eases between the sidebar's and 0; the top bar's start segment eases
          to the same target (`App`). */}
      <div
        className={dock.inset ? "contents" : columnClipClass(isDockEasing(dock.phase))}
        style={dock.inset ? undefined : { width: columnSpan(dock.phase, sidebarWidth.width) }}
        onTransitionEnd={dock.onColumnTransitionEnd}
      >
        <nav
          // Below the `docked` breakpoint this is a fixed overlay, closed by default, slid on and off
          // with `open`; at or above it the `docked:` variants in `drawerClass` put it back exactly
          // where it always was, a plain row sibling, regardless of `open`. With the docked sidebar
          // hidden, that column is clipped away; only a hover on a console's avatar (`dock.inset`)
          // makes it the fixed card. Drawers share that card's inset from the chrome.
          // Starting below `--top-chrome-height` leaves the top bar, and
          // with it the sidebar toggle, visible while the drawer is open; ending above
          // `--bottom-chrome-height` does the same for the connection banner.
          //
          // `data-escape-scope`: one of the origins `usePaneToggles`'s capture-phase Escape listener
          // closes a drawer for. `data-pane` is how it finds this element to see whether it holds
          // focus.
          data-escape-scope
          ref={navRef}
          onFocus={(e) => {
            // A portalled menu's focus bubbles here through React and may never blur, so it counts only
            // when it came from inside the sidebar (a menu opened from the keyboard), not from the
            // terminal (one opened with the mouse).
            if (e.currentTarget.contains(e.target as Node) || e.currentTarget.contains(e.relatedTarget as Node | null)) {
              holdsFocus.current = true;
            }
          }}
          onBlur={(e) => {
            // Focus moving around inside a portalled menu blurs here too; only leaving the sidebar's own
            // DOM counts.
            if (e.currentTarget.contains(e.target as Node) && !e.currentTarget.contains(e.relatedTarget)) {
              holdsFocus.current = false;
            }
          }}
          id={PANE_ID.sidebar}
          data-pane="sidebar"
          data-region="sidebar"
          className={`flex w-70 max-docked:max-w-[calc(100vw-var(--rail-width)-2*var(--peek-gap))] flex-col overflow-hidden sidebar-fills bg-panel-sidebar shrink-0 docked:w-(--sidebar-width) ${drawerClassName}${dock.phase === "closed" && !dock.inset ? " docked:invisible" : ""}`}
          onPointerEnter={peek?.keep}
          onPointerLeave={peek?.leave}
          onPointerDownCapture={peek ? onPeekPress : undefined}
          onTransitionEnd={dock.inset ? dock.onInsetTransitionEnd : undefined}
          style={{ "--sidebar-width": `${sidebarWidth.width}px` } as React.CSSProperties}
          aria-label={t("sidebar.sessions")}
        >
          <div key={focusId ?? "console"} ref={viewRef} className="flex min-h-0 flex-1 flex-col">
            {content}
          </div>
        </nav>
      </div>
    </RailClip>
  );
}

/** The header naming the console shown, with the console's own actions beside it. Its name only:
 * the console's avatar is already marked as the current one on the rail (`Rail.tsx`), which also
 * switches consoles and carries what is going on in every console. */
function ConsoleHeader({ handlers, current }: { handlers: SidebarHandlers; current: Console }): React.ReactElement {
  const t = useT();
  const headerRef = useRef<HTMLDivElement>(null);
  const sessions = useArchivedOwnerSessions(current.id);
  return (
    // The list under this header is inset twice — the scroll area, then each heading and row — so
    // the end padding matches that and this menu lines up with the rows' menus.
    <div ref={headerRef} className="flex h-(--pane-header-height) shrink-0 items-center gap-2 groove-b px-(--pane-inset)">
      <RowLabel title={current.name}>
        <span className="text-base font-semibold">{current.name}</span>
      </RowLabel>
      <RowIconGroup>
        <NewSessionButton
          label={t("sidebar.consoleSessions.new")}
          quickLabel={t("sidebar.consoleSessions.newQuick")}
          target={() => ({ kind: "new-console-session", console: current })}
          onOpen={handlers.onOpenDialog}
          onQuickCreate={handlers.onQuickCreateSession}
          isDisabledWhenBlocked
        />
        <ActionMenu
          label={t("sidebar.console.actions", { name: isolate(current.name) })}
          items={consoleMenu(t, current, handlers, sessions)}
          contextTargetRef={headerRef}
        />
      </RowIconGroup>
    </div>
  );
}

function ConsoleBody({
  handlers,
  console: thisConsole,
  projects,
  selectedSessionId,
  collapsed,
  toggle,
  setCollapsedFor,
  foldControl,
  onFold,
  filter: stored,
  setFilter,
}: {
  handlers: SidebarHandlers;
  console: Console;
  projects: Project[];
  selectedSessionId?: string;
  collapsed: Set<string>;
  toggle: (id: string) => void;
  setCollapsedFor: (ids: string[], value: boolean) => void;
  /** The action the Projects heading's fold button offers. */
  foldControl: ProjectFoldControl;
  /** Records that press. The listed projects are collapsed or expanded here, beside it. */
  onFold: () => void;
  filter: ProjectFilter;
  setFilter: (update: FilterUpdate) => void;
}): React.ReactElement {
  const t = useT();
  const listRef = useFlip<HTMLDivElement>();
  const ranks = useProjectRanks(projects);
  // Only the project holding the selected session is told which one it is, so a selection moving
  // between two sessions renders those projects alone.
  const selectedProjectId = useSessionProjectId(selectedSessionId);
  const foldButton = useRef<HTMLSpanElement>(null);
  // The tags to pick from are the ones the console's projects carry now; a picked tag none of them
  // carries any more is dropped here, so it neither filters nor shows, yet it stays in the stored
  // selection, which every change to the filter is applied to.
  const vocabulary = tagVocabulary(projects);
  const filter = { keyword: stored.keyword, tags: effectiveTags(stored.tags, vocabulary) };
  const rankOf = new Map(projects.map((p, i) => [p.id, ranks[i]]));
  const ordered = sortProjectsByRank(
    projects.filter((p) => matchesFilter(p, filter.keyword, filter.tags)),
    rankOf,
  );
  const foldIds = ordered
    .filter((p) => foldControl === "collapse" || rankOf.get(p.id) !== INACTIVE_PROJECT_RANK)
    .map((p) => p.id);
  const noMatch = projects.length > 0 && ordered.length === 0 ? t("sidebar.filter.noMatch") : undefined;

  // Collapsing takes away what a project row holds below it (`data-sessions`), and a keyboard user
  // may be standing in there: a mouse press leaves focus where it is (`preventFocusOnPress`), so
  // focus would drop to `<body>` with no blur event and the terminal would stop receiving
  // keystrokes. It is handed to the button that did it, with its ring showing, since whoever was
  // down there got there from the keyboard. The project rows themselves stay, so focus on one of
  // them, or on its controls, is left where it is. Expanding adds rows, so it has nothing to hand off.
  const foldAll = () => {
    if (foldControl === "collapse") {
      handFocusOff(document.activeElement?.closest("[data-sessions]"), foldButton.current, true);
    }
    setCollapsedFor(foldIds, foldControl === "collapse");
    onFold();
  };

  return (
    <>
      <ConsoleSessionsSection handlers={handlers} console={thisConsole} selectedSessionId={selectedSessionId} />
      <section className="flow-root">
        <StickySectionHeading
          inline={projects.length > 0 && isFiltering(filter) && <ProjectFilterSummary filter={filter} />}
          action={
            projects.length > 0 && (
              <>
                <ProjectFilterButton
                  filter={filter}
                  vocabulary={vocabulary}
                  onChange={setFilter}
                />
                {/* Only with something listed: a filter matching nothing leaves them nothing to act
                  on, while the filter button beside them stays, as the way back. */}
                {ordered.length > 0 && (
                  <span ref={foldButton} className="flex">
                    {/* Lines of the list sit on the leading side, so the glyph mirrors under
                      right-to-left. The chevrons stay vertical: inward to collapse, outward to expand. */}
                    <RowIconButton
                      icon={foldControl === "collapse" ? ListChevronsDownUp : ListChevronsUpDown}
                      iconClassName="rtl:-scale-x-100"
                      label={t(foldControl === "collapse" ? "sidebar.collapseAll" : "sidebar.expandAll")}
                      onPress={foldAll}
                    />
                  </span>
                )}
              </>
            )
          }
        >
          {t("sidebar.projects.headingWithCount", { count: ordered.length })}
        </StickySectionHeading>
        {/* The modal filter popover hides the surrounding sidebar from assistive technology;
          React Aria keeps a mounted live announcer and its ancestors exposed. */}
        <div data-live-announcer="true">
          <StatusAnnouncer text={noMatch} />
        </div>
        {noMatch ? (
          <EmptyPanel compact silent icon={SearchX} message={noMatch} />
        ) : ordered.length === 0 ? (
        // A row, as the console sessions section's own empty state is: the full panel with the same
        // offer now stands in the terminal's area (`TerminalPane`), but only while nothing at all
        // is selected there, which a console switch leaves it short of — so the offer stays here
        // too rather than falling back to the console header's menu alone.
          <EmptyPanel
            compact
            icon={FolderPlus}
            message={t("sidebar.noProjects")}
            action={{ label: t("sidebar.console.addProject"), onPress: () => handlers.onOpenDialog({ kind: "new-project", console: thisConsole }) }}
          />
        ) : (
        // The inactive projects (no unarchived session) sit at the end, dimmed as one group; pointing at
        // any of them, keyboard focus or an open menu inside one, brings the whole group back.
        // Keyboard focus only (`data-focus-visible`, set by the rows and HeroUI's controls alike):
        // focus a mouse-opened menu hands back to its trigger would otherwise keep the group lit
        // after the pointer has gone. An open menu is matched on `aria-haspopup`, since a project
        // row is itself `aria-expanded` while open.
          <div
            ref={listRef}
            className="relative flex flex-col gap-0.5 [&:has(>[data-inactive]:hover)>[data-inactive]]:after:bg-transparent [&:has(>[data-inactive]_[data-focus-visible])>[data-inactive]]:after:bg-transparent [&:has(>[data-inactive]_[aria-haspopup][aria-expanded=true])>[data-inactive]]:after:bg-transparent"
          >
            {ordered.map((project) => (
              <div
                key={project.id}
                data-flip={project.id}
                data-inactive={rankOf.get(project.id) === INACTIVE_PROJECT_RANK || undefined}
                // Dimmed by a veil of the sidebar's own colour laid over it, not by `opacity`: WebKit
                // composites an element that fades in or out (a row's hover controls, its chevron)
                // inside a translucent ancestor wrongly, painting it as a blank tile. The veil is
                // plain paint and lets every pointer event through. It takes the dimmed names below
                // WCAG AA (about 2.9:1, muted text about 1.9:1) on purpose, by the user's decision: a
                // de-emphasis cue, lifted as soon as the group is pointed at, focused or has a menu open.
                className="relative after:pointer-events-none after:absolute after:inset-0 after:transition-colors after:duration-200 data-inactive:after:bg-panel-sidebar/55"
              >
                <ProjectNode
                  handlers={handlers}
                  project={project}
                  parentConsole={thisConsole}
                  selectedSessionId={project.id === selectedProjectId ? selectedSessionId : undefined}
                  isCollapsed={collapsed.has(project.id)}
                  onToggle={toggle}
                />
              </div>
            ))}
          </div>
        )}
      </section>
    </>
  );
}

/** The console's console sessions, listed above the project list: every one that is not archived,
 * ordered the way a project's own sessions are (`liveSessions`).
 * The archived ones are reached from the console's own menu; see "The console
 * sessions section and the project list" in `docs/product/sidebar.md`. */
function ConsoleSessionsSection({
  handlers,
  console: thisConsole,
  selectedSessionId,
}: {
  handlers: SidebarHandlers;
  console: Console;
  selectedSessionId?: string;
}): React.ReactElement {
  const t = useT();
  const listRef = useFlip<HTMLDivElement>();
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const consoleSessions = useOwnerSessions(thisConsole.id);
  // With no agent on the machine at all, starting a console session would only be refused by the
  // daemon the moment it tried — so the action is disabled here instead, and `agents.noneFound` is
  // shown. It is rendered whenever the console is blocked, not only when `live` is empty, because a
  // blocked console can still hold console sessions left over from before: one that was
  // interrupted (an agent removed, or a `PATH` change) stays live and keeps `liveSessions` from
  // being empty.
  const blocked = noAgentAvailable(agentAvailability);
  const live = liveSessions(consoleSessions);
  const openNew = () => handlers.onOpenConsoleSession(thisConsole);

  return (
    <section className="flow-root">
      <StickySectionHeading>
        {t("sidebar.consoleSessions.headingWithCount", { count: live.length })}
      </StickySectionHeading>
      {/* A warning rather than the glyph the rows below carry: this row is the one in the section
          that offers nothing to press, and the console header's `+` is disabled. */}
      {blocked && <EmptyPanel compact icon={TriangleAlert} message={<Message id="agents.noneFound" params={PATH_VAR_PARAMS} />} />}
      {live.length === 0 && !blocked && (
        <EmptyPanel
          compact
          icon={MessageSquarePlus}
          message={t("sidebar.consoleSessions.empty")}
          action={{ label: t("sidebar.consoleSessions.new"), onPress: openNew }}
        />
      )}
      {live.length > 0 && (
        <div ref={listRef} className="relative flex flex-col gap-0.5">
          {live.map((session) => (
            <div key={session.id} data-flip={session.id}>
              <SessionRow handlers={handlers} session={session} selected={session.id === selectedSessionId} />
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

/** A project's rank (`projectRank`) for each of `projects`, in their order. Read from the store as
 * numbers, so an event that leaves every project's rank as it was does not render the list. */
function useProjectRanks(projects: Project[]): number[] {
  return useDaemonStore(
    useShallow((s) => {
      const { byProject } = indexSessions(s.sessions);
      return projects.map((p) => projectRank(sessionsOf(byProject, p.id)));
    }),
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

/** A project and its sessions. Memoised: it renders again only for its own project's change, its
 * sessions' (read from the store here, not handed down), the selection reaching or leaving it, or
 * a handler changing. */
const ProjectNode = memo(function ProjectNode({
  handlers,
  project,
  parentConsole,
  selectedSessionId,
  isCollapsed,
  onToggle,
}: {
  handlers: SidebarHandlers;
  project: Project;
  parentConsole: Console;
  selectedSessionId?: string;
  isCollapsed: boolean;
  onToggle: (projectId: string) => void;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const listRef = useFlip<HTMLDivElement>();
  // Read directly rather than taking it as a prop passed down from `App.tsx`: a status change
  // then only re-renders the one row it is about, instead of the whole tree (the selector returns
  // the same `GitStatus` reference until this project's own entry changes, so no `useShallow` is
  // needed).
  const gitStatus = useDaemonStore((s) => s.gitStatuses.get(project.id));
  // The same array until a session of this project changes, so an event about another project's
  // session leaves this row, its menu and its rows alone.
  const sessions = useProjectSessions(project.id);
  const rowRef = useRef<HTMLDivElement>(null);
  const rows = liveSessionRows(sessions);
  const lastRootIndex = rows.reduce((last, row, index) => row.under ? last : index, -1);
  const archived = archivedSessions(sessions);
  const activity = consoleActivity(sessions);
  const newSession = (): NewSessionRequest => ({ kind: "new-session", console: parentConsole, project, binding: { kind: "choose" } });
  const activityKey = activityLabelKey(activity, project.pinned);
  const nameLabel = activityKey
    ? t(activityKey, { name: isolate(project.name) })
    : project.pinned
      ? t("sidebar.project.ariaLabelPinned", { name: isolate(project.name) })
      : project.name;

  return (
    <div>
      <TreeRow
        ref={rowRef}
        ariaLabel={withGitBadge(language, t, nameLabel, gitStatus)}
        onActivate={() => onToggle(project.id)}
        expanded={!isCollapsed}
      >
        {/* `min-w-16` is the floor itself: it has to sit on this flex item (`flex-1`'s basis is
            0%, so it never shrinks "from" anything the floor could clamp if placed on a child
            instead), leaving `GitBadge`'s own `min-w-0` as the one that keeps giving way once
            this is reached. */}
        <div className="flex min-w-16 flex-1 items-center">
          <RowLabel title={project.name} className="min-w-0 shrink">
            <span className="font-medium">{project.name}</span>
          </RowLabel>
          {/* After the name rather than before it: while expanded it shows only on hover, which a
              leading chevron could not do without the names jumping sideways. Two glyphs rather than
              one rotated: a rotated chevron that is already mirrored would point the wrong way under
              right-to-left. While hidden it takes no width and no margin, like `RowControls`, so
              the name gets the room. */}
          {isCollapsed ? (
            <ChevronRight aria-hidden="true" className="ms-2 size-3.5 shrink-0 text-app-muted rtl:-scale-x-100" />
          ) : (
            <ChevronDown aria-hidden="true" className="h-3.5 w-0 shrink-0 overflow-hidden text-app-muted opacity-0 transition-[opacity,width,margin] duration-150 motion-reduce:transition-none group-hover:ms-2 group-hover:w-3.5 group-hover:opacity-100 group-data-focus-visible:ms-2 group-data-focus-visible:w-3.5 group-data-focus-visible:opacity-100" />
          )}
        </div>
        <GitBadge status={gitStatus} decorative />
        {isCollapsed && <ActivityMarker activity={activity} />}
        <RowControls fixed={project.pinned ? <PinButton name={project.name} onUnpin={() => handlers.onSetPinned({ project }, false)} returnFocusTo={rowRef} /> : undefined}>
          <NewSessionButton label={t("sidebar.project.openSession")} quickLabel={t("sidebar.project.openSessionQuick")} target={newSession} onOpen={handlers.onOpenDialog} onQuickCreate={handlers.onQuickCreateSession} />
          <ActionMenu label={t("sidebar.project.actions", { name: isolate(project.name) })} items={projectMenu(t, handlers, project, archived, sessions, { gitRepository: gitStatus?.repository })} contextTargetRef={rowRef} />
        </RowControls>
      </TreeRow>
      {!isCollapsed && (
        // Marked so collapsing every project at once can tell whether keyboard focus is standing in
        // a part about to go away (see `foldAll`).
        <div data-sessions className={`${NESTED_RAIL_CLASS} mt-0.5`}>
          {rows.length === 0 ? (
            <EmptyPanel
              compact
              icon={MessageSquarePlus}
              message={t("sidebar.noSessions")}
              action={{ label: t("sidebar.project.openSession"), onPress: () => handlers.onOpenDialog(newSession()) }}
            />
          ) : (
            <div ref={listRef} className="relative flex flex-col gap-0.5">
              {rows.map(({ session, under }, index) => (
                // One flat list, a session bound to a project session only inset
                // (`NESTED_ROW_CLASS`), rather than a list nested inside the owner's row: the
                // reorder animation reads the list's direct children (`useFlip`), so a third level
                // of its own would leave a team unanimated while its statuses change.
                <div key={session.id} data-flip={session.id} className={under ? NESTED_ROW_CLASS : "relative"}>
                  <NestedBranch last={index >= lastRootIndex} ancestor={!!under} />
                  <SessionRow
                    handlers={handlers}
                    session={session}
                    under={under}
                    selected={session.id === selectedSessionId}
                  />
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
});

/** A session row: a project session's own, or one of the console's console sessions
 * (`ConsoleSessionsSection`), which share the row and its menu — pin, rename, archive, no resume —
 * since both are ordinary sessions once the row stops being a console session's sole, special one.
 * The session a bound one reports to is its `under` when that is the project session listed right
 * above it, otherwise the console session it is bound to, read from the store here, so that a change
 * of another session never renders this row: a console session is drawn as the binding badge in its
 * colour, while a project session — a lead session, or any project session that started this
 * one — is not, since the row is already listed under it; either way the row's own accessible name
 * names it, which is what carries the nesting to assistive technology. Memoised, with `selected` a
 * flag rather than the selected id, so a change of selection renders only the two rows it moves
 * between. */
const SessionRow = memo(function SessionRow({
  handlers,
  session,
  under,
  selected,
}: {
  handlers: SidebarHandlers;
  session: Session;
  under?: Session;
  selected: boolean;
}): React.ReactElement {
  const metadataId = useId();
  const t = useT();
  const language = useCurrentLanguage();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const hasBound = useHasBoundSessions(session.id);
  const bound = useSession(session.bound_to);
  const owner = under ?? (bound?.role === "console" ? bound : undefined);
  const rowRef = useRef<HTMLDivElement>(null);
  const badgeOwner = session.role === "console" ? session : owner?.role === "console" ? owner : undefined;
  return (
    <TreeRow
      ref={rowRef}
      describedBy={hasVisibleUsage(session.usage) ? metadataId : undefined}
      ariaLabel={sessionAriaLabel(t, language, session, accounts, owner)}
      selected={selected}
      locked={session.locked}
      onActivate={() => handlers.onSelectSession(session)}
    >
      <StatusIcon status={sessionStatusKind(session)} decorative withTooltip />
      <AgentIcon agent={session.agent} tooltip={sessionAgentLabel(t, session, accounts)} />
      <div className="flex min-w-0 flex-1 items-center gap-2">
        <RowLabel title={sessionDisplayTitle(t, session)} className="min-w-0">{sessionDisplayTitle(t, session)}</RowLabel>
        {hasVisibleUsage(session.usage) && <span id={metadataId} className="shrink-0"><SessionUsageLabel usage={session.usage} /></span>}
      </div>
      <RowControls
        fixed={badgeOwner?.colour || session.pinned || session.locked ? (
          <>
            {/* A console session shows its own colour; a bound session shows its owner's. */}
            {badgeOwner?.colour && <RowIconSlot><BindingBadge owner={badgeOwner} decorative={session.role === "console"} /></RowIconSlot>}
            {session.locked && <LockButton name={sessionDisplayTitle(t, session)} onUnlock={() => handlers.onSetLocked(session, false)} returnFocusTo={rowRef} />}
            {session.pinned && <PinButton name={sessionDisplayTitle(t, session)} onUnpin={() => handlers.onSetPinned({ session }, false)} returnFocusTo={rowRef} />}
          </>
        ) : undefined}
      >
        <SessionActions handlers={handlers} session={session} accounts={accounts} hasBoundSessions={hasBound} rowRef={rowRef} />
      </RowControls>
    </TreeRow>
  );
});
