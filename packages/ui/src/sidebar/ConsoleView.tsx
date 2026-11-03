import { Bot, BotOff, FolderPlus, ListChevronsDownUp, ListChevronsUpDown, MessageSquarePlus, SearchX } from "lucide-react";
import React, { useRef } from "react";
import { useShallow } from "zustand/react/shallow";

import { noAgentState } from "../agents/agents";
import { tagVocabulary } from "../components/form/tagVocabulary";
import { ActionMenu } from "../components/menus/ActionMenu";
import { EmptyPanel } from "../components/panel/EmptyPanel";
import { PANE_HEADER_FRAME_CLASS } from "../components/panel/paneHeaderFrame";
import { handFocusOff } from "../components/focus/handFocusOff";
import { PATH_VAR_PARAMS } from "../components/text/literalParams";
import { RowIconGroup } from "../components/controls/RowControls";
import { StatusAnnouncer } from "../components/announce/StatusAnnouncer";
import { RowIconButton } from "../components/rows/RowIconButton";
import { RowLabel } from "../components/rows/TreeRow";
import { StickyHeading } from "../components/scroll/StickyHeading";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import { INACTIVE_VEIL_CLASS } from "../components/status/inactiveVeil";
import { useListMotion } from "../components/motion/useListMotion";
import type { Console, Project } from "../protocol";
import { INACTIVE_PROJECT_RANK, liveSessions, projectRank, sortProjectsByRank } from "../sessions/order";
import { effectiveTags, matchesFilter } from "../sessions/projectFiltering";
import { indexSessions, sessionsOf } from "../sessions/sessionIndex";
import { useArchivedOwnerSessions, useDaemonStore, useDaemonStoreEqual, useProviders, useSessionProjectId } from "../store";
import { consoleMenu } from "./menus";
import { NewSessionButton } from "./NewSessionButton";
import { type FilterUpdate, isFiltering, type ProjectFilter, ProjectFilterButton, ProjectFilterSummary } from "./ProjectFilter";
import type { ProjectFoldControl } from "./projectFold";
import { ProjectNode } from "./ProjectNode";
import { SessionRow } from "./SessionRow";
import type { SidebarHandlers } from "./types";

/** The header naming the console shown, with the console's own actions beside it. Its name only:
 * the console's avatar is already marked as the current one on the rail (`Rail.tsx`), which also
 * switches consoles and carries what is going on in every console. */
export function ConsoleHeader({ handlers, current }: { handlers: SidebarHandlers; current: Console }): React.ReactElement {
  const t = useT();
  const headerRef = useRef<HTMLDivElement>(null);
  const sessions = useArchivedOwnerSessions(current.id);
  return (
    // The list under this header is inset twice — the scroll area, then each heading and row — so
    // the end padding matches that and this menu lines up with the rows' menus.
    <div ref={headerRef} className={`${PANE_HEADER_FRAME_CLASS} gap-2 px-(--pane-inset)`}>
      <RowLabel title={current.name}>
        <span className="text-base font-semibold">{current.name}</span>
      </RowLabel>
      <RowIconGroup>
        <NewSessionButton
          label={t("sidebar.consoleSessions.new")}
          quickLabel={t("sidebar.consoleSessions.newQuick")}
          target={() => ({ kind: "new-console-session", console: current })}
          onOpen={handlers.onOpenDialog}
          onQuickCreate={handlers.onQuickCreateSession}
        />
        <ActionMenu
          label={t("sidebar.console.actions", { name: isolate(current.name) })}
          items={consoleMenu(t, current, handlers, sessions)}
          contextTargetRef={headerRef}
        />
      </RowIconGroup>
    </div>
  );
}

