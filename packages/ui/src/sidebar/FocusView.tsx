import { Badge, Button, Switch, Tooltip } from "@heroui/react";
import { ArrowLeft, FolderOpen, FolderPlus, List, MessageSquarePlus, Plus } from "lucide-react";
import React, { useLayoutEffect, useRef } from "react";

import { AGENT_LABEL } from "../agents";
import { ActionMenu } from "../components/ActionMenu";
import { ActivityMarker } from "../components/ActivityMarker";
import { AgentAccountText } from "../components/AgentAccountText";
import { AgentIcon } from "../components/AgentIcon";
import { EmptyPanel } from "../components/EmptyPanel";
import { FadeOverflow } from "../components/FadeOverflow";
import { RowControls, RowIconGroup, RowIconLine, RowIconSlot } from "../components/RowControls";
import { StatusIcon } from "../components/StatusIcon";
import { TitledControl } from "../components/TitledControl";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { FadedScroller } from "../components/scrollFade";
import { isolate } from "../i18n/isolate";
import { joinPhrases } from "../i18n/joinPhrases";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { Console, Project, Session } from "../protocol";
import { formatRelativeTime } from "../relativeTime";
import { activityLabelKey, sessionAccountName, sessionAccountTooltip, sessionAgentLabel, sessionAriaLabel, sessionDisplayTitle, statusLabel } from "../sessionLabel";
import { useDaemonStore } from "../store";
import { BindingBadge } from "./BindingBadge";
import { GitBadge } from "./GitBadge";
import { projectMenu, sessionMenu } from "./menus";
import { archivePreviewSessions, archivedSessions, boundArchivedSessions, boundElsewhere, consoleSessionAbove, focusGroups, liveSessionRows, type NestedSession, notUnderConsoleSession, sortProjects, switchStrip, type SwitchStripEntry } from "./order";
import { projectFocusIncludeConsoleSessions } from "./projectFocusPreference";
import { NESTED_INSET_CLASS, PinButton, RowIconButton, RowLabel, SectionHeading, TreeRow } from "./rows";
import type { SidebarHandlers } from "./types";
import { useFlip } from "./useFlip";

const FOCUS_HEADING_HEIGHT = 38;

/** Kept inside its own section so the next heading pushes it away without covering its text. */
function FocusSectionHeading({ children, action }: { children: React.ReactNode; action?: React.ReactNode }): React.ReactElement {
  return (
    <div className="sticky top-0 z-3 flow-root bg-panel-sidebar">
      <SectionHeading action={action}>{children}</SectionHeading>
    </div>
  );
}

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
    <div ref={ref} className="flex h-(--pane-header-height) shrink-0 items-center gap-1 border-b border-separator ps-2 pe-4">
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

/** What a focus view's header names, under the console's name. The name is the view's own heading,
 * so what the view holds reads as sections of it: the sections below are a level under it, and the
 * project groups inside those a level under them again. */
function FocusTitle({ consoleName, title }: { consoleName: string; title: string }): React.ReactElement {
  return (
    <>
      <FadeOverflow className="text-xs text-app-muted" dir="auto" titleWhenClipped={consoleName}>
        {consoleName}
      </FadeOverflow>
      <h2 className="min-w-0">
        <RowLabel title={title} className="block min-w-0">
          <span className="text-sm font-semibold">{title}</span>
        </RowLabel>
      </h2>
    </>
  );
}

