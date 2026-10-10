import { Button, ScrollShadow } from "@heroui/react";
import { ArrowLeft, FolderOpen, FolderPlus, List, MessageSquarePlus, Plus } from "lucide-react";
import React, { useLayoutEffect, useRef } from "react";

import { AGENT_LABEL } from "../agents";
import { ActionMenu } from "../components/ActionMenu";
import { AgentAccountText } from "../components/AgentAccountText";
import { AgentIcon } from "../components/AgentIcon";
import { EmptyPanel } from "../components/EmptyPanel";
import { FadeOverflow } from "../components/FadeOverflow";
import { ActivityMarker, StatusIcon } from "../components/StatusIcon";
import { TitledControl } from "../components/TitledControl";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { Console, Project, Session } from "../protocol";
import { formatRelativeTime } from "../relativeTime";
import { activityLabelKey, sessionAccountName, sessionAccountTooltip, sessionAgentLabel, sessionAriaLabel, statusLabel } from "../sessionLabel";
import { useDaemonStore } from "../store";
import { BindingBadge } from "./BindingBadge";
import { GitBadge } from "./GitBadge";
import { projectMenu, sessionMenu } from "./menus";
import { archivedSessionRows, archivedSessions, boundArchivedSessions, boundElsewhere, focusGroups, liveSessionRows, type NestedSession, notUnderConsoleSession, sortProjects, switchStrip, type SwitchStripEntry } from "./order";
import { NESTED_ROW_CLASS, PinButton, RowControls, RowIconButton, RowLabel, SectionHeading, TreeRow } from "./rows";
import type { SidebarHandlers } from "./types";
import { useFlip } from "./useFlip";

/** How many archived sessions focus mode lists before "View all". */
const ARCHIVE_PREVIEW = 10;

/** The top of either focus view: the back button that leaves focus mode, then whatever the view
 * puts after it. Both views are built from this header, the session cards and the archived list
 * below, so they differ only in what they are given. */
