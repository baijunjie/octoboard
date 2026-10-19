import { Badge, Button, Switch, Tooltip } from "@heroui/react";
import { ArrowLeft, FolderPlus, List, MessageSquarePlus } from "lucide-react";
import React, { memo, useId, useLayoutEffect, useRef } from "react";

import { ActionMenu } from "../components/ActionMenu";
import { ActivityMarker } from "../components/ActivityMarker";
import { AgentIcon } from "../components/AgentIcon";
import { EmptyPanel } from "../components/EmptyPanel";
import { FadeOverflow } from "../components/FadeOverflow";
import { PaneTitle } from "../components/PaneTitle";
import { RowControls, RowIconGroup, RowIconLine, RowIconSlot } from "../components/RowControls";
import { hasVisibleUsage, SessionUsageLabel } from "../components/SessionMetadata";
import { StatusIcon } from "../components/StatusIcon";
import { TitledControl } from "../components/TitledControl";
import { useListMotion } from "../components/useListMotion";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { FadedScroller } from "../components/scrollFade";
import type { NewSessionRequest } from "../dialogs/dialogRequest";
import { isolate } from "../i18n/isolate";
import { joinPhrases } from "../i18n/joinPhrases";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { Console, Project, Session } from "../protocol";
import { formatRelativeTime } from "../relativeTime";
import { activityLabelKey, sessionAgentLabel, sessionAriaLabel, sessionDisplayTitle, sessionStatusKind, statusLabel } from "../sessionLabel";
import { inheritedLaunch, sessionModelEffort, sessionModelLabel } from "../sessionModel";
import { indexSessions, sessionsOf } from "../sessions/sessionIndex";
import { useHasBoundSessions, useProjectSessions, useSessionProjectId } from "../sessions/useSessions";
import { useDaemon, useDaemonStore, useDaemonStoreEqual } from "../store";
import { useNow } from "../useNow";
import { BindingBadge } from "./BindingBadge";
import { GitBadge } from "./GitBadge";
import { projectMenu } from "./menus";
import { NewSessionButton } from "./NewSessionButton";
import { NewSessionMenu } from "./NewSessionMenu";
import { SessionActions } from "./SessionActions";
import { archivePreviewSessions, archivedSessions, boundArchivedSessions, boundElsewhere, consoleSessionAbove, focusGroupRows, focusGroups, liveSessionRows, type NestedSession, notUnderConsoleSession, switchStrip, type SwitchStripEntry } from "./order";
import { projectFocusIncludeConsoleSessions } from "./projectFocusPreference";
import { NESTED_INSET_CLASS, LockButton, PinButton, RowLabel, STICKY_HEADING_HEIGHT, StickySectionHeading, TreeRow } from "./rows";
import type { SidebarHandlers } from "./types";
import { useFlip } from "./useFlip";

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
    // A row or card below sits in the scroll area (`--row-fill-inset`), then inside its own padding (a
    // card's border and padding add up to `--row-content-inset`), so the end padding matches that and
    // this header's trailing menus line up with the menu on a row or card. The start stays put.
    <div ref={ref} className="flex h-(--pane-header-height) shrink-0 items-center gap-1 groove-b ps-(--row-fill-inset) pe-(--pane-inset)">
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

/** What a focus view's header names, as its flexing item. A project's header gives the console's
 * name over the project's, the project's name being the view's own heading; a console session's
 * gives the console's name alone as the heading, the session's title being on the card below. What
 * the view holds reads as sections of that heading: the sections below are a level under it, and
 * the project groups inside those a level under them again.
 * `min-w-16` is the floor itself: this is the flex item (`flex-1`'s basis is 0%, so a floor on the
 * label inside `PaneTitle` would clamp nothing), leaving, in a project's header,
 * `GitBadge`'s branch track as the part that keeps giving way once this is reached. */
