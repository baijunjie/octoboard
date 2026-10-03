import React, { useState } from "react";

import { isDormant, type Console, type Project, type Session } from "../protocol";
import { ActionMenu } from "./ActionMenu";
import { AgentBadge, StatusIcon } from "./StatusIcon";

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
  // Archived hub sessions are ignored here so the row always resolves to a session the user can
  // still resume into — otherwise an old, archived hub would keep showing up forever instead of
  // the fresh one `onOpenHub` starts once the Hub row has nothing live to find.
  // TODO(milestone 02): nothing can archive a hub session today, so this filter has no visible
  // effect yet — but 02's automatic archiving (a hub-dispatched session finishing cleanly) could
  // reach a hub session once hubs can be dispatched to, and an archived hub would then need its own
  // way back into view (today it would simply become unreachable: filtered from this row, and a hub
  // belongs to no project's Archive group either).
  const hub = sessions.find((s) => s.role === "hub" && s.status !== "archived");

  return (
    <div className="tree-console">
      <div
        className="tree-row tree-row-console"
        role="button"
        tabIndex={0}
        aria-label={`${thisConsole.name} console`}
        onMouseDown={keepFocus}
        onClick={() => toggle(thisConsole.id)}
        onKeyDown={rowKeyHandler(() => toggle(thisConsole.id))}
      >
        <span className={`tree-disclosure${isCollapsed ? " tree-disclosure-collapsed" : ""}`} />
        <span className="tree-label">{thisConsole.name}</span>
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
            aria-label={hub ? `Hub session, ${hub.status}` : "Start hub session"}
            onMouseDown={keepFocus}
            onClick={() => (hub ? handlers.onSelectSession(hub) : handlers.onOpenHub(thisConsole))}
            onKeyDown={rowKeyHandler(() => (hub ? handlers.onSelectSession(hub) : handlers.onOpenHub(thisConsole)))}
          >
            {hub ? <StatusIcon status={hub.status} /> : <span className="status-icon status-icon-placeholder" />}
            <span className="tree-label">Hub</span>
            {hub && <AgentBadge agent={hub.agent} />}
          </div>
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
  const archiveCollapsed = collapsed.has(archiveKey);

  return (
    <div className="tree-project">
      <div
        className="tree-row tree-row-project"
        role="button"
        tabIndex={0}
        aria-label={`${project.name} project`}
        onMouseDown={keepFocus}
        onClick={() => toggle(project.id)}
        onKeyDown={rowKeyHandler(() => toggle(project.id))}
      >
        <span className={`tree-disclosure${isCollapsed ? " tree-disclosure-collapsed" : ""}`} />
        <span className="tree-label">{project.name}</span>
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
            <div className="tree-archive">
              <div
                className="tree-row tree-row-archive"
                role="button"
                tabIndex={0}
                aria-label={`Archive, ${archived.length} sessions`}
                onMouseDown={keepFocus}
                onClick={() => toggle(archiveKey)}
                onKeyDown={rowKeyHandler(() => toggle(archiveKey))}
              >
                <span className={`tree-disclosure${archiveCollapsed ? " tree-disclosure-collapsed" : ""}`} />
                <span className="tree-label">Archive ({archived.length})</span>
              </div>
              {!archiveCollapsed && (
                <div className="tree-children">
                  {archived.map((session) => (
                    <SessionRow key={session.id} handlers={handlers} session={session} selectedSessionId={selectedSessionId} />
                  ))}
                </div>
              )}
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
  const resumable = isDormant(session.status);
  return (
    <div
      className={`tree-row tree-row-session${session.id === selectedSessionId ? " tree-row-selected" : ""}`}
      role="button"
      tabIndex={0}
      aria-label={`${session.title} session, ${session.status}`}
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