function FocusHeader({
  handlers,
  children,
  ref,
}: {
  handlers: SidebarHandlers;
  children: React.ReactNode;
  /** The header's element, so its action menu can open on a right-click anywhere on the header. */
  ref?: React.Ref<HTMLDivElement>;
}): React.ReactElement {
  const t = useT();
  return (
    // A row or card below sits in the scroll area (`px-2`), then inside its own padding (a card's
    // border and padding add up to `px-2`), so the end padding matches that and this header's
    // trailing menus line up with the menu on a row or card. The start stays put.
    <div ref={ref} className="flex h-14 shrink-0 items-center gap-1 border-b border-separator ps-2 pe-4">
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

/** A project's focus mode: the sidebar given over to one project, its sessions not bound to a
 * console session as cards, each with the sessions bound to it under it, its recent archive below
 * them and, last, the console sessions that hold the rest ("A project's focus mode" in
 * docs/product/focus-mode.md). A lead session and its team are left out of the list, with the
 * sessions bound to a console session, and summed up by a sentence with a chip for each console
 * session, leading to its own focus mode; the archive, bound sessions included, is not filtered. */
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
  const live = liveSessionRows(notUnderConsoleSession(sessions, owners));
  // The rows nest an archived session under its archived owner; the project menu's "View archive"
  // submenu lists the newest few flat, so it is given the plain list.
  const archived = archivedSessions(sessions);
  const archivedRows = archivedSessionRows(sessions);
  const elsewhere = boundElsewhere(sessions, owners);
  const openSession = () =>
    handlers.onOpenDialog({ kind: "new-session", console: parentConsole, project, binding: { kind: "unbound" } });
  const viewAll = () => handlers.onOpenArchive({ console: project.console_id, project: project.id });
  const headerRef = useRef<HTMLDivElement>(null);

  return (
    <>
      <FocusHeader handlers={handlers} ref={headerRef}>
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
          items={projectMenu(t, handlers, project, archived, { placement: "focused", gitRepository: gitStatus?.repository })}
          contextTargetRef={headerRef}
        />
      </FocusHeader>
      <ScrollShadow size={24} className="min-h-0 flex-1 px-2 pb-2">
        <SectionHeading>{t("sidebar.focus.sessions", { count: live.length })}</SectionHeading>
        {live.length === 0 ? (
          <EmptyPanel
            icon={MessageSquarePlus}
            message={t("sidebar.noSessions")}
            action={{ label: t("sidebar.project.openSession"), icon: Plus, onPress: openSession }}
          />
        ) : (
          <SessionCards handlers={handlers} sessions={live} selectedSessionId={selectedSessionId} />
        )}
        <ArchivedList handlers={handlers} archived={archivedRows} selectedSessionId={selectedSessionId} onViewAll={viewAll} />
        {elsewhere && <BoundElsewhere handlers={handlers} count={elsewhere.count} owners={elsewhere.owners} />}
      </ScrollShadow>
    </>
  );
}

/** A console session's focus mode: the sidebar given over to the projects that have a session
 * bound to it and, within each, only those sessions as cards — a lead session among them carrying
 * its own sessions under it — with its archived bound sessions below. A session opened from here
 * is bound to it, with no choice offered. What has no meaning here is left out: a project's own
 * focus mode (`projectMenu`'s `nested` placement on the project headings here) and the project
 * list's filter. */
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
  const archived = archivedSessionRows(boundArchivedSessions(sessions, consoleSession.id));
  const openSession = (project: Project) =>
    handlers.onOpenDialog({ kind: "new-session", console: parentConsole, project, binding: { kind: "bound", to: consoleSession } });
  const viewAll = () => handlers.onOpenArchive({ console: parentConsole.id, consoleSession: consoleSession.id });
  const newSessionLabel = t("sidebar.focus.newSession");
  const strip = switchStrip(sessions, parentConsole.id);
  // The groups are ordered by how urgent their sessions are, which changes while the view is open.
  const groupsRef = useFlip<HTMLDivElement>();
  const headerRef = useRef<HTMLDivElement>(null);

  return (
    <>
      <FocusHeader handlers={handlers} ref={headerRef}>
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
          items={sessionMenu(t, handlers, consoleSession, accounts, { placement: "focused" })}
          contextTargetRef={headerRef}
        />
      </FocusHeader>
      {strip.length > 1 && <SwitchStrip handlers={handlers} entries={strip} currentId={consoleSession.id} />}
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

/** The console's console sessions, one chip each, the current one marked, to see what is going on in
 * them and to move between them without leaving focus mode: a chip enters that console session's
 * focus mode and selects it. Not drawn for a console with a single console session, since there is
 * nothing to switch to. A row that does not fit scrolls sideways under an edge fade, with no
 * scrollbar, and brings the current chip into view when the view opens or the chip moves. */
function SwitchStrip({
  handlers,
  entries,
  currentId,
}: {
  handlers: SidebarHandlers;
  entries: SwitchStripEntry[];
  currentId: string;
}): React.ReactElement {
  const t = useT();
  const ref = useRef<HTMLDivElement>(null);
  // Pinning or unpinning reorders the strip, which moves the current chip without changing it.
  const currentIndex = entries.findIndex((entry) => entry.consoleSession.id === currentId);
  // Scrolls the strip's own `scrollLeft`, so no ancestor scrolls with it. Moved by the distance
  // between the chip's centre and the strip's, which holds in a right-to-left layout too, where
  // `scrollLeft` is negative.
  useLayoutEffect(() => {
    const scroller = ref.current;
    const chip = scroller?.querySelector<HTMLElement>("[data-switch-current]");
    if (!scroller || !chip) return;
    const from = scroller.getBoundingClientRect();
    const box = chip.getBoundingClientRect();
    scroller.scrollLeft += box.left + box.width / 2 - (from.left + from.width / 2);
  }, [currentId, currentIndex]);
  return (
    <ScrollShadow
      ref={ref}
      orientation="horizontal"
      hideScrollBar
      size={24}
      role="group"
      aria-label={t("sidebar.focus.switchStrip")}
      className="flex shrink-0 gap-1 px-2 py-2"
    >
      {entries.map(({ consoleSession, activity }) => {
        const current = consoleSession.id === currentId;
        const activityKey = activityLabelKey(activity);
        return (
          <Button
            key={consoleSession.id}
            size="sm"
            variant="tertiary"
            preventFocusOnPress
            aria-label={activityKey ? t(activityKey, { name: consoleSession.title }) : consoleSession.title}
            aria-current={current ? "true" : undefined}
            data-switch-current={current || undefined}
            onPress={() => handlers.onSwitchConsoleSession(consoleSession)}
            // Pill-shaped, so a chip reads as a tag rather than as one of the sidebar's buttons. The
            // current one is marked by the selected card's border alone, so it keeps the tertiary
            // fill's hover and press feedback; the others have a transparent border so that nothing
            // shifts when it changes.
            className="h-7 min-h-0 min-w-0 max-w-48 shrink-0 gap-1.5 rounded-full border border-transparent px-2.5 text-xs font-medium data-switch-current:border-accent-glyph"
          >
            <BindingBadge owner={consoleSession} decorative />
            <FadeOverflow as="span" dir="auto" className="min-w-0" titleWhenClipped={consoleSession.title}>
              {consoleSession.title}
            </FadeOverflow>
            <ActivityMarker activity={activity} />
          </Button>
        );
      })}
    </ScrollShadow>
  );
}

/** One project of a console session's focus mode: its name with its own new-session button and
 * actions, over the cards of the sessions bound to the console session, each lead session among
 * them carrying its own sessions under it. `archived` is all the project's archived sessions, for
 * its "View archive" submenu. */
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
  boundHere: NestedSession[];
  archived: Session[];
  onOpenSession: () => void;
  selectedSessionId?: string;
}): React.ReactElement {
  const t = useT();
  // Read as `ProjectFocusView` does, for the "Sync repository" item.
  const gitStatus = useDaemonStore((s) => s.gitStatuses.get(project.id));
  const headerRef = useRef<HTMLDivElement>(null);
  // Where focus goes when the pin button, ahead of it, is unpinned away.
  const openSessionHolder = useRef<HTMLDivElement>(null);
  return (
    <>
      <div ref={headerRef} className="mb-1 flex min-h-7 items-center gap-1 ps-2 pe-2">
        <h4 className="min-w-0 flex-1">
          <RowLabel title={project.name} className="block min-w-0">
            <span className="text-sm font-medium">{project.name}</span>
          </RowLabel>
        </h4>
        {project.pinned && <PinButton name={project.name} onUnpin={() => handlers.onSetPinned({ project }, false)} returnFocusTo={openSessionHolder} />}
        {/* Named for its project: the header's "+" is beside it in the same view. */}
        <RowIconButton ref={openSessionHolder} icon={Plus} label={t("sidebar.focus.newSessionIn", { name: project.name })} onPress={onOpenSession} />
        <ActionMenu
          label={t("sidebar.project.actions", { name: project.name })}
          items={projectMenu(t, handlers, project, archived, { placement: "nested", gitRepository: gitStatus?.repository })}
          contextTargetRef={headerRef}
        />
      </div>
      <SessionCards handlers={handlers} sessions={boundHere} selectedSessionId={selectedSessionId} />
    </>
  );
}