function FocusTitle({ heading, overline }: { heading: string; overline?: string }): React.ReactElement {
  return <PaneTitle className="min-w-16 flex-1 px-1" heading={heading} overline={overline} />;
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
  const newSession = (): NewSessionRequest => ({ kind: "new-session", console: parentConsole, project, binding: { kind: "unbound" } });
  const viewAll = () => handlers.onOpenArchive({ console: project.console_id, project: project.id });
  const headerRef = useRef<HTMLDivElement>(null);
  const switchFocus = useFocusHandoff(() => headerRef.current?.querySelector("button")?.focus());

  return (
    <>
      <FocusHeader handlers={handlers} ref={headerRef}>
        <FocusTitle heading={project.name} overline={parentConsole.name} />
        <GitBadge status={gitStatus} />
        <RowIconGroup>
          <NewSessionButton label={t("sidebar.project.openSession")} quickLabel={t("sidebar.project.openSessionQuick")} target={newSession} onOpen={handlers.onOpenDialog} onQuickCreate={handlers.onQuickCreateSession} />
          <ActionMenu
            label={t("sidebar.project.actions", { name: isolate(project.name) })}
            items={projectMenu(t, handlers, project, archived, archiveContext, { placement: "focused", gitRepository: gitStatus?.repository })}
            contextTargetRef={headerRef}
          />
        </RowIconGroup>
      </FocusHeader>
      <FadedScroller stickyHeadingHeight={STICKY_HEADING_HEIGHT} className="min-h-0 flex-1 px-(--row-fill-inset) pb-2">
        <section>
          <StickySectionHeading
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
          </StickySectionHeading>
          {live.length === 0 ? (
            <EmptyPanel
              compact
              icon={MessageSquarePlus}
              message={t("sidebar.noSessions")}
              action={{ label: t("sidebar.project.openSession"), onPress: () => handlers.onOpenDialog(newSession()) }}
            />
          ) : (
            // The padding covers the cards' shadow so the opaque sticky headings do not paint over it.
            <div className="py-1.5">
              <SessionCards handlers={handlers} sessions={live} selectedSessionId={selectedSessionId} consoleOwner={includeConsoleSessions ? consoleOwner : undefined} />
            </div>
          )}
          {!includeConsoleSessions && elsewhere && <BoundElsewhere handlers={handlers} count={elsewhere.count} owners={elsewhere.owners} />}
        </section>
        <ArchivedList handlers={handlers} archived={archived} sessions={archiveContext} selectedSessionId={selectedSessionId} onViewAll={viewAll} />
      </FadedScroller>
    </>
  );
}

