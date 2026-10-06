import { ScrollShadow } from "@heroui/react";
import { ChevronDown, ChevronRight } from "lucide-react";
import React, { useState } from "react";

import type { DialogRequest } from "../dialogs/dialogRequest";
import { useT } from "../i18n/react";
import { drawerClass, PANE_ID, PeekHotZone } from "../layout/paneOverlay";
import type { PaneWidth } from "../layout/paneWidth";
import type { PanePeek } from "../layout/usePaneToggles";
import { isDormant, type Console, type Project, type Session } from "../protocol";
import { statusLabel } from "../sessionLabel";
import { ActionMenu, type ActionMenuItem } from "./ActionMenu";
import { AgentBadge } from "./AgentBadge";
import { FadeOverflow } from "./FadeOverflow";
import { BubbledWaitingHand, StatusIcon } from "./StatusIcon";

/** The callbacks the tree triggers. Kept as one object, passed down by reference rather than
 * spread, so a child's prop list says exactly what data it narrows instead of inheriting whatever
 * the parent happened to have in scope under the same names. */
export interface SidebarHandlers {
  onSelectSession: (session: Session) => void;
  onOpenHub: (console: Console) => void;
  onOpenDialog: (dialog: DialogRequest) => void;
}

interface SidebarProps extends SidebarHandlers {
  consoles: Console[];
  projects: Project[];
  sessions: Session[];
  selectedSessionId?: string;
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

/** Stops a row's own mousedown from moving focus off whatever had it (typically the terminal) —
 * click still fires normally afterward. Shared by every row in the tree. */
function keepFocus(e: React.MouseEvent): void {
  e.preventDefault();
}

/** Enter and Space on a row, so a row that announces itself as a button can be operated as one.
 * Clicking deliberately does not focus the row (`keepFocus`), which is what keeps the terminal's
 * keyboard focus where it is; reaching a row by Tab still focuses it normally. */
function rowKeyHandler(activate: () => void) {
  return (event: React.KeyboardEvent): void => {
    // Only the row's own key presses. An action menu lives inside the row, and swallowing its
    // Enter would both block the menu and fire the row's action in its place.
    if (event.target !== event.currentTarget) return;
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    activate();
  };
}

/** One row of the tree: a `div` announcing itself as a button, since it carries an action menu and
 * a button cannot hold another. That is also why the tree is hand-built: HeroUI 3 has no Tree or
 * GridList, and its `Disclosure` trigger is itself a button, so it cannot contain the row's menu.
 * `expanded` is for a row that shows or hides the rows beneath it, and is announced as
 * `aria-expanded`. It is not a popup trigger: without `aria-haspopup` it does not match the
 * open-popup lookup in `usePaneToggles`, which keeps a floating sidebar open while a menu is. */
function TreeRow({
  ariaLabel,
  onActivate,
  selected,
  expanded,
  children,
}: {
  ariaLabel: string;
  onActivate: () => void;
  selected?: boolean;
  expanded?: boolean;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <div
      role="button"
      tabIndex={0}
      aria-label={ariaLabel}
      aria-current={selected ? "true" : undefined}
      aria-expanded={expanded}
      className={`flex min-h-8 cursor-pointer items-center gap-2 rounded-lg px-2 text-sm outline-none select-none hover:bg-default focus-visible:ring-2 focus-visible:ring-focus ${selected ? "bg-default" : ""}`}
      onMouseDown={keepFocus}
      onClick={onActivate}
      onKeyDown={rowKeyHandler(onActivate)}
    >
      {children}
    </div>
  );
}

function Chevron({ collapsed }: { collapsed: boolean }): React.ReactElement {
  // Two glyphs rather than one rotated: a rotated chevron that is already mirrored would point the
  // wrong way under right-to-left.
  return collapsed ? (
    <ChevronRight aria-hidden="true" className="size-4 shrink-0 text-muted rtl:-scale-x-100" />
  ) : (
    <ChevronDown aria-hidden="true" className="size-4 shrink-0 text-muted" />
  );
}

/** A row's single-line name: fades out at its end edge when it does not fit, rather than
 * ending in an ellipsis. `title` is the full text, offered as a tooltip only while it is cut. */
const RowLabel = ({ title, children }: { title?: string; children: React.ReactNode }) => (
  <FadeOverflow as="span" dir="auto" className="min-w-0 flex-1" titleWhenClipped={title}>
    {children}
  </FadeOverflow>
);

/** The console → project → session menu ("The console → project → session menu" in
 * docs/product/sessions.md). Expand/collapse state is purely local UI state; the daemon has no
 * notion of it. */
export function Sidebar({
  consoles,
  projects,
  sessions,
  selectedSessionId,
  open,
  peek,
  sidebarWidth,
  ...handlers
}: SidebarProps): React.ReactElement {
  const t = useT();
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());