/** What a project's focus mode carries for the sessions it leaves out: a line saying how many are
 * under console sessions, over a chip for each console session, with how many of the project's
 * sessions are under it, leading to its own focus mode. A lead session's own sessions are counted
 * with it, as they are listed with it there. */
function BoundElsewhere({
  handlers,
  count,
  owners,
}: {
  handlers: SidebarHandlers;
  count: number;
  owners: { owner: Session; count: number }[];
}): React.ReactElement {
  const t = useT();
  return (
    <div className="px-2 pt-3 pb-2">
      <p className="text-xs text-muted">
        {t("sidebar.focus.boundElsewhere", { count })}
      </p>
      <div className="mt-1 flex flex-wrap gap-1">
        {owners.map(({ owner, count: bound }) => (
          <Button
            key={owner.id}
            size="sm"
            variant="tertiary"
            preventFocusOnPress
            aria-label={t("sidebar.focus.enterConsoleSession", { name: owner.title, count: bound })}
            onPress={() => handlers.onFocus({ consoleSession: owner })}
            // Pill-shaped on purpose, on the sidebar's tinted tertiary fill (`.sidebar-fills`): a chip
            // reads as a tag, not as one of the sidebar's buttons.
            className="h-6 min-h-0 min-w-0 max-w-full gap-1 rounded-full px-2 text-xs font-medium"
          >
            <BindingBadge owner={owner} decorative />
            {/* A name longer than the line is cut rather than pushed past the sidebar's edge. */}
            <FadeOverflow as="span" dir="auto" className="min-w-0" titleWhenClipped={owner.title}>
              {owner.title}
            </FadeOverflow>
            <span aria-hidden="true" className="shrink-0 font-normal">
              · {bound}
            </span>
          </Button>
        ))}
      </div>
    </div>
  );
}

/** A focus mode's sessions as cards, in the order given, a session bound to a project session inset
 * under its owner. One flat list rather than a list nested inside the owner's card: the reorder
 * animation reads the list's direct children (`useFlip`), so a level of its own would leave a team
 * unanimated while its statuses change. Cards sit apart from one another, so the nesting is the
 * inset alone — the same one as `NESTED_ROW_CLASS`, without the line that would break at every
 * gap between them. */