/** A console session's focus mode: the sidebar given over to the console session's own card, fixed
 * above the list, and to the projects that have a session bound to it and, within each, only those
 * sessions as cards — a lead session among them carrying its own sessions under it — with its
 * archived bound sessions below. A session opened from here is bound to it, with no choice offered.
 * What has no meaning here is left out: a project's own focus mode (`projectMenu`'s `nested`
 * placement on the project headings here) and the project list's filter. */
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
  // The groups are ordered by how urgent their sessions are, which changes while the view is open.
  const groupsRef = useFlip<HTMLDivElement>();
  // Only the group holding the selected session is told which one it is, so a selection moving
  // between two sessions renders those groups alone.
  const selectedProjectId = useSessionProjectId(selectedSessionId);
  const groups = focusGroups(projects, sessions, consoleSession.id);
  const liveCount = groups.reduce((sum, group) => sum + group.sessions.length, 0);
  const archived = boundArchivedSessions(sessions, consoleSession.id);
  const newSession = (project: Project): NewSessionRequest => ({ kind: "new-session", console: parentConsole, project, binding: { kind: "bound", to: consoleSession } });
  const viewAll = () => handlers.onOpenArchive({ console: parentConsole.id, consoleSession: consoleSession.id });
  const strip = switchStrip(sessions, parentConsole.id);

  return (
    <>
      <FocusHeader handlers={handlers}>
        <FocusTitle heading={parentConsole.name} />
        <RowIconGroup>
          {/* A session needs a project, and a console session with nothing bound yet has no group
              to open one from, so the header offers every project. */}
          <NewSessionMenu
            label={t("sidebar.focus.newSession")}
            quickLabel={t("sidebar.focus.newSessionQuick")}
            projects={projects}
            target={newSession}
            onOpen={handlers.onOpenDialog}
            onQuickCreate={handlers.onQuickCreateSession}
          />
        </RowIconGroup>
      </FocusHeader>
      {strip.length > 1 && <SwitchStrip handlers={handlers} entries={strip} currentId={consoleSession.id} />}
      {/* The console session's own card, fixed above the list: selecting it is the only way to reach
          its terminal and report panel from this view, and its lock, actions and usage live on the
          card. The strip above supplies the top gap when it is drawn. */}
      <div className={`shrink-0 px-(--row-fill-inset) pb-2 ${strip.length > 1 ? "" : "pt-2"}`}>
        <div className="relative rounded-lg bg-panel">
          <SessionCard handlers={handlers} session={consoleSession} selected={consoleSession.id === selectedSessionId} own />
        </div>
      </div>
      <FadedScroller stickyHeadingHeight={STICKY_HEADING_HEIGHT} className="min-h-0 flex-1 px-(--row-fill-inset) pb-2">
        <section>
          <StickySectionHeading>{t("sidebar.focus.sessions", { count: liveCount })}</StickySectionHeading>
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
            // The padding covers the cards' shadow so the opaque sticky headings do not paint over it.
            <div ref={groupsRef} className="relative flex flex-col gap-2 pb-1.5">
              {groups.map(({ project }) => (
                <div key={project.id} data-flip={project.id}>
                  <FocusProjectGroup
                    handlers={handlers}
                    console={parentConsole}
                    consoleSessionId={consoleSession.id}
                    project={project}
                    selectedSessionId={project.id === selectedProjectId ? selectedSessionId : undefined}
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
  const ref = useListMotion<HTMLDivElement>();
  // Pinning or unpinning reorders the strip, which moves the current chip without changing it.
  const currentIndex = entries.findIndex((entry) => entry.consoleSession.id === currentId);
  // Use layout offsets rather than the chip's animated rectangle, so centring targets where a
  // reordered chip will settle. The offset is physical, as scrollLeft is (negative under RTL).
  useLayoutEffect(() => {
    const scroller = ref.current;
    const chip = scroller?.querySelector<HTMLElement>("[data-switch-current]");
    if (!scroller || !chip) return;
    scroller.scrollLeft = chip.offsetLeft + chip.offsetWidth / 2 - scroller.clientWidth / 2;
  }, [currentId, currentIndex]);
  return (
    <FadedScroller
      ref={ref}
      orientation="horizontal"
      hideScrollBar
      role="group"
      aria-label={t("sidebar.focus.switchStrip")}
      className="relative flex shrink-0 gap-1 px-(--row-fill-inset) py-2"
    >
      {entries.map(({ consoleSession, activity }) => {
        const current = consoleSession.id === currentId;
        const activityKey = activityLabelKey(activity);
        return (
          <Button
            key={consoleSession.id}
            data-list-motion={consoleSession.id}
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
 * them carrying its own sessions under it. Memoised, and reading its own sessions from the store
 * (the rows as `focusGroupRows` gives them, the project's archive for its "View archive" submenu),
 * so an event about another project's session leaves it alone. */
const FocusProjectGroup = memo(function FocusProjectGroup({
  handlers,
  console: parentConsole,
  consoleSessionId,
  project,
  selectedSessionId,
}: {
  handlers: SidebarHandlers;
  console: Console;
  /** The console session, by id: its record changes with every status event, which would render
   * every group, and a press reads it from the store. */
  consoleSessionId: string;
  project: Project;
  selectedSessionId?: string;
}): React.ReactElement {
  const t = useT();
  const { store } = useDaemon();
  // Read as `ProjectFocusView` does, for the Git submenu.
  const gitStatus = useDaemonStore((s) => s.gitStatuses.get(project.id));
  const boundHere = useDaemonStoreEqual((s) => focusGroupRows(sessionsOf(indexSessions(s.sessions).byConsole, parentConsole.id), project, consoleSessionId));
  const sessions = useProjectSessions(project.id);
  const archived = archivedSessions(sessions);
  const newSessionHere = (): NewSessionRequest | undefined => {
    const consoleSession = store.getState().sessions.get(consoleSessionId);
    return consoleSession && { kind: "new-session", console: parentConsole, project, binding: { kind: "bound", to: consoleSession } };
  };
  const headerRef = useRef<HTMLDivElement>(null);
  // Where focus goes when the pin button, ahead of it, is unpinned away.
  const openSessionHolder = useRef<HTMLDivElement>(null);
  return (
    <>
      <div ref={headerRef} className="mb-1 flex min-h-7 items-center gap-1 px-(--row-content-inset)">
        <h4 className="min-w-0 flex-1">
          <RowLabel title={project.name} className="block min-w-0">
            <span className="text-sm font-medium">{project.name}</span>
          </RowLabel>
        </h4>
        <RowIconGroup>
          {project.pinned && <PinButton name={project.name} onUnpin={() => handlers.onSetPinned({ project }, false)} returnFocusTo={openSessionHolder} />}
          {/* Named for its project: the header's "+" is beside it in the same view. */}
          <NewSessionButton
            ref={openSessionHolder}
            label={t("sidebar.focus.newSessionIn", { name: isolate(project.name) })}
            quickLabel={t("sidebar.focus.newSessionInQuick", { name: isolate(project.name) })}
            target={newSessionHere}
            onOpen={handlers.onOpenDialog}
            onQuickCreate={handlers.onQuickCreateSession}
          />
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
});

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
  const ownersRef = useListMotion<HTMLDivElement>();
  return (
    <div className="px-(--row-content-inset) pt-3 pb-2">
      <p className="text-xs text-app-muted">
        {t("sidebar.focus.boundElsewhere", { count })}
      </p>
      <div ref={ownersRef} className="relative mt-1 flex flex-wrap gap-1">
        {owners.map(({ owner, count: bound }) => (
          <Button
            key={owner.id}
            data-list-motion={owner.id}
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
 * unanimated while its statuses change. The moving wrapper supplies an opaque base beneath
 * the card's translucent state fills,
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
        <div key={session.id} data-flip={session.id} className={`relative rounded-lg bg-panel ${under ? `session-nested-card ${NESTED_INSET_CLASS}` : ""}`}>
          <SessionCard handlers={handlers} session={session} under={under} selected={session.id === selectedSessionId} consoleOwner={consoleOwner?.(session)} />
        </div>
      ))}
    </div>
  );
}

/** A focus mode's archive: the ten newest archived parent sessions. The heading counts the full
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
  const listRef = useFlip<HTMLDivElement>();
  if (archived.length === 0) return null;
  const preview = archivePreviewSessions(archived, sessions, 10);
  return (
    <section>
      <StickySectionHeading>{t("sidebar.focus.archived", { count: archived.length })}</StickySectionHeading>
      <div ref={listRef} className="relative flex flex-col gap-0.5">
        {preview.map((session) => (
          <div key={session.id} data-flip={session.id}>
            <ArchivedRow handlers={handlers} session={session} selected={session.id === selectedSessionId} />
          </div>
        ))}
        {archived.length > preview.length && (
          <div data-flip="view-all">
            <TreeRow ariaLabel={t("sidebar.archive.viewAll")} onActivate={onViewAll}>
              <List aria-hidden="true" className="size-4 shrink-0 text-app-muted rtl:-scale-x-100" />
              <RowLabel>
                <span className="text-app-muted">{t("sidebar.archive.viewAll")}</span>
              </RowLabel>
            </TreeRow>
          </div>
        )}
      </div>
    </section>
  );
}

/** A session in focus mode: its status put into words, its title on one fading line, the model and
 * reasoning effort it runs with beside its agent's mark (the agent and account are in the mark's
 * tooltip), and when it started. The combined project view also names and marks its console owner.
 * `under`, the project session this one is bound to and is inset below, goes into its accessible name,
 * which is what carries that nesting to assistive technology. `own` is the console session's own
 * card in its focus mode: it shows its own colour as a plain mark, has no pin button, and its menu
 * follows `sessionMenu`'s `focused` placement (no Pin / Unpin, no Focus mode). */
const SessionCard = memo(function SessionCard({
  handlers,
  session,
  under,
  selected,
  consoleOwner,
  own,
}: {
  handlers: SidebarHandlers;
  session: Session;
  under?: Session;
  selected: boolean;
  consoleOwner?: Session;
  own?: boolean;
}): React.ReactElement {
  const metadataId = useId();
  const t = useT();
  const language = useCurrentLanguage();
  const now = useNow();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const hasBound = useHasBoundSessions(session.id);
  const rowRef = useRef<HTMLDivElement>(null);
  const inherited = useDaemonStoreEqual((s) => inheritedLaunch(session, s.consoles, s.projects));
  const modelLabel = sessionModelLabel(t, session.agent, sessionModelEffort(session, inherited));
  // The row is one button, so the model line beside the agent's mark reaches assistive technology
  // only through the row's name, where it comes last.
  const name = [sessionAriaLabel(t, language, session, accounts, under ?? consoleOwner)];
  if (under && consoleOwner) name.push(t("sidebar.session.boundTo", { name: isolate(sessionDisplayTitle(t, consoleOwner)) }));
  const showPin = session.pinned && !own;
  // A console session's card shows its own colour; a project session's shows its console owner's.
  const badgeOwner = own ? session : consoleOwner;
  return (
    <TreeRow
      ref={rowRef}
      describedBy={hasVisibleUsage(session.usage) ? metadataId : undefined}
      ariaLabel={joinPhrases(language, [...name, modelLabel])}
      selected={selected}
      locked={session.locked}
      onActivate={() => handlers.onSelectSession(session)}
      className="min-h-8 flex-col gap-1 border border-transparent shadow-(--panel-shadow) px-[calc(var(--row-content-inset)-1px)] py-[7px] data-selected:border-accent-glyph"
    >
      <RowIconLine icon={<StatusIcon status={sessionStatusKind(session)} decorative />}>
        <span className="text-xs font-medium text-app-muted">{statusLabel(t, sessionStatusKind(session))}</span>
        <span className="flex-1" />
        {/* The controls reach 4px past the status line above and below so it keeps the agent line's
            height, leaving the card's top and bottom edges the same distance from their text. */}
        <RowControls always className="-my-1" fixed={session.locked || showPin ? <>{session.locked && <LockButton name={sessionDisplayTitle(t, session)} onUnlock={() => handlers.onSetLocked(session, false)} returnFocusTo={rowRef} />}{showPin && <PinButton name={sessionDisplayTitle(t, session)} onUnpin={() => handlers.onSetPinned({ session }, false)} returnFocusTo={rowRef} />}</> : undefined}>
          <SessionActions handlers={handlers} session={session} accounts={accounts} hasBoundSessions={hasBound} rowRef={rowRef} placement={own ? "focused" : undefined} />
        </RowControls>
      </RowIconLine>
      <div className="flex min-w-0 items-center gap-(--row-leading-icon-gap)">
        {badgeOwner?.colour && <RowIconSlot placement="leading"><BindingBadge owner={badgeOwner} decorative={own} /></RowIconSlot>}
        <FadeOverflow as="span" titleWhenClipped={sessionDisplayTitle(t, session)} className="min-w-0 flex-1 text-sm font-medium">
          <bdi>{sessionDisplayTitle(t, session)}</bdi>
        </FadeOverflow>
      </div>
      <RowIconLine
        icon={<AgentIcon agent={session.agent} tooltip={sessionAgentLabel(t, session, accounts)} />}
        className="text-xs text-app-muted"
      >
        <FadeOverflow
          as="span"
          className="min-w-0 flex-1"
          titleWhenClipped={modelLabel}
        >
          {modelLabel}
        </FadeOverflow>
        <div className="flex shrink-0 items-center gap-2">
          {hasVisibleUsage(session.usage) && <div id={metadataId}><SessionUsageLabel usage={session.usage} /></div>}
          <span>{formatRelativeTime(language, session.started_at, now)}</span>
        </div>
      </RowIconLine>
    </TreeRow>
  );
});

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
  const metadataId = useId();
  const t = useT();
  const language = useCurrentLanguage();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  const rowRef = useRef<HTMLDivElement>(null);
  return (
    <TreeRow
      ref={rowRef}
      describedBy={hasVisibleUsage(session.usage) ? metadataId : undefined}
      ariaLabel={sessionAriaLabel(t, language, session, accounts)}
      selected={selected}
      onActivate={() => handlers.onSelectSession(session)}
    >
      <AgentIcon agent={session.agent} tooltip={sessionAgentLabel(t, session, accounts)} />
      <RowLabel title={sessionDisplayTitle(t, session)} className="min-w-0 flex-1">
        <span className="text-app-muted">{sessionDisplayTitle(t, session)}</span>
      </RowLabel>
      {hasVisibleUsage(session.usage) && <span id={metadataId}><SessionUsageLabel usage={session.usage} /></span>}
      <RowControls>
        <SessionActions handlers={handlers} session={session} accounts={accounts} hasBoundSessions={false} rowRef={rowRef} />
      </RowControls>
    </TreeRow>
  );
}