  const toggle = (id: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const drawerClassName = peek
    ? `docked:rounded-e-xl ${drawerClass("start", "floating", open, peek.active)}`
    : drawerClass("start", "drawer", open);

  return (
    <>
      <nav
        // Below the `docked` breakpoint this is a fixed overlay, closed by default, slid on and off
        // with `open`; at or above it the `docked:` variants in `drawerClass` put it back exactly
        // where it always was, a plain row sibling, regardless of `open`. With the sidebar hidden
        // from the top bar's toggle, `drawerClass` instead keeps it a fixed overlay at the docked
        // width that floats in while `peek` is active. Starting below `--top-chrome-height` leaves
        // the top bar, and with it the sidebar toggle, visible while this is open.
        //
        // `data-escape-scope`: one of the origins `usePaneToggles`'s capture-phase Escape listener
        // closes a drawer for. `data-pane` is how it finds this element to see whether it holds
        // focus.
        data-escape-scope
        id={PANE_ID.sidebar}
        data-pane="sidebar"
        data-region="sidebar"
        className={`flex w-70 flex-col border-e border-separator bg-surface shrink-0 docked:w-(--sidebar-width) ${drawerClassName}`}
        onPointerEnter={peek?.keep}
        onPointerLeave={peek?.leave}
        style={{ "--sidebar-width": `${sidebarWidth.width}px` } as React.CSSProperties}
        aria-label={t("sidebar.sessions")}
      >
        <ScrollShadow size={24} className="min-h-0 flex-1 p-2">
          {consoles.map((console) => (
            <ConsoleNode
              key={console.id}
              handlers={handlers}
              console={console}
              projects={projects.filter((p) => p.console_id === console.id)}
              sessions={sessions.filter((s) => s.console_id === console.id)}
              selectedSessionId={selectedSessionId}
              collapsed={collapsed}
              toggle={toggle}
            />
          ))}
          {consoles.length === 0 && (
            <p className="px-2 py-1 text-sm text-muted">{t("sidebar.noConsoles")}</p>
          )}
        </ScrollShadow>
      </nav>
      {peek && <PeekHotZone side="start" peek={peek} />}
    </>
  );
}

function ConsoleNode({
  handlers,
  console: thisConsole,
  projects,
  sessions,
  selectedSessionId,
  collapsed,
  toggle,
}: {
  handlers: SidebarHandlers;
  console: Console;
  projects: Project[];
  sessions: Session[];
  selectedSessionId?: string;
  collapsed: Set<string>;
  toggle: (id: string) => void;
}): React.ReactElement {
  const t = useT();
  const isCollapsed = collapsed.has(thisConsole.id);
  // The daemon refuses to create or resume a second live hub, so there should only ever be one —
  // but the tree stays total regardless: sorting by `started_at` and keeping only the newest in the
  // Hub row means a second one (were it ever to exist) still gets a row, as an ordinary session,
  // rather than disappearing from the tree entirely.
  const liveHubs = sessions
    .filter((s) => s.role === "hub" && s.status !== "archived")
    .sort((a, b) => b.started_at - a.started_at);
  const hub = liveHubs[0];
  const extraHubs = liveHubs.slice(1);
  const archivedHubs = sessions.filter((s) => s.role === "hub" && s.status === "archived");
  const archiveKey = `${thisConsole.id}:archive`;
  const anyWaiting = sessions.some((s) => s.status === "waiting_user");
  const activateHub = () => (hub ? handlers.onSelectSession(hub) : handlers.onOpenHub(thisConsole));

  return (
    <div>
      <TreeRow
        ariaLabel={t(anyWaiting ? "sidebar.console.ariaLabelWaiting" : "sidebar.console.ariaLabel", { name: thisConsole.name })}
        onActivate={() => toggle(thisConsole.id)}
        expanded={!isCollapsed}
      >
        <Chevron collapsed={isCollapsed} />
        <RowLabel title={thisConsole.name}>
          <span className="font-medium">{thisConsole.name}</span>
        </RowLabel>
        {anyWaiting && <BubbledWaitingHand />}
        <ActionMenu
          label={t("sidebar.console.actions", { name: thisConsole.name })}
          items={[
            { label: t("sidebar.console.addProject"), onClick: () => handlers.onOpenDialog({ kind: "new-project", console: thisConsole }) },
            { label: t("sidebar.console.edit"), onClick: () => handlers.onOpenDialog({ kind: "edit-console", console: thisConsole }) },
            {
              label: t("sidebar.console.delete"),
              onClick: () => handlers.onOpenDialog({ kind: "delete-console", console: thisConsole }),
              destructive: true,
            },
          ]}
        />
      </TreeRow>
      {!isCollapsed && (
        <div className="ms-3">
          <TreeRow
            ariaLabel={hub ? t("sidebar.hub.ariaLabel", { status: statusLabel(t, hub.status) }) : t("sidebar.hub.start")}
            selected={hub !== undefined && hub.id === selectedSessionId}
            onActivate={activateHub}
          >
            {hub ? <StatusIcon status={hub.status} /> : <span className="size-4 shrink-0" />}
            <RowLabel title={t("sidebar.hub.name")}>{t("sidebar.hub.name")}</RowLabel>
            {hub && <AgentBadge agent={hub.agent} />}
            {hub && (
              // The only way to archive the hub: it cannot archive itself, and while it sits in this
              // row (running or interrupted), archiving it is what lets the row open a fresh one.
              <ActionMenu
                label={t("sidebar.hub.actions", { name: thisConsole.name })}
                items={[
                  ...(isDormant(hub.status) ? [{ label: t("sidebar.session.resume"), onClick: () => handlers.onSelectSession(hub) }] : []),
                  { label: t("sidebar.session.archive"), onClick: () => handlers.onOpenDialog({ kind: "archive-session", session: hub }), destructive: true },
                ]}
              />
            )}
          </TreeRow>
          {extraHubs.map((session) => (
            <SessionRow key={session.id} handlers={handlers} session={session} selectedSessionId={selectedSessionId} />
          ))}
          {archivedHubs.length > 0 && (
            // A hub belongs to no project, so this is the only Archive group that can ever show
            // one — without it, an archived hub would be unreachable once a fresh one takes its
            // place in the Hub row above.
            <ArchiveGroup
              label={t("sidebar.archive.hubs")}
              sessions={archivedHubs}
              handlers={handlers}
              selectedSessionId={selectedSessionId}
              isCollapsed={collapsed.has(archiveKey)}
              onToggle={() => toggle(archiveKey)}
            />
          )}
          {projects.map((project) => (
            <ProjectNode
              key={project.id}
              handlers={handlers}
              project={project}
              parentConsole={thisConsole}
              sessions={sessions.filter((s) => s.project_id === project.id)}
              selectedSessionId={selectedSessionId}
              collapsed={collapsed}
              toggle={toggle}
            />
          ))}
          {projects.length === 0 && <p className="px-2 py-1 text-sm text-muted">{t("sidebar.noProjects")}</p>}
        </div>
      )}
    </div>
  );
}

/** A collapsible group of archived sessions. Both a project's own archive and the console's archive
 * for archived hubs render through this one component — the caller supplies the label and the
 * collapsed/toggle state for whichever key is theirs, so neither grows a dependency on the other's. */
function ArchiveGroup({
  label,
  sessions,
  handlers,
  selectedSessionId,
  isCollapsed,
  onToggle,
}: {
  label: string;
  sessions: Session[];
  handlers: SidebarHandlers;
  selectedSessionId?: string;
  isCollapsed: boolean;
  onToggle: () => void;
}): React.ReactElement {
  const t = useT();
  return (
    <div>
      <TreeRow
        ariaLabel={t("sidebar.archive.ariaLabel", { label, count: sessions.length })}
        onActivate={onToggle}
        expanded={!isCollapsed}
      >
        <Chevron collapsed={isCollapsed} />
        <RowLabel>
          <span className="text-muted">
            {t("sidebar.archive.heading", { label, count: sessions.length })}
          </span>
        </RowLabel>
      </TreeRow>
      {!isCollapsed && (
        <div className="ms-3">
          {sessions.map((session) => (
            <SessionRow key={session.id} handlers={handlers} session={session} selectedSessionId={selectedSessionId} />
          ))}
        </div>
      )}
    </div>
  );
}

function ProjectNode({
  handlers,
  project,
  parentConsole,
  sessions,
  selectedSessionId,
  collapsed,
  toggle,
}: {
  handlers: SidebarHandlers;
  project: Project;
  parentConsole: Console;
  sessions: Session[];
  selectedSessionId?: string;
  collapsed: Set<string>;
  toggle: (id: string) => void;
}): React.ReactElement {
  const t = useT();
  const isCollapsed = collapsed.has(project.id);
  const live = sessions.filter((s) => s.status !== "archived");
  const archived = sessions.filter((s) => s.status === "archived");
  const archiveKey = `${project.id}:archive`;
  const anyWaiting = sessions.some((s) => s.status === "waiting_user");

  return (
    <div>
      <TreeRow
        ariaLabel={t(anyWaiting ? "sidebar.project.ariaLabelWaiting" : "sidebar.project.ariaLabel", { name: project.name })}
        onActivate={() => toggle(project.id)}
        expanded={!isCollapsed}
      >
        <Chevron collapsed={isCollapsed} />
        <RowLabel title={project.name}>{project.name}</RowLabel>
        {anyWaiting && <BubbledWaitingHand />}
        <ActionMenu
          label={t("sidebar.project.actions", { name: project.name })}
          items={[
            { label: t("sidebar.project.openSession"), onClick: () => handlers.onOpenDialog({ kind: "new-session", console: parentConsole, project }) },
            { label: t("sidebar.project.edit"), onClick: () => handlers.onOpenDialog({ kind: "edit-project", project }) },
            { label: t("sidebar.project.remove"), onClick: () => handlers.onOpenDialog({ kind: "delete-project", project }), destructive: true },
          ]}
        />
      </TreeRow>
      {!isCollapsed && (
        <div className="ms-3">
          {live.map((session) => (
            <SessionRow key={session.id} handlers={handlers} session={session} selectedSessionId={selectedSessionId} />
          ))}
          {live.length === 0 && <p className="px-2 py-1 text-sm text-muted">{t("sidebar.noSessions")}</p>}
          {archived.length > 0 && (
            <ArchiveGroup
              label={t("sidebar.archive.project")}
              sessions={archived}
              handlers={handlers}
              selectedSessionId={selectedSessionId}
              isCollapsed={collapsed.has(archiveKey)}
              onToggle={() => toggle(archiveKey)}
            />
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
  const items: ActionMenuItem[] = [
    ...(isDormant(session.status) ? [{ label: t("sidebar.session.resume"), onClick: () => handlers.onSelectSession(session) }] : []),
    { label: t("sidebar.session.rename"), onClick: () => handlers.onOpenDialog({ kind: "rename-session", session }) },
    ...(session.status !== "archived"
      ? [{ label: t("sidebar.session.archive"), onClick: () => handlers.onOpenDialog({ kind: "archive-session", session }), destructive: true }]
      : []),
  ];
  return (
    <TreeRow
      ariaLabel={t("sidebar.session.ariaLabel", { title: session.title, status: statusLabel(t, session.status) })}
      selected={session.id === selectedSessionId}
      onActivate={() => handlers.onSelectSession(session)}
    >
      <StatusIcon status={session.status} />
      <RowLabel title={session.title}>{session.title}</RowLabel>
      <AgentBadge agent={session.agent} />
      <ActionMenu label={t("sidebar.session.actions", { title: session.title })} items={items} />
    </TreeRow>
  );
}