export function ConsoleBody({
  handlers,
  console: thisConsole,
  projects,
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
  const listRef = useListMotion<HTMLDivElement>();
  const ranks = useProjectRanks(projects);
  const byStatus = useDaemonStore((s) => s.settings.sidebar_order_by_status);
  // Only the project holding the selected session is told which one it is, so a selection moving
  // between two sessions renders those projects alone.
  const selectedProjectId = useSessionProjectId(selectedSessionId);
  const foldButton = useRef<HTMLSpanElement>(null);
  // The tags to pick from are the ones the console's projects carry now; a picked tag none of them
  // carries any more is dropped here, so it neither filters nor shows, yet it stays in the stored
  // selection, which every change to the filter is applied to.
  const vocabulary = tagVocabulary(projects.map((p) => p.tags));
  const filter = { keyword: stored.keyword, tags: effectiveTags(stored.tags, vocabulary) };
  const rankOf = new Map(projects.map((p, i) => [p.id, ranks[i]]));
  const ordered = sortProjectsByRank(
    projects.filter((p) => matchesFilter(p, filter.keyword, filter.tags)),
    rankOf,
    byStatus,
  );
  const foldIds = ordered
    .filter((p) => foldControl === "collapse" || rankOf.get(p.id) !== INACTIVE_PROJECT_RANK)
    .map((p) => p.id);
  const noMatch = projects.length > 0 && ordered.length === 0 ? t("sidebar.filter.noMatch") : undefined;

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
    setCollapsedFor(foldIds, foldControl === "collapse");
    onFold();
  };

  return (
    <>
      <ConsoleSessionsSection handlers={handlers} console={thisConsole} selectedSessionId={selectedSessionId} />
      <section className="flow-root">
        <StickyHeading
          variant="list"
          inline={projects.length > 0 && isFiltering(filter) && <ProjectFilterSummary filter={filter} />}
          action={
            projects.length > 0 && (
              <>
                <ProjectFilterButton
                  filter={filter}
                  vocabulary={vocabulary}
                  onChange={setFilter}
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
          {t("sidebar.projects.headingWithCount", { count: ordered.length })}
        </StickyHeading>
        {/* The modal filter popover hides the surrounding sidebar from assistive technology;
          React Aria keeps a mounted live announcer and its ancestors exposed. */}
        <div data-live-announcer="true">
          <StatusAnnouncer text={noMatch} />
        </div>
        {noMatch ? (
          <EmptyPanel compact silent icon={SearchX} message={noMatch} />
        ) : ordered.length === 0 ? (
        // A row, as the console sessions section's own empty state is: the full panel with the same
        // offer now stands in the terminal's area (`TerminalPane`), but only while nothing at all
        // is selected there, which a console switch leaves it short of — so the offer stays here
        // too rather than falling back to the console header's menu alone.
          <EmptyPanel
            compact
            icon={FolderPlus}
            message={t("sidebar.noProjects")}
            action={{ label: t("sidebar.console.addProject"), onPress: () => handlers.onOpenDialog({ kind: "new-project", console: thisConsole }) }}
          />
        ) : (
        // The inactive projects (no unarchived session) sit at the end, dimmed as one group; pointing at
        // any of them, keyboard focus or an open menu inside one, brings the whole group back.
        // Keyboard focus only (`data-focus-visible`, set by the rows and HeroUI's controls alike):
        // focus a mouse-opened menu hands back to its trigger would otherwise keep the group lit
        // after the pointer has gone. An open menu is matched on `aria-haspopup`, since a project
        // row is itself `aria-expanded` while open.
          <div
            ref={listRef}
            className="relative flex flex-col gap-0.5 [&:has(>[data-inactive]:hover)>[data-inactive]]:after:bg-transparent [&:has(>[data-inactive]_[data-focus-visible])>[data-inactive]]:after:bg-transparent [&:has(>[data-inactive]_[aria-haspopup][aria-expanded=true])>[data-inactive]]:after:bg-transparent"
          >
            {ordered.map((project) => (
              <div
                key={project.id}
                data-list-motion={project.id}
                data-inactive={rankOf.get(project.id) === INACTIVE_PROJECT_RANK || undefined}
                // Dimmed by a veil of the sidebar's own colour laid over it, not by `opacity`: WebKit
                // composites an element that fades in or out (a row's hover controls, its chevron)
                // inside a translucent ancestor wrongly, painting it as a blank tile. The veil is
                // plain paint and lets every pointer event through. It takes the dimmed names below
                // WCAG AA (about 2.9:1, muted text about 1.9:1) on purpose, by the user's decision: a
                // de-emphasis cue, lifted as soon as the group is pointed at, focused or has a menu open.
                className={`${INACTIVE_VEIL_CLASS} data-inactive:after:bg-panel-sidebar/55`}
              >
                <ProjectNode
                  handlers={handlers}
                  project={project}
                  parentConsole={thisConsole}
                  selectedSessionId={project.id === selectedProjectId ? selectedSessionId : undefined}
                  isCollapsed={collapsed.has(project.id)}
                  onToggle={toggle}
                />
              </div>
            ))}
          </div>
        )}
      </section>
    </>
  );
}

/** The console's console sessions, listed above the project list: every one that is not archived,
 * ordered the way a project's own sessions are (`liveSessions`).
 * The archived ones are reached from the console's own menu; see "The console
 * sessions section and the project list" in `docs/product/sidebar.md`. */
function ConsoleSessionsSection({
  handlers,
  console: thisConsole,
  selectedSessionId,
}: {
  handlers: SidebarHandlers;
  console: Console;
  selectedSessionId?: string;
}): React.ReactElement {
  const t = useT();
  const listRef = useListMotion<HTMLDivElement>();
  const agentAvailability = useDaemonStore((s) => s.agentAvailability);
  const providers = useProviders();
  const addedAgents = useDaemonStore((s) => s.settings.added_agents);
  // Only the rows themselves, so a status event that leaves this list as it was renders nothing here.
  const live = useDaemonStoreEqual((s) => {
    const index = indexSessions(s.sessions);
    return liveSessions(sessionsOf(index.byConsole, thisConsole.id).filter((x) => x.kind === "console"), index.delegatingIds, s.queuedAccountSwitches, s.settings.sidebar_order_by_status);
  });
  // With no agent added and installed, starting a console session would only be refused by the
  // daemon the moment it tried — so the action is disabled here instead, and `agents.noneAdded` (an
  // installed agent waits to be added) or `agents.noneFound` (none is installed) is shown. It is
  // rendered whenever the console is blocked, not only when `live` is empty, because a blocked
  // console can still hold console sessions left over from before: one that was interrupted (an
  // agent removed, or a `PATH` change) stays live and keeps `liveSessions` from being empty.
  const noAgents = noAgentState(agentAvailability, addedAgents, providers);
  const blocked = noAgents !== undefined;
  const openNew = () => handlers.onOpenConsoleSession(thisConsole);

  return (
    <section className="flow-root">
      <StickyHeading variant="list">
        {t("sidebar.consoleSessions.headingWithCount", { count: live.length })}
      </StickyHeading>
      {/* Not the glyph the rows below carry: this row is the one in the section that offers nothing to
          press, and the console header's `+` is disabled. The bot says an agent is waiting to be added;
          the crossed-out bot says none is installed. Both are ordinary absences, so neither is a warning. */}
      {blocked && <EmptyPanel compact icon={noAgents === "add" ? Bot : BotOff} message={noAgents === "add" ? t("agents.noneAdded") : <Message id="agents.noneFound" params={PATH_VAR_PARAMS} />} />}
      {live.length === 0 && !blocked && (
        <EmptyPanel
          compact
          icon={MessageSquarePlus}
          message={t("sidebar.consoleSessions.empty")}
          action={{ label: t("sidebar.consoleSessions.new"), onPress: openNew }}
        />
      )}
      {live.length > 0 && (
        <div ref={listRef} className="relative flex flex-col gap-0.5">
          {live.map((session) => (
            <div key={session.id} data-list-motion={session.id}>
              <SessionRow handlers={handlers} session={session} selected={session.id === selectedSessionId} />
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

/** A project's rank (`projectRank`) for each of `projects`, in their order. Read from the store as
 * numbers, so an event that leaves every project's rank as it was does not render the list. */
function useProjectRanks(projects: Project[]): number[] {
  return useDaemonStore(
    useShallow((s) => {
      const { byProject, delegatingIds } = indexSessions(s.sessions);
      return projects.map((p) => projectRank(sessionsOf(byProject, p.id), delegatingIds, s.queuedAccountSwitches));
    }),
  );
}
