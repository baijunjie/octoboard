import { ScrollShadow } from "@heroui/react";
import {
  Archive,
  ChevronDown,
  ChevronRight,
  ChevronsUpDown,
  FolderOpen,
  FolderPlus,
  LayoutDashboard,
  MessageSquarePlus,
  Pencil,
  Pin,
  Play,
  Plus,
  SearchX,
  Trash2,
  Waypoints,
} from "lucide-react";
import { setInteractionModality } from "react-aria";
import React, { useEffect, useLayoutEffect, useRef, useState } from "react";

import { AGENT_LABEL } from "../agents";
import { ActionMenu, type ActionMenuEntry } from "../components/ActionMenu";
import { AgentIcon } from "../components/AgentIcon";
import { ConsoleAvatar } from "../components/ConsoleAvatar";
import { EmptyPanel } from "../components/EmptyPanel";
import { ActivityMarker, StatusIcon } from "../components/StatusIcon";
import { useT } from "../i18n/react";
import { drawerClass, PANE_ID, PeekHotZone } from "../layout/paneOverlay";
import type { PaneWidth } from "../layout/paneWidth";
import type { PanePeek } from "../layout/usePaneToggles";
import { effectiveTags, matchesFilter, tagVocabulary, withoutTags } from "../projectFiltering";
import type { Console, Project, Session } from "../protocol";
import { sessionAriaLabel, statusLabel } from "../sessionLabel";
import { FocusView } from "./FocusView";
import { type FilterUpdate, NO_FILTER, type ProjectFilter, ProjectFilterButton, ProjectFilterTag, ProjectFilterTags } from "./ProjectFilter";
import { archiveSubmenu, projectMenu, sessionMenu } from "./menus";
import { archivedSessions, consoleActivity, isInactiveProject, liveSessions, sortProjects, type Activity } from "./order";
import { RowControls, RowIconButton, RowLabel, SectionHeading, TreeRow } from "./rows";
import type { SidebarHandlers } from "./types";
import { useFlip } from "./useFlip";

interface SidebarProps extends SidebarHandlers {
  consoles: Console[];
  projects: Project[];
  sessions: Session[];
  selectedSessionId?: string;
  /** The console the sidebar shows; the caller resolves it to an existing one. */
  currentConsole?: Console;
  /** The project in focus mode, if any; it belongs to `currentConsole`. */
  focusProject?: Project;
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
 * docs/product/sidebar.md), with its hub and projects — or, in a project's focus mode, that
 * project alone. Expand/collapse state is purely local UI state; the daemon has no notion of it. */
