import { Button } from "@heroui/react";
import React, { useState } from "react";

import type { DialogRequest } from "../dialogs/dialogRequest";
import { isDormant, type Console, type Project, type Session } from "../protocol";
import { STATUS_LABEL } from "../sessionLabel";
import { ActionMenu, type ActionMenuItem } from "./ActionMenu";
import { AgentBadge } from "./AgentBadge";
import { NotificationsPrompt } from "./NotificationsPrompt";
import { BubbledWaitingHand, StatusIcon } from "./StatusIcon";
import { TitledControl } from "./TitledControl";
import { TrustedFolders } from "./TrustedFolders";

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
  /** Directories whose projects Octoboard answers Claude Code's trust prompt for. */
  trustedDirectories: string[];
  onRemoveTrustedDirectory: (path: string) => void;
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
 * a button cannot hold another. */
function TreeRow({
  ariaLabel,
  onActivate,
  selected,
  children,
}: {
  ariaLabel: string;
  onActivate: () => void;
  selected?: boolean;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <div
      role="button"
      tabIndex={0}
      aria-label={ariaLabel}
      aria-current={selected ? "true" : undefined}
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
  return (
    <span aria-hidden="true" className="w-3 shrink-0 text-xs text-muted">
      {collapsed ? "▸" : "▾"}
    </span>
  );
}

const RowLabel = ({ children }: { children: React.ReactNode }) => <span className="min-w-0 flex-1 truncate">{children}</span>;

/** The console → project → session menu ("The console → project → session menu" in
 * docs/product/sessions.md), with the list of trusted folders under it. Expand/collapse state is
 * purely local UI state; the daemon has no notion of it. */
export function Sidebar({
  consoles,
  projects,
  sessions,
  selectedSessionId,
  trustedDirectories,
  onRemoveTrustedDirectory,
  ...handlers
}: SidebarProps): React.ReactElement {
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());

  const toggle = (id: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  return (
    <nav className="flex w-70 shrink-0 flex-col border-r border-separator bg-surface" aria-label="Sessions">
      <div className="flex shrink-0 items-center justify-between px-3 py-2">
        <h1 className="text-base font-semibold">Octoboard</h1>
        <TitledControl title="New console">
          <Button
            size="sm"
            variant="secondary"
            preventFocusOnPress
            onPress={() => handlers.onOpenDialog({ kind: "new-console" })}
          >
            + Console
          </Button>
        </TitledControl>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
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
          <p className="px-2 py-1 text-sm text-muted">No consoles yet. Create one to get started.</p>
        )}
      </div>
      <TrustedFolders directories={trustedDirectories} onRemove={onRemoveTrustedDirectory} />
      <NotificationsPrompt />
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
  const activateHub = () => (hub ? handlers.onSelectSession(hub) : handlers.onOpenHub(thisConsole));

  return (
    <div>
      <TreeRow
        ariaLabel={`${thisConsole.name} console${anyWaiting ? ", a session is waiting for you" : ""}`}
        onActivate={() => toggle(thisConsole.id)}
      >
        <Chevron collapsed={isCollapsed} />
        <RowLabel>
          <span className="font-medium">{thisConsole.name}</span>
        </RowLabel>
        {anyWaiting && <BubbledWaitingHand />}
        <ActionMenu
          label={`Actions for console ${thisConsole.name}`}
          items={[
            { label: "Add project", onClick: () => handlers.onOpenDialog({ kind: "new-project", console: thisConsole }) },
            { label: "Edit console", onClick: () => handlers.onOpenDialog({ kind: "edit-console", console: thisConsole }) },
            {
              label: "Delete console",
              onClick: () => handlers.onOpenDialog({ kind: "delete-console", console: thisConsole }),
              destructive: true,
            },
          ]}
        />
      </TreeRow>
      {!isCollapsed && (
        <div className="ml-3">
          <TreeRow
            ariaLabel={hub ? `Hub session, ${STATUS_LABEL[hub.status]}` : "Start hub session"}
            selected={hub !== undefined && hub.id === selectedSessionId}
            onActivate={activateHub}
          >
            {hub ? <StatusIcon status={hub.status} /> : <span className="size-4 shrink-0" />}
            <RowLabel>Hub</RowLabel>
            {hub && <AgentBadge agent={hub.agent} />}
            {hub && (
              // The only way to archive the hub: it cannot archive itself, and while it sits in this
              // row (running or interrupted), archiving it is what lets the row open a fresh one.
              <ActionMenu
                label={`Actions for the hub of ${thisConsole.name}`}
                items={[
                  ...(isDormant(hub.status) ? [{ label: "Resume", onClick: () => handlers.onSelectSession(hub) }] : []),
                  { label: "Archive", onClick: () => handlers.onOpenDialog({ kind: "archive-session", session: hub }), destructive: true },
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
              label="Archived hubs"
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
          {projects.length === 0 && <p className="px-2 py-1 text-sm text-muted">No projects yet.</p>}
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
  return (
    <div>
      <TreeRow
        ariaLabel={`${label}, ${sessions.length} ${sessions.length === 1 ? "session" : "sessions"}`}
        onActivate={onToggle}
      >
        <Chevron collapsed={isCollapsed} />
        <RowLabel>
          <span className="text-muted">
            {label} ({sessions.length})
          </span>
        </RowLabel>
      </TreeRow>
      {!isCollapsed && (
        <div className="ml-3">
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
    <div>
      <TreeRow
        ariaLabel={`${project.name} project${anyWaiting ? ", a session is waiting for you" : ""}`}
        onActivate={() => toggle(project.id)}
      >
        <Chevron collapsed={isCollapsed} />
        <RowLabel>{project.name}</RowLabel>
        {anyWaiting && <BubbledWaitingHand />}
        <ActionMenu
          label={`Actions for project ${project.name}`}
          items={[
            { label: "Open session", onClick: () => handlers.onOpenDialog({ kind: "new-session", console: parentConsole, project }) },
            { label: "Edit project", onClick: () => handlers.onOpenDialog({ kind: "edit-project", project }) },
            { label: "Remove project", onClick: () => handlers.onOpenDialog({ kind: "delete-project", project }), destructive: true },
          ]}
        />
      </TreeRow>
      {!isCollapsed && (
        <div className="ml-3">
          {live.map((session) => (
            <SessionRow key={session.id} handlers={handlers} session={session} selectedSessionId={selectedSessionId} />
          ))}
          {live.length === 0 && <p className="px-2 py-1 text-sm text-muted">No sessions.</p>}
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
  const items: ActionMenuItem[] = [
    ...(isDormant(session.status) ? [{ label: "Resume", onClick: () => handlers.onSelectSession(session) }] : []),
    { label: "Rename", onClick: () => handlers.onOpenDialog({ kind: "rename-session", session }) },
    ...(session.status !== "archived"
      ? [{ label: "Archive", onClick: () => handlers.onOpenDialog({ kind: "archive-session", session }), destructive: true }]
      : []),
  ];
  return (
    <TreeRow
      ariaLabel={`${session.title} session, ${STATUS_LABEL[session.status]}`}
      selected={session.id === selectedSessionId}
      onActivate={() => handlers.onSelectSession(session)}
    >
      <StatusIcon status={session.status} />
      <RowLabel>{session.title}</RowLabel>
      <AgentBadge agent={session.agent} />
      <ActionMenu label={`Actions for session ${session.title}`} items={items} />
    </TreeRow>
  );
}
