import { Button, ScrollShadow } from "@heroui/react";
import { Archive, ArrowLeft, FolderOpen, FolderPlus, List, MessageSquarePlus, Pin, Plus } from "lucide-react";
import React from "react";

import { AGENT_LABEL } from "../agents";
import { ActionMenu } from "../components/ActionMenu";
import { AgentAccountText } from "../components/AgentAccountText";
import { AgentIcon } from "../components/AgentIcon";
import { EmptyPanel } from "../components/EmptyPanel";
import { FadeOverflow } from "../components/FadeOverflow";
import { StatusIcon } from "../components/StatusIcon";
import { TitledControl } from "../components/TitledControl";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { Console, Project, Session } from "../protocol";
import { formatRelativeTime } from "../relativeTime";
import { sessionAccountName, sessionAgentLabel, sessionAriaLabel, statusLabel } from "../sessionLabel";
import { useDaemonStore } from "../store";
import { BindingBadge } from "./BindingBadge";
import { GitBadge } from "./GitBadge";
import { projectMenu, sessionMenu } from "./menus";
import { archivedSessions, boundArchivedSessions, boundElsewhere, focusGroups, liveSessions, sortProjects, unboundSessions } from "./order";
import { RowControls, RowIconButton, RowLabel, SectionHeading, TreeRow } from "./rows";
import type { SidebarHandlers } from "./types";
import { useFlip } from "./useFlip";

/** How many archived sessions focus mode lists before "View all". */
const ARCHIVE_PREVIEW = 10;

/** The top of either focus view: the back button that leaves focus mode, then whatever the view
 * puts after it. Both views are built from this header, the session cards and the archived list
 * below, so they differ only in what they are given. */
function FocusHeader({ handlers, children }: { handlers: SidebarHandlers; children: React.ReactNode }): React.ReactElement {
  const t = useT();
  return (
    // A card below sits in the scroll area, then inside its own border and padding (`px-2` + 1px +
    // `p-3`), so the end padding matches that and this header's trailing menus line up with the menu
    // on a card. The start stays put: only the trailing icons were inset short of the cards.
    <div className="flex h-14 shrink-0 items-center gap-1 border-b border-separator ps-2 pe-[21px]">
      <TitledControl title={t("sidebar.focus.exit")}>
        <Button
          isIconOnly
          size="sm"
          variant="ghost"
          aria-label={t("sidebar.focus.exit")}
          data-focus-exit
          preventFocusOnPress
          onPress={() => handlers.onFocus(undefined)}
        >
          <ArrowLeft aria-hidden="true" className="size-4 rtl:-scale-x-100" />
        </Button>
      </TitledControl>
      {children}
    </div>
  );
}

/** What a focus view's header names, under the console's name. */
function FocusTitle({ consoleName, title }: { consoleName: string; title: string }): React.ReactElement {
  return (
    <>
      <div className="truncate text-xs text-muted" dir="auto">
        {consoleName}
      </div>
      <RowLabel title={title} className="block min-w-0">
        <span className="text-sm font-semibold">{title}</span>
      </RowLabel>
    </>
  );
}

/** A project's focus mode: the sidebar given over to one project, its unbound sessions as cards
 * and its recent archive below them ("Focus mode" in docs/product/sidebar.md). The sessions bound
 * to a console session are left out of the list and summed up in one line leading to each console
 * session's own focus mode; the archive, bound sessions included, is not filtered. */