function SessionCards({
  handlers,
  sessions,
  selectedSessionId,
}: {
  handlers: SidebarHandlers;
  sessions: NestedSession[];
  selectedSessionId?: string;
}): React.ReactElement {
  const listRef = useFlip<HTMLDivElement>();
  return (
    <div ref={listRef} className="relative flex flex-col gap-2">
      {sessions.map(({ session, under }) => (
        <div key={session.id} data-flip={session.id} className={under ? "ms-4" : undefined}>
          <SessionCard handlers={handlers} session={session} under={under} selected={session.id === selectedSessionId} />
        </div>
      ))}
    </div>
  );
}

/** A focus mode's archive: its ten most recent rows, with "View all" to the archive view, an
 * archived session bound to an archived project session inset under it. The rows are already flat
 * and in display order, so the ten are counted as rows: the preview can end between an owner and
 * its team, but never shows an inset row without the row it is inset under. Not drawn while there
 * is none. */
function ArchivedList({
  handlers,
  archived,
  selectedSessionId,
  onViewAll,
}: {
  handlers: SidebarHandlers;
  archived: NestedSession[];
  selectedSessionId?: string;
  onViewAll: () => void;
}): React.ReactElement | null {
  const t = useT();
  if (archived.length === 0) return null;
  return (
    <>
      <SectionHeading>{t("sidebar.focus.archived", { count: archived.length })}</SectionHeading>
      <div className="flex flex-col gap-0.5">
        {archived.slice(0, ARCHIVE_PREVIEW).map(({ session, under }) => (
          <div key={session.id} className={under ? NESTED_ROW_CLASS : undefined}>
            <ArchivedRow handlers={handlers} session={session} under={under} selected={session.id === selectedSessionId} />
          </div>
        ))}
      </div>
      <Button size="sm" variant="ghost" fullWidth preventFocusOnPress onPress={onViewAll} className="mt-1 justify-start font-normal text-muted hover:text-foreground">
        <List aria-hidden="true" className="size-4" />
        {t("sidebar.archive.viewAll", { count: archived.length })}
      </Button>
    </>
  );
}

/** A session in focus mode: its status put into words, its agent and account, its title over two
 * lines, and when it started. No binding badge: a focus mode lists either only the sessions no
 * console session is above or only those under the one console session it is for. `under`, the
 * project session this one is bound to and is inset below, goes into the card's accessible name,
 * which is what carries that nesting to assistive technology. */
function SessionCard({
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
  const t = useT();
  const language = useCurrentLanguage();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const rowRef = useRef<HTMLDivElement>(null);
  return (
    <TreeRow
      ref={rowRef}
      ariaLabel={sessionAriaLabel(t, language, session, accounts, under)}
      selected={selected}
      onActivate={() => handlers.onSelectSession(session)}
      className="min-h-8 flex-col gap-1 border border-separator p-[7px] data-selected:border-accent-glyph"
    >
      <div className="flex items-center gap-2">
        <StatusIcon status={session.status} decorative />
        <span className="text-xs font-medium text-muted">{statusLabel(t, session.status)}</span>
        <span className="flex-1" />
        {session.pinned && <PinButton name={session.title} onUnpin={() => handlers.onSetPinned({ session }, false)} returnFocusTo={rowRef} />}
        <RowControls always>
          <ActionMenu label={t("sidebar.session.actions", { title: session.title })} items={sessionMenu(t, handlers, session, accounts)} contextTargetRef={rowRef} />
        </RowControls>
      </div>
      <div dir="auto" className="line-clamp-2 text-sm font-medium break-words">
        {session.title}
      </div>
      <div className="flex min-w-0 items-center gap-1.5 text-xs text-muted">
        <AgentIcon agent={session.agent} className="size-3.5" tooltip={sessionAccountTooltip(t, session, accounts)} />
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

/** One row of a focus mode's archive. `under`, the archived project session this one is bound to
 * and is inset below, goes into the row's accessible name, as on a session card. */
function ArchivedRow({
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
  const t = useT();
  const language = useCurrentLanguage();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const rowRef = useRef<HTMLDivElement>(null);
  return (
    <TreeRow
      ref={rowRef}
      ariaLabel={sessionAriaLabel(t, language, session, accounts, under)}
      selected={selected}
      onActivate={() => handlers.onSelectSession(session)}
    >
      <AgentIcon agent={session.agent} tooltip={sessionAccountTooltip(t, session, accounts)} />
      <RowLabel title={session.title}>
        <span className="text-muted">{session.title}</span>
      </RowLabel>
      <span className="shrink-0 text-xs text-muted">
        {formatRelativeTime(language, session.ended_at ?? session.started_at)}
      </span>
      <RowControls>
        <ActionMenu label={t("sidebar.session.actions", { title: session.title })} items={sessionMenu(t, handlers, session, accounts)} contextTargetRef={rowRef} />
      </RowControls>
    </TreeRow>
  );
}
