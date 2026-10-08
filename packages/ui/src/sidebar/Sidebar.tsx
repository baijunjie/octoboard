import { ScrollShadow } from "@heroui/react";
import {
  ChevronDown,
  ChevronRight,
  ChevronsUpDown,
  FolderOpen,
  FolderPlus,
  LayoutDashboard,
  ListChevronsDownUp,
  ListChevronsUpDown,
  MessageSquarePlus,
  Pencil,
  Pin,
  Plus,
  SearchX,
  Trash2,
} from "lucide-react";
import { setInteractionModality } from "react-aria";
import React, { useEffect, useLayoutEffect, useRef, useState } from "react";

import { noAgentAvailable } from "../agents";
import { ActionMenu, type ActionMenuEntry } from "../components/ActionMenu";
import { AgentIcon } from "../components/AgentIcon";
import { ConsoleAvatar } from "../components/ConsoleAvatar";
import { EmptyPanel } from "../components/EmptyPanel";
import { handFocusOff } from "../components/handFocusOff";
import { ActivityMarker, StatusIcon } from "../components/StatusIcon";
import { withGitBadge } from "../gitStatusLabel";
import { useCurrentLanguage, useT } from "../i18n/react";
import { drawerClass, PANE_ID, PeekHotZone } from "../layout/paneOverlay";
import type { PaneWidth } from "../layout/paneWidth";
import type { PanePeek } from "../layout/usePaneToggles";
import { effectiveTags, matchesFilter, tagVocabulary, withoutTags } from "../projectFiltering";
import type { Console, Project, Session } from "../protocol";
import { sessionAriaLabel } from "../sessionLabel";
import { useDaemonStore } from "../store";
import { BindingBadge } from "./BindingBadge";
import { ConsoleSessionFocusView, ProjectFocusView } from "./FocusView";
import { GitBadge } from "./GitBadge";
import { type FilterUpdate, NO_FILTER, type ProjectFilter, ProjectFilterButton, ProjectFilterTag, ProjectFilterTags } from "./ProjectFilter";
import { archiveSubmenu, projectMenu, sessionMenu } from "./menus";
import { archivedSessions, consoleActivity, isInactiveProject, liveSessions, sortProjects, type Activity } from "./order";
import { pinAfterFoldAction, projectFoldControl, reconcileExpandPins, type ProjectFoldControl } from "./projectFold";
import { RowControls, RowIconButton, RowLabel, SectionHeading, TreeRow } from "./rows";
import { focusTargetId } from "./sidebarView";
import type { FocusTarget, SidebarHandlers } from "./types";
import { useFlip } from "./useFlip";

interface SidebarProps extends SidebarHandlers {
  consoles: Console[];
  projects: Project[];
  sessions: Session[];
  selectedSessionId?: string;
  /** The console the sidebar shows; the caller resolves it to an existing one. */
  currentConsole?: Console;
  /** The project or console session in focus mode, if any; it belongs to `currentConsole`. */
  focus?: FocusTarget;
  /** Whether the drawer is open below the `docked` breakpoint; above it the docked sidebar is
   * shown or, with `peek`, hidden (`usePaneToggles` owns the state, resetting it once the window no
   * longer needs it). */
  open: boolean;
  /** The hover reveal of the sidebar while the user has hidden it from the top bar: the same panel,
   * kept as a fixed overlay at the docked width that floats in over the terminal, with its shadow
   * and rounded edge. `undefined` while the docked sidebar is shown. */
  peek?: PanePeek;
  /** The docked-mode width and its setters; `App.tsx` owns it (`usePaneWidth`) so it can also
   * be read from outside the sidebar. */
  sidebarWidth: PaneWidth;
}

/** The sidebar: one console at a time, picked from the switcher at its top (see
 * docs/product/sidebar.md), with its console sessions and projects — or, in focus mode, one project
 * or one console session alone. Expand/collapse state is purely local UI state; the daemon has no notion of
 * it. */
