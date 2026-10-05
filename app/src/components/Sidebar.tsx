import React, { useState } from "react";

import { STATUS_LABEL } from "../sessionLabel";
import { isDormant, type Console, type Project, type Session } from "../protocol";
import { ActionMenu } from "./ActionMenu";
import { AgentBadge, BubbledWaitingHand, StatusIcon } from "./StatusIcon";

/** The callbacks the tree triggers. Kept as one object, passed down by reference rather than
 * spread, so a child's prop list says exactly what data it narrows instead of inheriting whatever
 * the parent happened to have in scope under the same names. */
export interface SidebarHandlers {
  onSelectSession: (session: Session) => void;
  onNewConsole: () => void;
  onEditConsole: (console: Console) => void;
  onDeleteConsole: (console: Console) => void;
  onOpenHub: (console: Console) => void;
  onNewProject: (console: Console) => void;
  onEditProject: (project: Project) => void;
  onDeleteProject: (project: Project) => void;
  onNewSession: (console: Console, project: Project) => void;
  onRenameSession: (session: Session) => void;
  onArchiveSession: (session: Session) => void;
}

interface SidebarProps extends SidebarHandlers {
  consoles: Console[];
  projects: Project[];
  sessions: Session[];
  selectedSessionId?: string;
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

/** The console → project → session three-level menu (docs/mvp.md "Interface"). Expand/collapse
 * state is purely local UI state; the daemon has no notion of it. */
export function Sidebar({
  consoles,
  projects,
  sessions,
  selectedSessionId,
  onNewConsole,
  ...handlerRest
}: SidebarProps): React.ReactElement {
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const handlers: SidebarHandlers = { onNewConsole, ...handlerRest };

  const toggle = (id: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  return (
    <nav className="sidebar">
      <div className="sidebar-header">
        <h1>Octoboard</h1>
        <button type="button" onClick={onNewConsole} title="New console">
          + Console
        </button>
      </div>
      <div className="sidebar-tree">
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
        {consoles.length === 0 && <p className="sidebar-empty">No consoles yet. Create one to get started.</p>}
      </div>
    </nav>
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

  return (
    <div className="tree-console">
      <div
        className="tree-row tree-row-console"
        role="button"
        tabIndex={0}
        aria-label={`${thisConsole.name} console${anyWaiting ? ", a session is waiting for you" : ""}`}
        onMouseDown={keepFocus}
        onClick={() => toggle(thisConsole.id)}
        onKeyDown={rowKeyHandler(() => toggle(thisConsole.id))}
      >
        <span className={`tree-disclosure${isCollapsed ? " tree-disclosure-collapsed" : ""}`} />
        <span className="tree-label">{thisConsole.name}</span>
        {anyWaiting && <BubbledWaitingHand />}
        <ActionMenu
          items={[
            { label: "Add project", onClick: () => handlers.onNewProject(thisConsole) },
            { label: "Edit console", onClick: () => handlers.onEditConsole(thisConsole) },
            { label: "Delete console", onClick: () => handlers.onDeleteConsole(thisConsole), destructive: true },
          ]}
        />
      </div>
      {!isCollapsed && (
        <div className="tree-children">
          <div
            className={`tree-row tree-row-hub${hub && hub.id === selectedSessionId ? " tree-row-selected" : ""}`}
            role="button"
            tabIndex={0}
            aria-label={hub ? `Hub session, ${STATUS_LABEL[hub.status]}` : "Start hub session"}
            onMouseDown={keepFocus}
            onClick={() => (hub ? handlers.onSelectSession(hub) : handlers.onOpenHub(thisConsole))}
            onKeyDown={rowKeyHandler(() => (hub ? handlers.onSelectSession(hub) : handlers.onOpenHub(thisConsole)))}
          >
            {hub ? <StatusIcon status={hub.status} /> : <span className="status-icon status-icon-placeholder" />}
            <span className="tree-label">Hub</span>
            {hub && <AgentBadge agent={hub.agent} />}
            {hub && (
              // The only way to archive the hub: it cannot archive itself, and while it sits in this
              // row (running or interrupted), archiving it is what lets the row open a fresh one.
              <ActionMenu
                items={[
                  ...(isDormant(hub.status) ? [{ label: "Resume", onClick: () => handlers.onSelectSession(hub) }] : []),
                  { label: "Archive", onClick: () => handlers.onArchiveSession(hub), destructive: true },
                ]}
              />
            )}
          </div>
          {extraHubs.map((session) => (
            <SessionRow key={session.id} handlers={handlers} session={session} selectedSessionId={selectedSessionId} />
          ))}
          {archivedHubs.length > 0 && (
            // A hub belongs to no project, so this is the only Archive group that can ever show
            // one — without it, an archived hub would be unreachable once a fresh one takes its
            // place in the Hub row above.
            <ArchiveGroup
              label="Archived hubs"
              className="tree-row-archive-console"
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
          {projects.length === 0 && <p className="sidebar-empty">No projects yet.</p>}
        </div>
      )}
    </div>
  );
}

/** A collapsible group of archived sessions. Both a project's own archive and the console's archive
 * for archived hubs render through this one component — the caller supplies the label, the indent
 * class, and the collapsed/toggle state for whichever key is theirs, so neither grows a dependency
 * on the other's. */
function ArchiveGroup({
  label,
  className,
  sessions,
  handlers,
  selectedSessionId,
  isCollapsed,
  onToggle,
}: {
  label: string;
  className?: string;
  sessions: Session[];
  handlers: SidebarHandlers;
  selectedSessionId?: string;
  isCollapsed: boolean;
  onToggle: () => void;
}): React.ReactElement {
  return (
    <div className="tree-archive">
      <div
        className={`tree-row tree-row-archive${className ? ` ${className}` : ""}`}
        role="button"
        tabIndex={0}
        aria-label={`${label}, ${sessions.length} ${sessions.length === 1 ? "session" : "sessions"}`}
        onMouseDown={keepFocus}
        onClick={onToggle}
        onKeyDown={rowKeyHandler(onToggle)}
      >
        <span className={`tree-disclosure${isCollapsed ? " tree-disclosure-collapsed" : ""}`} />
        <span className="tree-label">{label} ({sessions.length})</span>
      </div>
      {!isCollapsed && (
        <div className="tree-children">
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
  const isCollapsed = collapsed.has(project.id);
  const live = sessions.filter((s) => s.status !== "archived");
  const archived = sessions.filter((s) => s.status === "archived");
  const archiveKey = `${project.id}:archive`;
  const anyWaiting = sessions.some((s) => s.status === "waiting_user");

  return (
    <div className="tree-project">
      <div
        className="tree-row tree-row-project"
        role="button"
        tabIndex={0}
        aria-label={`${project.name} project${anyWaiting ? ", a session is waiting for you" : ""}`}
        onMouseDown={keepFocus}
        onClick={() => toggle(project.id)}
        onKeyDown={rowKeyHandler(() => toggle(project.id))}
      >
        <span className={`tree-disclosure${isCollapsed ? " tree-disclosure-collapsed" : ""}`} />
        <span className="tree-label">{project.name}</span>
        {anyWaiting && <BubbledWaitingHand />}
        <ActionMenu
          items={[
            { label: "Open session", onClick: () => handlers.onNewSession(parentConsole, project) },
            { label: "Edit project", onClick: () => handlers.onEditProject(project) },
            { label: "Remove project", onClick: () => handlers.onDeleteProject(project), destructive: true },
          ]}
        />
      </div>
      {!isCollapsed && (
        <div className="tree-children">
          {live.map((session) => (
            <SessionRow key={session.id} handlers={handlers} session={session} selectedSessionId={selectedSessionId} />
          ))}
          {live.length === 0 && <p className="sidebar-empty">No sessions.</p>}
          {archived.length > 0 && (
            <ArchiveGroup
              label="Archive"
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
  const resumable = isDormant(session.status);
  return (
    <div
      className={`tree-row tree-row-session${session.id === selectedSessionId ? " tree-row-selected" : ""}`}
      role="button"
      tabIndex={0}
      aria-label={`${session.title} session, ${STATUS_LABEL[session.status]}`}
      onMouseDown={keepFocus}
      onClick={() => handlers.onSelectSession(session)}
      onKeyDown={rowKeyHandler(() => handlers.onSelectSession(session))}
    >
      <StatusIcon status={session.status} />
      <span className="tree-label">{session.title}</span>
      <AgentBadge agent={session.agent} />
      <ActionMenu
        items={[
          ...(resumable ? [{ label: "Resume", onClick: () => handlers.onSelectSession(session) }] : []),
          { label: "Rename", onClick: () => handlers.onRenameSession(session) },
          ...(session.status !== "archived"
            ? [{ label: "Archive", onClick: () => handlers.onArchiveSession(session), destructive: true }]
            : []),
        ]}
      />
    </div>
  );
}