/** A project's focus mode: the sidebar given over to one project, its sessions not bound to a
 * console session as cards, each with the sessions bound to it under it, then the console sessions
 * that hold the rest and its recent archive. The optional combined view lists every live session
 * with its console session colour instead of the summary chips; its archive scope stays the same. */
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
  const includeConsoleSessions = projectFocusIncludeConsoleSessions.useValue();
  const live = liveSessionRows(includeConsoleSessions ? sessions : notUnderConsoleSession(sessions, owners));
  const byId = new Map(sessions.map((session) => [session.id, session]));
  const consoleOwner = (session: Session) => consoleSessionAbove(session, (id) => byId.get(id) ?? owners.get(id));
  const archived = archivedSessions(sessions);
  const archiveContext = [...sessions, ...owners.values()];
  const elsewhere = boundElsewhere(sessions, owners);
  const openSession = () =>
    handlers.onOpenDialog({ kind: "new-session", console: parentConsole, project, binding: { kind: "unbound" } });
  const viewAll = () => handlers.onOpenArchive({ console: project.console_id, project: project.id });
  const headerRef = useRef<HTMLDivElement>(null);
  const switchFocus = useFocusHandoff(() => headerRef.current?.querySelector("button")?.focus());

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
        <RowIconGroup>
          <RowIconButton icon={Plus} label={t("sidebar.project.openSession")} onPress={openSession} />
          <ActionMenu
            label={t("sidebar.project.actions", { name: isolate(project.name) })}
            items={projectMenu(t, handlers, project, archived, archiveContext, { placement: "focused", gitRepository: gitStatus?.repository })}
            contextTargetRef={headerRef}
          />
        </RowIconGroup>
      </FocusHeader>
      <FadedScroller stickyHeadingHeight={FOCUS_HEADING_HEIGHT} className="min-h-0 flex-1 px-2 pb-2">
        <section>
          <FocusSectionHeading
            action={elsewhere && (
              <span className="contents" {...switchFocus}>
                <Tooltip>
                  {/* Switch sends hover props to its hidden input; this trigger covers the visible control without adding a tab stop. */}
                  <Tooltip.Trigger role="presentation" tabIndex={-1}>
                    <Switch
                      size="sm"
                      aria-label={t("sidebar.focus.includeConsoleSessions")}
                      aria-describedby=""
                      isSelected={includeConsoleSessions}
                      onChange={(value) => projectFocusIncludeConsoleSessions.set(value)}
                    >
                      <Switch.Content>
                        <Switch.Control>
                          <Switch.Thumb />
                        </Switch.Control>
                      </Switch.Content>
                    </Switch>
                  </Tooltip.Trigger>
                  <Tooltip.Content className="pointer-events-none">{t("sidebar.focus.includeConsoleSessions")}</Tooltip.Content>
                </Tooltip>
              </span>
            )}
          >
            {t("sidebar.focus.sessions", { count: live.length })}
          </FocusSectionHeading>
          {live.length === 0 ? (
            <EmptyPanel
              compact
              icon={MessageSquarePlus}
              message={t("sidebar.noSessions")}
              action={{ label: t("sidebar.project.openSession"), onPress: openSession }}
            />
          ) : (
            <SessionCards handlers={handlers} sessions={live} selectedSessionId={selectedSessionId} consoleOwner={includeConsoleSessions ? consoleOwner : undefined} />
          )}
          {!includeConsoleSessions && elsewhere && <BoundElsewhere handlers={handlers} count={elsewhere.count} owners={elsewhere.owners} />}
        </section>
        <ArchivedList handlers={handlers} archived={archived} sessions={archiveContext} selectedSessionId={selectedSessionId} onViewAll={viewAll} />
      </FadedScroller>
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
  const archived = boundArchivedSessions(sessions, consoleSession.id);
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
            <FocusTitle consoleName={parentConsole.name} title={sessionDisplayTitle(t, consoleSession)} />
          </div>
        </TreeRow>
        <RowIconGroup>
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
            label={t("sidebar.session.actions", { title: isolate(sessionDisplayTitle(t, consoleSession)) })}
            items={sessionMenu(t, handlers, consoleSession, accounts, { placement: "focused" })}
            contextTargetRef={headerRef}
          />
        </RowIconGroup>
      </FocusHeader>
      {strip.length > 1 && <SwitchStrip handlers={handlers} entries={strip} currentId={consoleSession.id} />}
      <FadedScroller stickyHeadingHeight={FOCUS_HEADING_HEIGHT} className="min-h-0 flex-1 px-2 pb-2">
        <section>
          <FocusSectionHeading>{t("sidebar.focus.sessions", { count: liveCount })}</FocusSectionHeading>
          {projects.length === 0 ? (
            // The header's "+" is disabled with no project to open a session in, so say why here.
            <EmptyPanel
              compact
              icon={FolderPlus}
              message={t("sidebar.noProjects")}
              action={{ label: t("sidebar.console.addProject"), onPress: () => handlers.onOpenDialog({ kind: "new-project", console: parentConsole }) }}
            />
          ) : groups.length === 0 ? (
            <EmptyPanel compact icon={MessageSquarePlus} message={t("sidebar.focus.noBoundSessions")} />
          ) : (
            <div ref={groupsRef} className="relative flex flex-col gap-2">
              {groups.map(({ project, sessions: boundHere }) => (
                <div key={project.id} data-flip={project.id}>
                  <FocusProjectGroup
                    handlers={handlers}
                    project={project}
                    boundHere={boundHere}
                    archived={archivedSessions(sessions.filter((s) => s.project_id === project.id))}
                    sessions={sessions}
                    onOpenSession={() => openSession(project)}
                    selectedSessionId={selectedSessionId}
                  />
                </div>
              ))}
            </div>
          )}
        </section>
        <ArchivedList handlers={handlers} archived={archived} sessions={sessions} selectedSessionId={selectedSessionId} onViewAll={viewAll} />
      </FadedScroller>
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
    <FadedScroller
      ref={ref}
      orientation="horizontal"
      hideScrollBar
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
            aria-label={activityKey ? t(activityKey, { name: isolate(sessionDisplayTitle(t, consoleSession)) }) : sessionDisplayTitle(t, consoleSession)}
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
            <FadeOverflow as="span" dir="auto" className="min-w-0" titleWhenClipped={sessionDisplayTitle(t, consoleSession)}>
              {sessionDisplayTitle(t, consoleSession)}
            </FadeOverflow>
            <ActivityMarker activity={activity} />
          </Button>
        );
      })}
    </FadedScroller>
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
  sessions,
  onOpenSession,
  selectedSessionId,
}: {
  handlers: SidebarHandlers;
  project: Project;
  boundHere: NestedSession[];
  archived: Session[];
  sessions: Session[];
  onOpenSession: () => void;
  selectedSessionId?: string;
}): React.ReactElement {
  const t = useT();
  // Read as `ProjectFocusView` does, for the Git submenu.
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
        <RowIconGroup>
          {project.pinned && <PinButton name={project.name} onUnpin={() => handlers.onSetPinned({ project }, false)} returnFocusTo={openSessionHolder} />}
          {/* Named for its project: the header's "+" is beside it in the same view. */}
          <RowIconButton ref={openSessionHolder} icon={Plus} label={t("sidebar.focus.newSessionIn", { name: isolate(project.name) })} onPress={onOpenSession} />
          <ActionMenu
            label={t("sidebar.project.actions", { name: isolate(project.name) })}
            items={projectMenu(t, handlers, project, archived, sessions, { placement: "nested", gitRepository: gitStatus?.repository })}
            contextTargetRef={headerRef}
          />
        </RowIconGroup>
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
      <p className="text-xs text-app-muted">
        {t("sidebar.focus.boundElsewhere", { count })}
      </p>
      <div className="mt-1 flex flex-wrap gap-1">
        {owners.map(({ owner, count: bound }) => (
          <Button
            key={owner.id}
            size="sm"
            variant="tertiary"
            preventFocusOnPress
            aria-label={t("sidebar.focus.enterConsoleSession", { name: isolate(sessionDisplayTitle(t, owner)), count: bound })}
            onPress={() => handlers.onFocus({ consoleSession: owner })}
            // Pill-shaped on purpose, on the sidebar's tinted tertiary fill (`.sidebar-fills`): a chip
            // reads as a tag, not as one of the sidebar's buttons.
            className="h-6 min-h-0 min-w-0 max-w-full gap-1 rounded-full px-2 text-xs font-medium"
          >
            <BindingBadge owner={owner} decorative />
            {/* A name longer than the line is cut rather than pushed past the sidebar's edge. */}
            <FadeOverflow as="span" dir="auto" className="min-w-0" titleWhenClipped={sessionDisplayTitle(t, owner)}>
              {sessionDisplayTitle(t, owner)}
            </FadeOverflow>
            <Badge
              size="sm"
              className="static transform-none"
              aria-hidden="true"
              style={{
                "--badge-bg": "color-mix(in oklab, var(--foreground) 22%, var(--panel-sidebar))",
                "--badge-fg": "var(--foreground)",
                "--badge-border": "transparent",
              } as React.CSSProperties}
            >
              <Badge.Label>{bound}</Badge.Label>
            </Badge>
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
 * inset alone (`NESTED_INSET_CLASS`), without the line that would break at every gap between
 * them. The moving wrapper supplies an opaque base beneath the card's translucent state fills,
 * so overlapping cards cannot show each other's text during a reorder. The list's stacking context
 * keeps the summary and archive from painting over cards moving beyond the list's new bounds. */
function SessionCards({
  handlers,
  sessions,
  selectedSessionId,
  consoleOwner,
}: {
  handlers: SidebarHandlers;
  sessions: NestedSession[];
  selectedSessionId?: string;
  consoleOwner?: (session: Session) => Session | undefined;
}): React.ReactElement {
  const listRef = useFlip<HTMLDivElement>();
  return (
    <div ref={listRef} className="relative z-1 flex flex-col gap-2">
      {sessions.map(({ session, under }) => (
        <div key={session.id} data-flip={session.id} className={`rounded-lg bg-panel ${under ? NESTED_INSET_CLASS : ""}`}>
          <SessionCard handlers={handlers} session={session} under={under} selected={session.id === selectedSessionId} consoleOwner={consoleOwner?.(session)} />
        </div>
      ))}
    </div>
  );
}

/** A focus mode's archive: the five newest archived parent sessions. The heading counts the full
 * archive, and "View all" keeps every omitted session reachable, including hidden children. */
function ArchivedList({
  handlers,
  archived,
  sessions,
  selectedSessionId,
  onViewAll,
}: {
  handlers: SidebarHandlers;
  archived: Session[];
  sessions: Session[];
  selectedSessionId?: string;
  onViewAll: () => void;
}): React.ReactElement | null {
  const t = useT();
  if (archived.length === 0) return null;
  const preview = archivePreviewSessions(archived, sessions);
  return (
    <section>
      <FocusSectionHeading>{t("sidebar.focus.archived", { count: archived.length })}</FocusSectionHeading>
      <div className="flex flex-col gap-0.5">
        {preview.map((session) => (
          <ArchivedRow key={session.id} handlers={handlers} session={session} selected={session.id === selectedSessionId} />
        ))}
        {archived.length > preview.length && (
          <TreeRow ariaLabel={t("sidebar.archive.viewAll")} onActivate={onViewAll}>
            <List aria-hidden="true" className="size-4 shrink-0 text-app-muted rtl:-scale-x-100" />
            <RowLabel>
              <span className="text-app-muted">{t("sidebar.archive.viewAll")}</span>
            </RowLabel>
          </TreeRow>
        )}
      </div>
    </section>
  );
}

/** A session in focus mode: its status put into words, its agent and account, its title on one
 * fading line, and when it started. The combined project view also names and marks its console owner.
 * `under`, the project session this one is bound to and is inset below, goes into its accessible name,
 * which is what carries that nesting to assistive technology. */
function SessionCard({
  handlers,
  session,
  under,
  selected,
  consoleOwner,
}: {
  handlers: SidebarHandlers;
  session: Session;
  under?: Session;
  selected: boolean;
  consoleOwner?: Session;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const rowRef = useRef<HTMLDivElement>(null);
  const label = sessionAriaLabel(t, language, session, accounts, under ?? consoleOwner);
  return (
    <TreeRow
      ref={rowRef}
      ariaLabel={under && consoleOwner ? joinPhrases(language, [label, t("sidebar.session.boundTo", { name: isolate(sessionDisplayTitle(t, consoleOwner)) })]) : label}
      selected={selected}
      onActivate={() => handlers.onSelectSession(session)}
      className="min-h-8 flex-col gap-1 border border-separator p-[7px] data-selected:border-accent-glyph"
    >
      <RowIconLine icon={<StatusIcon status={session.status} decorative />}>
        <span className="text-xs font-medium text-app-muted">{statusLabel(t, session.status)}</span>
        <span className="flex-1" />
        <RowControls always fixed={session.pinned ? <PinButton name={sessionDisplayTitle(t, session)} onUnpin={() => handlers.onSetPinned({ session }, false)} returnFocusTo={rowRef} /> : undefined}>
          <ActionMenu label={t("sidebar.session.actions", { title: isolate(sessionDisplayTitle(t, session)) })} items={sessionMenu(t, handlers, session, accounts)} contextTargetRef={rowRef} />
        </RowControls>
      </RowIconLine>
      <div className="flex min-w-0 items-center gap-(--row-leading-icon-gap)">
        {consoleOwner?.colour && <RowIconSlot placement="leading"><BindingBadge owner={consoleOwner} /></RowIconSlot>}
        <FadeOverflow as="span" titleWhenClipped={sessionDisplayTitle(t, session)} className="min-w-0 flex-1 text-sm font-medium">
          <bdi>{sessionDisplayTitle(t, session)}</bdi>
        </FadeOverflow>
      </div>
      <RowIconLine
        icon={<AgentIcon agent={session.agent} className="size-3.5" tooltip={sessionAccountTooltip(t, session, accounts)} />}
        className="text-xs text-app-muted"
      >
        <FadeOverflow
          as="span"
          className="min-w-0 flex-1"
          titleWhenClipped={sessionAgentLabel(t, session, accounts)}
        >
          <AgentAccountText agent={AGENT_LABEL[session.agent]} account={sessionAccountName(t, session, accounts)} />
        </FadeOverflow>
        <span className="shrink-0">{formatRelativeTime(language, session.started_at)}</span>
      </RowIconLine>
    </TreeRow>
  );
}

/** One parent session in a focus mode's archive preview. */
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
  const rowRef = useRef<HTMLDivElement>(null);
  return (
    <TreeRow
      ref={rowRef}
      ariaLabel={sessionAriaLabel(t, language, session, accounts)}
      selected={selected}
      onActivate={() => handlers.onSelectSession(session)}
    >
      <AgentIcon agent={session.agent} tooltip={sessionAccountTooltip(t, session, accounts)} />
      <RowLabel title={sessionDisplayTitle(t, session)}>
        <span className="text-app-muted">{sessionDisplayTitle(t, session)}</span>
      </RowLabel>
      <span className="shrink-0 text-xs text-app-muted">
        {formatRelativeTime(language, session.ended_at ?? session.started_at)}
      </span>
      <RowControls>
        <ActionMenu label={t("sidebar.session.actions", { title: isolate(sessionDisplayTitle(t, session)) })} items={sessionMenu(t, handlers, session, accounts)} contextTargetRef={rowRef} />
      </RowControls>
    </TreeRow>
  );
}