export function ProjectFocusView({
  handlers,
  console: parentConsole,
  project,
  sessions,
  owners,
  selectedSessionId,
}: {
  handlers: SidebarHandlers;
  console: Console;
  project: Project;
  /** Every session of the project, whatever its status and binding. */
  sessions: Session[];
  /** This console's console sessions, by id, to name the owners of the sessions left out. */
  owners: Map<string, Session>;
  selectedSessionId?: string;
}): React.ReactElement {
  const t = useT();
  // Read directly rather than taking it as a prop threaded through `Sidebar.tsx`: this project's
  // own entry, which keeps the same `GitStatus` reference until that entry itself changes, so no
  // `useShallow` is needed.
  const gitStatus = useDaemonStore((s) => s.gitStatuses.get(project.id));
  const live = liveSessions(unboundSessions(sessions));
  const archived = archivedSessions(sessions);
  const elsewhere = boundElsewhere(sessions, owners);
  const openSession = () =>
    handlers.onOpenDialog({ kind: "new-session", console: parentConsole, project, binding: { kind: "unbound" } });
  const viewAll = () => handlers.onOpenArchive({ console: project.console_id, project: project.id });

  return (
    <>
      <FocusHeader handlers={handlers}>
        {/* `min-w-16` is the floor itself: this is the flex item (`flex-1`'s basis is 0%, so a
            floor on the `RowLabel` child below would clamp nothing), leaving `GitBadge`'s own
            `min-w-0` as the one that keeps giving way once this is reached. */}
        <div className="min-w-16 flex-1 px-1">
          <FocusTitle consoleName={parentConsole.name} title={project.name} />
        </div>
        <GitBadge status={gitStatus} />
        <RowIconButton icon={Plus} label={t("sidebar.project.openSession")} onPress={openSession} />
        <ActionMenu
          label={t("sidebar.project.actions", { name: project.name })}
          items={projectMenu(t, handlers, project, archived, { inFocus: true })}
        />
      </FocusHeader>
      <ScrollShadow size={24} className="min-h-0 flex-1 px-2 pb-2">
        <SectionHeading>{t("sidebar.focus.sessions", { count: live.length })}</SectionHeading>
        {elsewhere && <BoundElsewhere handlers={handlers} count={elsewhere.count} owners={elsewhere.owners} />}
        {live.length === 0 ? (
          <EmptyPanel
            icon={MessageSquarePlus}
            message={t("sidebar.noSessions")}
            action={{ label: t("sidebar.project.openSession"), icon: Plus, onPress: openSession }}
          />
        ) : (
          <SessionCards handlers={handlers} sessions={live} selectedSessionId={selectedSessionId} />
        )}
        <ArchivedList handlers={handlers} archived={archived} selectedSessionId={selectedSessionId} onViewAll={viewAll} />
      </ScrollShadow>
    </>
  );
}

/** A console session's focus mode: the sidebar given over to the projects that have a session bound
 * to it and, within each, only those sessions as cards, with its archived bound sessions below. A
 * session opened from here is bound to it, with no choice offered. What has no meaning here is
 * left out: a project's own focus mode (`projectMenu`'s `inFocus`) and the project list's filter. */