export function Sidebar({
  consoles,
  projects,
  sessions,
  selectedSessionId,
  currentConsole,
  focus,
  open,
  peek,
  sidebarWidth,
  ...handlers
}: SidebarProps): React.ReactElement {
  const t = useT();
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
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
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  // Collapse or expand a whole list of projects at once. It is given the ids rather than reading
  // the console's projects itself, so a filtered list only moves the projects it shows; the ones
  // the filter hides keep the state they had. Pressing it again once they are all there is an
  // ordinary thing to do, so an unchanged set is returned as it was and the tree is left alone.
  const setCollapsedFor = (ids: string[], value: boolean) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      for (const id of ids) {
        if (value) next.add(id);
        else next.delete(id);
      }
      return next.size === prev.size ? prev : next;
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
  useEffect(() => {
    const previousId = shownFocusId.current;
    shownFocusId.current = focusId;
    if (previousId === focusId) return;
    // After a short delay: a menu item that switched the view has its menu hand focus back (to the
    // trigger that just unmounted) a task later, which would otherwise land after this.
    const timer = setTimeout(() => {
      if (!holdsFocus.current || document.activeElement !== document.body) return;
      const target = focus
        ? navRef.current?.querySelector<HTMLElement>("[data-focus-exit]")
        : navRef.current?.querySelector<HTMLElement>(`[data-flip="${CSS.escape(previousId ?? "")}"] [role=button]`);
      if (!target) return;
      setInteractionModality("keyboard");
      target.focus();
    }, 50);
    return () => clearTimeout(timer);
  }, [focusId]);

  const drawerClassName = peek
    ? `docked:rounded-e-xl ${drawerClass("start", "floating", open, peek.active)}`
    : drawerClass("start", "drawer", open);

  const consoleSessions = currentConsole ? sessions.filter((s) => s.console_id === currentConsole.id) : [];
  // Every project session's binding badge names the console session it is bound to (`bound_to`) by
  // looking it up here, once, rather than each row searching the console's sessions itself.
  const owners = new Map(consoleSessions.filter((s) => s.role === "console").map((s) => [s.id, s]));

  // What the sidebar is showing, and how it arrived there: going down into a focus mode slides the
  // new view in from the end, coming back up slides it in from the start, and switching to another
  // console fades it in.
  const viewKey = !currentConsole ? "none" : focusId ? `focus:${focusId}` : `console:${currentConsole.id}`;
  const viewRef = useRef<HTMLDivElement>(null);
  const previousViewRef = useRef(viewKey);
  // Played in place rather than by remounting the view: a remount on a console switch would
  // destroy the switcher's trigger that the menu hands keyboard focus back to.
  useLayoutEffect(() => {
    const previous = previousViewRef.current;
    previousViewRef.current = viewKey;
    if (previous === viewKey || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    // Between two focus modes (following a chip) there is no level to go down or up.
    const from =
      viewKey.startsWith("focus:") && previous.startsWith("focus:")
        ? { transform: "translateY(6px)" }
        : viewKey.startsWith("focus:")
          ? { transform: "translateX(var(--view-shift))" }
          : previous.startsWith("focus:")
            ? { transform: "translateX(calc(-1 * var(--view-shift)))" }
            : previous.startsWith("console:")
              ? { transform: "translateY(6px)" }
              : undefined;
    if (from) viewRef.current?.animate([{ ...from, opacity: 0 }, {}], { duration: 240, easing: "cubic-bezier(0.2, 0, 0, 1)" });
  }, [viewKey]);

  let content: React.ReactNode;
  if (!currentConsole) {
    content = (
      <EmptyPanel
        icon={LayoutDashboard}
        message={t("sidebar.noConsoles")}
        action={{ label: t("sidebar.newConsole"), icon: Plus, onPress: () => handlers.onOpenDialog({ kind: "new-console" }) }}
      />
    );
  } else if (focus && "project" in focus) {
    content = (
      <ProjectFocusView
        handlers={handlers}
        console={currentConsole}
        project={focus.project}
        sessions={consoleSessions.filter((s) => s.project_id === focus.project.id)}
        owners={owners}
        selectedSessionId={selectedSessionId}
      />
    );
  } else if (focus) {
    content = (
      <ConsoleSessionFocusView
        handlers={handlers}
        console={currentConsole}
        consoleSession={focus.consoleSession}
        projects={projects.filter((p) => p.console_id === currentConsole.id)}
        sessions={consoleSessions}
        selectedSessionId={selectedSessionId}
      />
    );
  } else {
    const consoleProjects = projects.filter((p) => p.console_id === currentConsole.id);
    // Every project of the console, including any a filter is hiding: the button press changes
    // only the listed ones, but the action it offers follows the whole console.
    const foldControl = projectFoldControl(
      expandPinned.get(currentConsole.id) ?? false,
      consoleProjects.map((project) => project.id),
      collapsed,
    );
    content = (
      <>
        <ConsoleSwitcher handlers={handlers} consoles={consoles} sessions={sessions} current={currentConsole} />
        <ScrollShadow size={24} className="min-h-0 flex-1 px-2 pb-2">
          <ConsoleBody
            handlers={handlers}
            console={currentConsole}
            projects={consoleProjects}
            sessions={consoleSessions}
            owners={owners}
            selectedSessionId={selectedSessionId}
            collapsed={collapsed}
            toggle={toggle}
            setCollapsedFor={setCollapsedFor}
            foldControl={foldControl}
            onFold={() => {
              const consoleId = currentConsole.id;
              const pin = pinAfterFoldAction(foldControl);
              setExpandPinned((prev) => {
                if ((prev.get(consoleId) ?? false) === pin) return prev;
                const next = new Map(prev);
                next.set(consoleId, pin);
                return next;
              });
            }}
            filter={filters.get(currentConsole.id) ?? NO_FILTER}
            setFilter={(update) =>
              setFilters((prev) => new Map(prev).set(currentConsole.id, update(prev.get(currentConsole.id) ?? NO_FILTER)))
            }
          />
        </ScrollShadow>
      </>
    );
  }

  return (
    <>
      <nav
        // Below the `docked` breakpoint this is a fixed overlay, closed by default, slid on and off
        // with `open`; at or above it the `docked:` variants in `drawerClass` put it back exactly
        // where it always was, a plain row sibling, regardless of `open`. With the sidebar hidden
        // from the top bar's toggle, `drawerClass` instead keeps it a fixed overlay at the docked
        // width that floats in while `peek` is active. Starting below `--top-chrome-height` leaves
        // the top bar, and with it the sidebar toggle, visible while this is open; ending above
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
        className={`flex w-70 flex-col overflow-hidden border-e border-separator bg-surface shrink-0 docked:w-(--sidebar-width) ${drawerClassName}`}
        onPointerEnter={peek?.keep}
        onPointerLeave={peek?.leave}
        style={{ "--sidebar-width": `${sidebarWidth.width}px` } as React.CSSProperties}
        aria-label={t("sidebar.sessions")}
      >
        <div key={focusId ?? "console"} ref={viewRef} className="flex min-h-0 flex-1 flex-col">
          {content}
        </div>
      </nav>
      {peek && <PeekHotZone side="start" peek={peek} />}
    </>
  );
}

function activityLabelKey(activity: Activity, pinned = false) {
  switch (activity) {
    case "waiting":
      return pinned ? ("sidebar.activity.waitingPinned" as const) : ("sidebar.activity.waiting" as const);
    case "working":
      return pinned ? ("sidebar.activity.workingPinned" as const) : ("sidebar.activity.working" as const);
    case "running":
      return pinned ? ("sidebar.activity.runningPinned" as const) : ("sidebar.activity.running" as const);
    default:
      return undefined;
  }
}

/** The header naming the console shown, which switches to another one; every console in its list
 * carries what is going on in it, so activity elsewhere is visible without switching. Beside it,
 * the console's own actions. */
function ConsoleSwitcher({
  handlers,
  consoles,
  sessions,
  current,
}: {
  handlers: SidebarHandlers;
  consoles: Console[];
  sessions: Session[];
  current: Console;
}): React.ReactElement {
  const t = useT();
  const activities = new Map(consoles.map((c) => [c.id, consoleActivity(sessions.filter((s) => s.console_id === c.id))]));
  // What the other consoles are doing shows on the switcher itself, so a raised hand elsewhere is
  // not hidden behind it.
  const elsewhere = consoles.some((c) => c.id !== current.id && activities.get(c.id) === "waiting")
    ? "waiting"
    : undefined;

  const items: ActionMenuEntry[] = [
    ...consoles.map((c) => {
      const activityKey = activityLabelKey(activities.get(c.id));
      return {
        label: c.name,
        ariaLabel: activityKey ? t(activityKey, { name: c.name }) : undefined,
        icon: <ConsoleAvatar icon={c.icon} className="size-5" />,
        end: <ActivityMarker activity={activities.get(c.id)} />,
        selected: c.id === current.id,
        onClick: () => handlers.onSelectConsole(c.id),
      };
    }),
    "separator",
    { label: t("sidebar.newConsole"), icon: Plus, onClick: () => handlers.onOpenDialog({ kind: "new-console" }) },
  ];

  const headerRef = useRef<HTMLDivElement>(null);
  const consoleActions: ActionMenuEntry[] = [
    { label: t("sidebar.console.addProject"), icon: FolderPlus, onClick: () => handlers.onOpenDialog({ kind: "new-project", console: current }) },
    { label: t("sidebar.console.edit"), icon: Pencil, onClick: () => handlers.onOpenDialog({ kind: "edit-console", console: current }) },
    "separator",
    { label: t("sidebar.console.delete"), icon: Trash2, onClick: () => handlers.onOpenDialog({ kind: "delete-console", console: current }), destructive: true },
  ];

  return (
    // The list under this header is inset twice — the scroll area, then each heading and row — so
    // the end padding matches that and this menu lines up with the rows' menus. The chevron is a
    // bare glyph: the trigger keeps only the inset a size-6 icon button puts around its own glyph,
    // and the gap is the one those buttons use, so the chevron sits over the button beside a menu.
    <div ref={headerRef} className="flex h-14 shrink-0 items-center gap-0.5 border-b border-separator ps-2 pe-4">
      <ActionMenu
        className="min-w-0 flex-1"
        label={t(elsewhere ? "sidebar.console.switchWaiting" : "sidebar.console.switch", { name: current.name })}
        tooltip={false}
        items={items}
        triggerClassName="flex h-9 w-full min-w-0 items-center gap-2 rounded-lg ps-2 pe-1 text-start text-sm hover:bg-default aria-expanded:bg-default"
        trigger={
          <>
            <ConsoleAvatar icon={current.icon} />
            <RowLabel title={current.name}>
              <span className="font-semibold">{current.name}</span>
            </RowLabel>
            <ActivityMarker activity={elsewhere} />
            <ChevronsUpDown aria-hidden="true" className="size-4 shrink-0 text-muted" />
          </>
        }
      />
      <ActionMenu label={t("sidebar.console.actions", { name: current.name })} items={consoleActions} contextTargetRef={headerRef} />
    </div>
  );
}

function ConsoleBody({
  handlers,
  console: thisConsole,
  projects,
  sessions,
  owners,
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
  sessions: Session[];
  /** Every console session of this console, by id — a project session's binding badge is looked up
   * here by its `bound_to`. */
  owners: Map<string, Session>;
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
  const filterButton = useRef<HTMLDivElement>(null);
  const foldButton = useRef<HTMLSpanElement>(null);
  const sessionsOf = (project: Project) => sessions.filter((s) => s.project_id === project.id);
  // The tags to pick from are the ones the console's projects carry now; a picked tag none of them
  // carries any more is dropped here, so it neither filters nor shows, yet it stays in the stored
  // selection, which every change to the filter is applied to.
  const vocabulary = tagVocabulary(projects);
  const filter = { keyword: stored.keyword, tags: effectiveTags(stored.tags, vocabulary) };
  const ordered = sortProjects(
    projects.filter((p) => matchesFilter(p, filter.keyword, filter.tags)),
    sessionsOf,
  );
  const orderedIds = ordered.map((p) => p.id);

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
    setCollapsedFor(orderedIds, foldControl === "collapse");
    onFold();
  };

  return (
    <>
      <ConsoleSessionsSection handlers={handlers} console={thisConsole} sessions={sessions} selectedSessionId={selectedSessionId} />
      <SectionHeading
        after={
          // Only while the filter button is there (below): the chips hand focus to it when removed.
          projects.length > 0 && (
            <>
              {filter.keyword.trim() !== "" && (
                <ProjectFilterTag
                  keyword={filter.keyword.trim()}
                  returnFocusTo={filterButton}
                  onRemove={() => setFilter((f) => ({ ...f, keyword: "" }))}
                />
              )}
              {filter.tags.length > 0 && (
                <ProjectFilterTags
                  tags={filter.tags}
                  returnFocusTo={filterButton}
                  onRemove={(removed) => setFilter((f) => ({ ...f, tags: withoutTags(f.tags, removed) }))}
                />
              )}
            </>
          )
        }
        action={
          projects.length > 0 && (
            <>
              <ProjectFilterButton
                filter={filter}
                vocabulary={vocabulary}
                onChange={setFilter}
                holderRef={filterButton}
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
        {t("sidebar.projects")}
      </SectionHeading>
      {projects.length > 0 && ordered.length === 0 ? (
        <EmptyPanel compact icon={SearchX} message={t("sidebar.filter.noMatch")} />
      ) : ordered.length === 0 ? (
        <EmptyPanel
          icon={FolderOpen}
          message={t("sidebar.noProjects")}
          action={{ label: t("sidebar.console.addProject"), icon: FolderPlus, onPress: () => handlers.onOpenDialog({ kind: "new-project", console: thisConsole }) }}
        />
      ) : (
        // The inactive projects (no running session) sit at the end, dimmed as one group; pointing at
        // any of them, keyboard focus or an open menu inside one, brings the whole group back.
        // Keyboard focus only (`:focus-visible`): focus a mouse-opened menu hands back to its
        // trigger would otherwise keep the group lit after the pointer has gone. An open menu is
        // matched on `aria-haspopup`, since a project row is itself `aria-expanded` while open.
        <div
          ref={listRef}
          className="relative flex flex-col gap-0.5 [&:has(>[data-inactive]:hover)>[data-inactive]]:after:bg-transparent [&:has(>[data-inactive]_:focus-visible)>[data-inactive]]:after:bg-transparent [&:has(>[data-inactive]_[aria-haspopup][aria-expanded=true])>[data-inactive]]:after:bg-transparent"
        >
          {ordered.map((project) => (
            <div
              key={project.id}
              data-flip={project.id}
              data-inactive={isInactiveProject(sessionsOf(project)) || undefined}
              // Dimmed by a veil of the sidebar's own colour laid over it, not by `opacity`: WebKit
              // composites an element that fades in or out (a row's hover controls, its chevron)
              // inside a translucent ancestor wrongly, painting it as a blank tile. The veil is
              // plain paint and lets every pointer event through. It takes the dimmed names below
              // WCAG AA (about 2.9:1, muted text about 1.9:1) on purpose, by the user's decision: a
              // de-emphasis cue, lifted as soon as the group is pointed at, focused or has a menu open.
              className="relative after:pointer-events-none after:absolute after:inset-0 after:transition-colors after:duration-200 data-inactive:after:bg-surface/55"
            >
              <ProjectNode
                handlers={handlers}
                project={project}
                parentConsole={thisConsole}
                sessions={sessionsOf(project)}
                owners={owners}
                selectedSessionId={selectedSessionId}
                isCollapsed={collapsed.has(project.id)}
                onToggle={() => toggle(project.id)}
              />
            </div>
          ))}
        </div>
      )}
    </>
  );
}

/** The console's console sessions, listed above the project list: every one that is not archived,
 * ordered the way a project's own sessions are (`liveSessions`), with an action to start a new one
 * and, reached from the section rather than from any one row, the console's archived console
 * sessions (see "The console sessions section and the project list" in `docs/product/sidebar.md`). */
function ConsoleSessionsSection({
  handlers,
  console: thisConsole,
  sessions,
  selectedSessionId,
}: {
  handlers: SidebarHandlers;
  console: Console;
  sessions: Session[];
  selectedSessionId?: string;
}): React.ReactElement {
  const t = useT();
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  // With no agent on the machine at all, starting a console session would only be refused by the
  // daemon the moment it tried — so the action is disabled here instead, and the install prompt
  // is shown. The prompt stands on its own rather than inside the empty panel, because a console
  // can be blocked while still holding console sessions left over from before: one that was
  // interrupted (an agent removed, or a `PATH` change) stays live and keeps `liveSessions` from
  // being empty.
  const blocked = noAgentAvailable(agentAvailability);
  const consoleSessions = sessions.filter((s) => s.role === "console");
  const live = liveSessions(consoleSessions);
  const archivedConsoleSessions = archivedSessions(consoleSessions);
  const openNew = () => handlers.onOpenConsoleSession(thisConsole);
  const listRef = useFlip<HTMLDivElement>();

  const menu: ActionMenuEntry[] = [
    archiveSubmenu(t, t("sidebar.archive.consoleSessions"), archivedConsoleSessions, handlers.onSelectSession, () =>
      handlers.onOpenArchive({ console: thisConsole.id }),
    ),
  ];

  return (
    <>
      <SectionHeading
        action={
          <>
            <RowIconButton icon={Plus} label={t("sidebar.consoleSessions.new")} onPress={openNew} isDisabled={blocked} />
            {archivedConsoleSessions.length > 0 && (
              <ActionMenu label={t("sidebar.consoleSessions.actions", { name: thisConsole.name })} items={menu} />
            )}
          </>
        }
      >
        {t("sidebar.consoleSessions.heading")}
      </SectionHeading>
      {blocked && <EmptyPanel compact icon={MessageSquarePlus} message={t("agents.installPrompt")} />}
      {live.length === 0 && !blocked && <EmptyPanel compact icon={MessageSquarePlus} message={t("sidebar.consoleSessions.empty")} />}
      {live.length > 0 && (
        <div ref={listRef} className="relative flex flex-col gap-0.5">
          {live.map((session) => (
            <div key={session.id} data-flip={session.id}>
              <SessionRow handlers={handlers} session={session} selectedSessionId={selectedSessionId} />
            </div>
          ))}
        </div>
      )}
    </>
  );
}

function ProjectNode({
  handlers,
  project,
  parentConsole,
  sessions,
  owners,
  selectedSessionId,
  isCollapsed,
  onToggle,
}: {
  handlers: SidebarHandlers;
  project: Project;
  parentConsole: Console;
  sessions: Session[];
  owners: Map<string, Session>;
  selectedSessionId?: string;
  isCollapsed: boolean;
  onToggle: () => void;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  // Read directly rather than taking it as a prop passed down from `App.tsx`: a status change
  // then only re-renders the one row it is about, instead of the whole tree (the selector returns
  // the same `GitStatus` reference until this project's own entry changes, so no `useShallow` is
  // needed).
  const gitStatus = useDaemonStore((s) => s.gitStatuses.get(project.id));
  const live = liveSessions(sessions);
  const archived = archivedSessions(sessions);
  const activity = consoleActivity(live);
  const listRef = useFlip<HTMLDivElement>();
  const rowRef = useRef<HTMLDivElement>(null);
  const openSession = () =>
    handlers.onOpenDialog({ kind: "new-session", console: parentConsole, project, binding: { kind: "choose" } });
  const activityKey = activityLabelKey(activity, project.pinned);
  const nameLabel = activityKey
    ? t(activityKey, { name: project.name })
    : project.pinned
      ? t("sidebar.project.ariaLabelPinned", { name: project.name })
      : project.name;

  return (
    <div>
      <TreeRow
        ref={rowRef}
        ariaLabel={withGitBadge(language, t, nameLabel, gitStatus)}
        onActivate={onToggle}
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
          {project.pinned && <Pin aria-hidden="true" className="ms-2 size-3 shrink-0 text-muted" />}
          {/* After the name rather than before it: while expanded it shows only on hover, which a
              leading chevron could not do without the names jumping sideways. Two glyphs rather than
              one rotated: a rotated chevron that is already mirrored would point the wrong way under
              right-to-left. While hidden it takes no width and no margin, like `RowControls`, so
              the name gets the room. */}
          {isCollapsed ? (
            <ChevronRight aria-hidden="true" className="ms-2 size-3.5 shrink-0 text-muted rtl:-scale-x-100" />
          ) : (
            <ChevronDown aria-hidden="true" className="h-3.5 w-0 shrink-0 overflow-hidden text-muted opacity-0 transition-[opacity,width,margin] duration-150 motion-reduce:transition-none group-hover:ms-2 group-hover:w-3.5 group-hover:opacity-100 group-focus-visible:ms-2 group-focus-visible:w-3.5 group-focus-visible:opacity-100" />
          )}
        </div>
        <GitBadge status={gitStatus} decorative />
        {isCollapsed && <ActivityMarker activity={activity} />}
        <RowControls>
          <RowIconButton icon={Plus} label={t("sidebar.project.openSession")} onPress={openSession} />
          <ActionMenu label={t("sidebar.project.actions", { name: project.name })} items={projectMenu(t, handlers, project, archived)} contextTargetRef={rowRef} />
        </RowControls>
      </TreeRow>
      {!isCollapsed && (
        // Marked so collapsing every project at once can tell whether keyboard focus is standing in
        // a part about to go away (see `foldAll`).
        <div data-sessions className="ms-3 mt-0.5 border-s border-separator ps-1">
          {live.length === 0 ? (
            <EmptyPanel
              compact
              icon={MessageSquarePlus}
              message={t("sidebar.noSessions")}
              action={{ label: t("sidebar.project.openSession"), icon: Plus, onPress: openSession }}
            />
          ) : (
            <div ref={listRef} className="relative flex flex-col gap-0.5">
              {live.map((session) => (
                <div key={session.id} data-flip={session.id}>
                  <SessionRow
                    handlers={handlers}
                    session={session}
                    owner={session.bound_to ? owners.get(session.bound_to) : undefined}
                    selectedSessionId={selectedSessionId}
                  />
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/** A session row: a project session's own, or one of the console's console sessions
 * (`ConsoleSessionsSection`), which share the row and its menu — pin, rename, archive, no resume —
 * since both are ordinary sessions once the row stops being a console session's sole, special one.
 * `owner`, given only for a bound project session, is the console session it reports to, drawn as
 * the binding badge in its colour. */
function SessionRow({
  handlers,
  session,
  owner,
  selectedSessionId,
}: {
  handlers: SidebarHandlers;
  session: Session;
  owner?: Session;
  selectedSessionId?: string;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const rowRef = useRef<HTMLDivElement>(null);
  return (
    <TreeRow
      ref={rowRef}
      ariaLabel={sessionAriaLabel(t, language, session, accounts, owner)}
      selected={session.id === selectedSessionId}
      onActivate={() => handlers.onSelectSession(session)}
    >
      <StatusIcon status={session.status} decorative />
      <AgentIcon agent={session.agent} />
      <RowLabel title={session.title}>{session.title}</RowLabel>
      {/* A console session shows its own colour, decorative here since the row's own label already
          names it; a bound project session's badge names its owner in its tooltip instead. */}
      {session.role === "console" && session.colour ? (
        <BindingBadge owner={session} decorative />
      ) : (
        owner && <BindingBadge owner={owner} />
      )}
      {session.pinned && <Pin aria-hidden="true" className="size-3 shrink-0 text-muted" />}
      <RowControls>
        <ActionMenu label={t("sidebar.session.actions", { title: session.title })} items={sessionMenu(t, handlers, session, accounts)} contextTargetRef={rowRef} />
      </RowControls>
    </TreeRow>
  );
}