export function Sidebar({
  consoles,
  projects,
  sessions,
  selectedSessionId,
  currentConsole,
  focusProject,
  open,
  peek,
  sidebarWidth,
  ...handlers
}: SidebarProps): React.ReactElement {
  const t = useT();
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  // Each console's project filter (keyword and tags), held here rather than in the project list so
  // that focus mode, which replaces the list, and switching consoles both leave it in place.
  const [filters, setFilters] = useState<Map<string, ProjectFilter>>(new Map());

  const toggle = (id: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  // Entering or leaving focus mode swaps the sidebar's content, which unmounts the control that
  // started it and drops a keyboard user's focus to `<body>` (no blur event fires). `holdsFocus`
  // remembers that focus was in the sidebar (React's focus events also bubble up from its menus'
  // portals), so focus can go to the new view's first control; a mouse press leaves it on the
  // terminal, which this does not touch.
  const navRef = useRef<HTMLElement>(null);
  const holdsFocus = useRef(false);
  const shownFocusId = useRef(focusProject?.id);
  useEffect(() => {
    const previousId = shownFocusId.current;
    shownFocusId.current = focusProject?.id;
    if (previousId === focusProject?.id) return;
    // After a short delay: a menu item that switched the view has its menu hand focus back (to the
    // trigger that just unmounted) a task later, which would otherwise land after this.
    const timer = setTimeout(() => {
      if (!holdsFocus.current || document.activeElement !== document.body) return;
      const target = focusProject
        ? navRef.current?.querySelector<HTMLElement>("[data-focus-exit]")
        : navRef.current?.querySelector<HTMLElement>(`[data-flip="${CSS.escape(previousId ?? "")}"] [role=button]`);
      if (!target) return;
      setInteractionModality("keyboard");
      target.focus();
    }, 50);
    return () => clearTimeout(timer);
  }, [focusProject?.id]);

  const drawerClassName = peek
    ? `docked:rounded-e-xl ${drawerClass("start", "floating", open, peek.active)}`
    : drawerClass("start", "drawer", open);

  const consoleSessions = currentConsole ? sessions.filter((s) => s.console_id === currentConsole.id) : [];

  // What the sidebar is showing, and how it arrived there: going down into a project's focus mode
  // slides the new view in from the end, coming back up slides it in from the start, and switching
  // to another console fades it in.
  const viewKey = !currentConsole ? "none" : focusProject ? `focus:${focusProject.id}` : `console:${currentConsole.id}`;
  const viewRef = useRef<HTMLDivElement>(null);
  const previousViewRef = useRef(viewKey);
  // Played in place rather than by remounting the view: a remount on a console switch would
  // destroy the switcher's trigger that the menu hands keyboard focus back to.
  useLayoutEffect(() => {
    const previous = previousViewRef.current;
    previousViewRef.current = viewKey;
    if (previous === viewKey || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const from = viewKey.startsWith("focus:")
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
  } else if (focusProject) {
    content = (
      <FocusView
        handlers={handlers}
        console={currentConsole}
        project={focusProject}
        sessions={consoleSessions.filter((s) => s.project_id === focusProject.id)}
        selectedSessionId={selectedSessionId}
      />
    );
  } else {
    content = (
      <>
        <ConsoleSwitcher handlers={handlers} consoles={consoles} sessions={sessions} current={currentConsole} />
        <ScrollShadow size={24} className="min-h-0 flex-1 px-2 pb-2">
          <ConsoleBody
            handlers={handlers}
            console={currentConsole}
            projects={projects.filter((p) => p.console_id === currentConsole.id)}
            sessions={consoleSessions}
            selectedSessionId={selectedSessionId}
            collapsed={collapsed}
            toggle={toggle}
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
        <div key={focusProject?.id ?? "console"} ref={viewRef} className="flex min-h-0 flex-1 flex-col">
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

  const consoleActions: ActionMenuEntry[] = [
    { label: t("sidebar.console.addProject"), icon: FolderPlus, onClick: () => handlers.onOpenDialog({ kind: "new-project", console: current }) },
    { label: t("sidebar.console.edit"), icon: Pencil, onClick: () => handlers.onOpenDialog({ kind: "edit-console", console: current }) },
    "separator",
    { label: t("sidebar.console.delete"), icon: Trash2, onClick: () => handlers.onOpenDialog({ kind: "delete-console", console: current }), destructive: true },
  ];

  return (
    <div className="flex h-14 shrink-0 items-center gap-1 border-b border-separator px-2">
      <ActionMenu
        className="min-w-0 flex-1"
        label={t(elsewhere ? "sidebar.console.switchWaiting" : "sidebar.console.switch", { name: current.name })}
        tooltip={false}
        items={items}
        triggerClassName="flex h-9 w-full min-w-0 items-center gap-2 rounded-lg px-2 text-start text-sm hover:bg-default aria-expanded:bg-default"
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
      <ActionMenu label={t("sidebar.console.actions", { name: current.name })} items={consoleActions} />
    </div>
  );
}

function ConsoleBody({
  handlers,
  console: thisConsole,
  projects,
  sessions,
  selectedSessionId,
  collapsed,
  toggle,
  filter: stored,
  setFilter,
}: {
  handlers: SidebarHandlers;
  console: Console;
  projects: Project[];
  sessions: Session[];
  selectedSessionId?: string;
  collapsed: Set<string>;
  toggle: (id: string) => void;
  filter: ProjectFilter;
  setFilter: (update: FilterUpdate) => void;
}): React.ReactElement {
  const t = useT();
  const listRef = useFlip<HTMLDivElement>();
  const filterButton = useRef<HTMLDivElement>(null);
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

  return (
    <>
      <HubRows handlers={handlers} console={thisConsole} sessions={sessions} selectedSessionId={selectedSessionId} />
      <SectionHeading
        after={
          // Only while the filter button is there (below): the chip hands focus to it when removed.
          projects.length > 0 &&
          filter.keyword.trim() !== "" && (
            <ProjectFilterTag
              keyword={filter.keyword.trim()}
              returnFocusTo={filterButton}
              onRemove={() => setFilter((f) => ({ ...f, keyword: "" }))}
            />
          )
        }
        action={
          projects.length > 0 && (
            <ProjectFilterButton
              filter={filter}
              vocabulary={vocabulary}
              onChange={setFilter}
              holderRef={filterButton}
            />
          )
        }
      >
        {t("sidebar.projects")}
      </SectionHeading>
      {filter.tags.length > 0 && (
        <ProjectFilterTags
          tags={filter.tags}
          returnFocusTo={filterButton}
          onRemove={(removed) => setFilter((f) => ({ ...f, tags: withoutTags(f.tags, removed) }))}
        />
      )}
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

/** The console's Hub row, and below it any further hub that is not archived (should one ever
 * exist). Archived hubs are reached from the row's menu. */
function HubRows({
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
  // The daemon refuses to create or resume a second live hub, so there should only ever be one —
  // but the sidebar stays total regardless: sorting by `started_at` and keeping only the newest in
  // the Hub row means a second one (were it ever to exist) still gets a row, as an ordinary
  // session, rather than disappearing.
  const liveHubs = sessions
    .filter((s) => s.role === "hub" && s.status !== "archived")
    .sort((a, b) => b.started_at - a.started_at);
  const hub = liveHubs[0];
  const extraHubs = liveHubs.slice(1);
  const archivedHubs = archivedSessions(sessions.filter((s) => s.role === "hub"));
  const activateHub = () => (hub ? handlers.onSelectSession(hub) : handlers.onOpenHub(thisConsole));

  const menu: ActionMenuEntry[] = [
    archiveSubmenu(t, t("sidebar.archive.hubs"), archivedHubs, handlers.onSelectSession, () =>
      handlers.onOpenArchive({ console: thisConsole.id }),
    ),
    // The only way to archive the hub: it cannot archive itself, and while it sits in this row
    // (running or interrupted), archiving it is what lets the row open a fresh one.
    ...(hub
      ? (["separator", { label: t("sidebar.session.archive"), icon: Archive, onClick: () => handlers.onOpenDialog({ kind: "archive-session", session: hub }) }] as ActionMenuEntry[])
      : []),
  ];

  return (
    <div className="flex flex-col gap-0.5 pt-2">
      <TreeRow
        ariaLabel={hub ? t("sidebar.hub.ariaLabel", { agent: AGENT_LABEL[hub.agent], status: statusLabel(t, hub.status) }) : t("sidebar.hub.start")}
        selected={hub !== undefined && hub.id === selectedSessionId}
        onActivate={activateHub}
      >
        <span className="flex size-4 shrink-0 items-center justify-center">
          {hub ? <StatusIcon status={hub.status} decorative /> : <Play aria-hidden="true" className="size-3.5 text-muted" />}
        </span>
        {hub ? <AgentIcon agent={hub.agent} /> : <Waypoints aria-hidden="true" className="size-4 shrink-0 text-muted" />}
        <RowLabel title={t("sidebar.hub.name")}>
          <span className="font-medium">{t("sidebar.hub.name")}</span>
          {!hub && <span className="ms-2 text-muted">{t("sidebar.hub.startHint")}</span>}
        </RowLabel>
        <RowControls>
          {(hub || archivedHubs.length > 0) && <ActionMenu label={t("sidebar.hub.actions", { name: thisConsole.name })} items={menu} />}
        </RowControls>
      </TreeRow>
      {extraHubs.map((session) => (
        <SessionRow key={session.id} handlers={handlers} session={session} selectedSessionId={selectedSessionId} />
      ))}
    </div>
  );
}

function ProjectNode({
  handlers,
  project,
  parentConsole,
  sessions,
  selectedSessionId,
  isCollapsed,
  onToggle,
}: {
  handlers: SidebarHandlers;
  project: Project;
  parentConsole: Console;
  sessions: Session[];
  selectedSessionId?: string;
  isCollapsed: boolean;
  onToggle: () => void;
}): React.ReactElement {
  const t = useT();
  const live = liveSessions(sessions);
  const archived = archivedSessions(sessions);
  const activity = consoleActivity(live);
  const listRef = useFlip<HTMLDivElement>();
  const openSession = () => handlers.onOpenDialog({ kind: "new-session", console: parentConsole, project });
  const activityKey = activityLabelKey(activity, project.pinned);

  return (
    <div>
      <TreeRow
        ariaLabel={
          activityKey
            ? t(activityKey, { name: project.name })
            : project.pinned
              ? t("sidebar.project.ariaLabelPinned", { name: project.name })
              : project.name
        }
        onActivate={onToggle}
        expanded={!isCollapsed}
      >
        <div className="flex min-w-0 flex-1 items-center">
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
            <ChevronDown aria-hidden="true" className="h-3.5 w-0 shrink-0 overflow-hidden text-muted opacity-0 transition-opacity group-hover:ms-2 group-hover:w-3.5 group-hover:opacity-100 group-focus-visible:ms-2 group-focus-visible:w-3.5 group-focus-visible:opacity-100" />
          )}
        </div>
        {isCollapsed && <ActivityMarker activity={activity} />}
        <RowControls>
          <RowIconButton icon={Plus} label={t("sidebar.project.openSession")} onPress={openSession} />
          <ActionMenu label={t("sidebar.project.actions", { name: project.name })} items={projectMenu(t, handlers, project, archived)} />
        </RowControls>
      </TreeRow>
      {!isCollapsed && (
        <div className="ms-3 mt-0.5 border-s border-separator ps-1">
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
                  <SessionRow handlers={handlers} session={session} selectedSessionId={selectedSessionId} />
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function SessionRow({
  handlers,
  session,
  selectedSessionId,
}: {
  handlers: SidebarHandlers;
  session: Session;
  selectedSessionId?: string;
}): React.ReactElement {
  const t = useT();
  return (
    <TreeRow
      ariaLabel={sessionAriaLabel(t, session)}
      selected={session.id === selectedSessionId}
      onActivate={() => handlers.onSelectSession(session)}
    >
      <StatusIcon status={session.status} decorative />
      <AgentIcon agent={session.agent} />
      <RowLabel title={session.title}>{session.title}</RowLabel>
      {session.pinned && <Pin aria-hidden="true" className="size-3 shrink-0 text-muted" />}
      <RowControls>
        <ActionMenu label={t("sidebar.session.actions", { title: session.title })} items={sessionMenu(t, handlers, session)} />
      </RowControls>
    </TreeRow>
  );
}