export function ConsoleSessionFocusView({
  handlers,
  console: parentConsole,
  consoleSession,
  projects,
  sessions,
  selectedSessionId,
}: {
  handlers: SidebarHandlers;
  console: Console;
  consoleSession: Session;
  /** The console's projects. */
  projects: Project[];
  /** Every session of the console. */
  sessions: Session[];
  selectedSessionId?: string;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const groups = focusGroups(projects, sessions, consoleSession.id);
  const liveCount = groups.reduce((sum, group) => sum + group.sessions.length, 0);
  const archived = boundArchivedSessions(sessions, consoleSession.id);
  const openSession = (project: Project) =>
    handlers.onOpenDialog({ kind: "new-session", console: parentConsole, project, binding: { kind: "bound", to: consoleSession } });
  const viewAll = () => handlers.onOpenArchive({ console: parentConsole.id, consoleSession: consoleSession.id });
  const newSessionLabel = t("sidebar.focus.newSession");
  // The groups are ordered by how urgent their sessions are, which changes while the view is open.
  const groupsRef = useFlip<HTMLDivElement>();

  return (
    <>
      <FocusHeader handlers={handlers}>
        {/* The name selects the console session, whose terminal and report panel this view has no
            other way to reach. */}
        <TreeRow
          ariaLabel={sessionAriaLabel(t, language, consoleSession, accounts)}
          selected={consoleSession.id === selectedSessionId}
          onActivate={() => handlers.onSelectSession(consoleSession)}
          className="min-h-10 min-w-16 flex-1 items-center gap-2 px-2"
        >
          <BindingBadge owner={consoleSession} decorative />
          <div className="min-w-0 flex-1">
            <FocusTitle consoleName={parentConsole.name} title={consoleSession.title} />
          </div>
        </TreeRow>
        {projects.length === 0 ? (
          <RowIconButton icon={Plus} label={newSessionLabel} onPress={() => {}} isDisabled />
        ) : (
          // A session needs a project, and a console session with nothing bound yet has no group
          // to open one from, so the header offers every project.
          <ActionMenu
            label={newSessionLabel}
            tooltip={newSessionLabel}
            trigger={<Plus aria-hidden="true" className="size-4" />}
            items={sortProjects(projects, () => []).map((project) => ({
              label: project.name,
              icon: FolderOpen,
              onClick: () => openSession(project),
            }))}
          />
        )}
        <ActionMenu
          label={t("sidebar.session.actions", { title: consoleSession.title })}
          items={sessionMenu(t, handlers, consoleSession, accounts, { inFocus: true })}
        />
      </FocusHeader>
      <ScrollShadow size={24} className="min-h-0 flex-1 px-2 pb-2">
        <SectionHeading>{t("sidebar.focus.sessions", { count: liveCount })}</SectionHeading>
        {projects.length === 0 ? (
          // The header's "+" is disabled with no project to open a session in, so say why here.
          <EmptyPanel
            icon={FolderOpen}
            message={t("sidebar.noProjects")}
            action={{ label: t("sidebar.console.addProject"), icon: FolderPlus, onPress: () => handlers.onOpenDialog({ kind: "new-project", console: parentConsole }) }}
          />
        ) : groups.length === 0 ? (
          <EmptyPanel icon={MessageSquarePlus} message={t("sidebar.focus.noBoundSessions")} />
        ) : (
          <div ref={groupsRef} className="relative flex flex-col gap-2">
            {groups.map(({ project, sessions: boundHere }) => (
              <div key={project.id} data-flip={project.id}>
                <FocusProjectGroup
                  handlers={handlers}
                  project={project}
                  boundHere={boundHere}
                  archived={archivedSessions(sessions.filter((s) => s.project_id === project.id))}
                  onOpenSession={() => openSession(project)}
                  selectedSessionId={selectedSessionId}
                />
              </div>
            ))}
          </div>
        )}
        <ArchivedList handlers={handlers} archived={archived} selectedSessionId={selectedSessionId} onViewAll={viewAll} />
      </ScrollShadow>
    </>
  );
}

/** One project of a console session's focus mode: its name with its own new-session button and
 * actions, over the cards of the sessions bound to the console session. `archived` is all the
 * project's archived sessions, for its "View archive" submenu. */
function FocusProjectGroup({
  handlers,
  project,
  boundHere,
  archived,
  onOpenSession,
  selectedSessionId,
}: {
  handlers: SidebarHandlers;
  project: Project;
  boundHere: Session[];
  archived: Session[];
  onOpenSession: () => void;
  selectedSessionId?: string;
}): React.ReactElement {
  const t = useT();
  return (
    <>
      <div className="mb-1 flex min-h-7 items-center gap-1 ps-2">
        <h4 className="min-w-0 flex-1">
          <RowLabel title={project.name} className="block min-w-0">
            <span className="text-sm font-medium">{project.name}</span>
          </RowLabel>
        </h4>
        {project.pinned && <Pin aria-hidden="true" className="size-3 shrink-0 text-muted" />}
        {/* Named for its project: the header's "+" is beside it in the same view. */}
        <RowIconButton icon={Plus} label={t("sidebar.focus.newSessionIn", { name: project.name })} onPress={onOpenSession} />
        <ActionMenu
          label={t("sidebar.project.actions", { name: project.name })}
          items={projectMenu(t, handlers, project, archived, { inFocus: true })}
        />
      </div>
      <SessionCards handlers={handlers} sessions={boundHere} selectedSessionId={selectedSessionId} />
    </>
  );
}

/** The one line a project's focus mode carries for the sessions it leaves out: how many are bound
 * to which console sessions, each named console session leading to its own focus mode. */
function BoundElsewhere({
  handlers,
  count,
  owners,
}: {
  handlers: SidebarHandlers;
  count: number;
  owners: Session[];
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  let next = 0;
  // The list's own punctuation and joining word, per language, around the names as buttons. Known
  // and accepted: `Intl.ListFormat` joins zh-Hans names with 和 and no spaces, where the glossary
  // wants a half-width space beside Latin text. Mending it by joining translated fragments here
  // would break the rule that a sentence is one message.
  const names = new Intl.ListFormat(language, { style: "long", type: "conjunction" })
    .formatToParts(owners.map((owner) => owner.title))
    .map((part, index) => {
      if (part.type === "literal") return <React.Fragment key={index}>{part.value}</React.Fragment>;
      const owner = owners[next++];
      return (
        <Button
          key={index}
          size="sm"
          variant="ghost"
          preventFocusOnPress
          aria-label={t("sidebar.focus.enterConsoleSession", { name: owner.title })}
          onPress={() => handlers.onFocus({ consoleSession: owner })}
          className="h-auto min-h-0 min-w-0 max-w-full gap-1 px-1 py-0 align-baseline text-xs font-medium"
        >
          <BindingBadge owner={owner} decorative />
          {/* A name longer than the line is cut rather than pushed past the sidebar's edge. */}
          <FadeOverflow as="span" dir="auto" className="min-w-0" titleWhenClipped={owner.title}>
            {part.value}
          </FadeOverflow>
        </Button>
      );
    });
  return (
    <p className="px-2 pb-1 text-xs text-muted">
      <Message id="sidebar.focus.boundElsewhere" params={{ count, owners: names }} />
    </p>
  );
}

/** A focus mode's sessions as cards, in the order given. */
function SessionCards({
  handlers,
  sessions,
  selectedSessionId,
}: {
  handlers: SidebarHandlers;
  sessions: Session[];
  selectedSessionId?: string;
}): React.ReactElement {
  const listRef = useFlip<HTMLDivElement>();
  return (
    <div ref={listRef} className="relative flex flex-col gap-2">
      {sessions.map((session) => (
        <div key={session.id} data-flip={session.id}>
          <SessionCard handlers={handlers} session={session} selected={session.id === selectedSessionId} />
        </div>
      ))}
    </div>
  );
}

/** A focus mode's archive: its ten most recent rows, with "View all" to the archive view. */
function ArchivedList({
  handlers,
  archived,
  selectedSessionId,
  onViewAll,
}: {
  handlers: SidebarHandlers;
  archived: Session[];
  selectedSessionId?: string;
  onViewAll: () => void;
}): React.ReactElement {
  const t = useT();
  return (
    <>
      <SectionHeading>{t("sidebar.focus.archived", { count: archived.length })}</SectionHeading>
      {archived.length === 0 ? (
        <EmptyPanel compact icon={Archive} message={t("sidebar.archive.empty")} />
      ) : (
        <>
          <div className="flex flex-col gap-0.5">
            {archived.slice(0, ARCHIVE_PREVIEW).map((session) => (
              <ArchivedRow key={session.id} handlers={handlers} session={session} selected={session.id === selectedSessionId} />
            ))}
          </div>
          <Button size="sm" variant="ghost" fullWidth preventFocusOnPress onPress={onViewAll} className="mt-1 justify-start font-normal text-muted hover:text-foreground">
            <List aria-hidden="true" className="size-4" />
            {t("sidebar.archive.viewAll", { count: archived.length })}
          </Button>
        </>
      )}
    </>
  );
}

/** A session in focus mode: its status put into words, its agent and account, its title over two
 * lines, and when it started. No binding badge: a focus mode lists either only unbound sessions or
 * only those bound to the one console session it is for. */
function SessionCard({
  handlers,
  session,
  selected,
}: {
  handlers: SidebarHandlers;
  session: Session;
  selected: boolean;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  return (
    <TreeRow
      ariaLabel={sessionAriaLabel(t, language, session, accounts)}
      selected={selected}
      onActivate={() => handlers.onSelectSession(session)}
      className="min-h-8 flex-col gap-1.5 border border-separator bg-background p-3 data-selected:border-accent"
    >
      <div className="flex items-center gap-2">
        <StatusIcon status={session.status} decorative />
        <span className="text-xs font-medium text-muted">{statusLabel(t, session.status)}</span>
        <span className="flex-1" />
        {session.pinned && <Pin aria-hidden="true" className="size-3 shrink-0 text-muted" />}
        <RowControls always>
          <ActionMenu label={t("sidebar.session.actions", { title: session.title })} items={sessionMenu(t, handlers, session, accounts)} />
        </RowControls>
      </div>
      <div dir="auto" className="line-clamp-2 text-sm font-medium break-words">
        {session.title}
      </div>
      <div className="flex min-w-0 items-center gap-1.5 text-xs text-muted">
        <AgentIcon agent={session.agent} className="size-3.5" />
        <FadeOverflow
          as="span"
          className="min-w-0"
          titleWhenClipped={sessionAgentLabel(t, session, accounts)}
        >
          <AgentAccountText agent={AGENT_LABEL[session.agent]} account={sessionAccountName(t, session, accounts)} />
        </FadeOverflow>
        <span aria-hidden="true">·</span>
        <span className="shrink-0">{formatRelativeTime(language, session.started_at)}</span>
      </div>
    </TreeRow>
  );
}

function ArchivedRow({
  handlers,
  session,
  selected,
}: {
  handlers: SidebarHandlers;
  session: Session;
  selected: boolean;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  return (
    <TreeRow
      ariaLabel={sessionAriaLabel(t, language, session, accounts)}
      selected={selected}
      onActivate={() => handlers.onSelectSession(session)}
    >
      <AgentIcon agent={session.agent} />
      <RowLabel title={session.title}>
        <span className="text-muted">{session.title}</span>
      </RowLabel>
      <span className="shrink-0 text-xs text-muted">
        {formatRelativeTime(language, session.ended_at ?? session.started_at)}
      </span>
      <RowControls>
        <ActionMenu label={t("sidebar.session.actions", { title: session.title })} items={sessionMenu(t, handlers, session, accounts)} />
      </RowControls>
    </TreeRow>
  );
}
