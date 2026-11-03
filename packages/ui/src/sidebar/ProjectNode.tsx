import { ChevronDown, ChevronRight, MessageSquarePlus } from "lucide-react";
import React, { memo, useRef } from "react";

import { ActionMenu } from "../components/menus/ActionMenu";
import { TopStatusMarker } from "../components/status/TopStatusMarker";
import { EmptyPanel } from "../components/panel/EmptyPanel";
import { RowControls } from "../components/controls/RowControls";
import { NESTED_RAIL_CLASS, NESTED_ROW_CLASS, NestedBranch, RowLabel, TreeRow } from "../components/rows/TreeRow";
import { useListMotion } from "../components/motion/useListMotion";
import type { NewSessionRequest } from "../dialogs/dialogRequest";
import { withGitBadge } from "../git/gitStatusLabel";
import { isolate } from "../i18n/isolate";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { Console, Project } from "../protocol";
import { archivedSessions, liveSessionRows } from "../sessions/order";
import { statusLabel } from "../sessions/sessionLabel";
import { indexSessions, sessionsOf } from "../sessions/sessionIndex";
import { useDaemonStore, useDaemonStoreEqual, useProjectSessions, useProjectStatus } from "../store";
import { GitBadge } from "./GitBadge";
import { PinButton } from "./MarkButtons";
import { projectMenu } from "./menus";
import { NewSessionButton } from "./NewSessionButton";
import { SessionRow } from "./SessionRow";
import { useNoAgentToStart } from "./useNewSessionMode";
import type { SidebarHandlers } from "./types";

/** A project and its sessions. Memoised: it renders again only for its own project's change, its
 * sessions' (read from the store here, not handed down), the selection reaching or leaving it, or
 * a handler changing. */
export const ProjectNode = memo(function ProjectNode({
  handlers,
  project,
  parentConsole,
  selectedSessionId,
  isCollapsed,
  onToggle,
}: {
  handlers: SidebarHandlers;
  project: Project;
  parentConsole: Console;
  selectedSessionId?: string;
  isCollapsed: boolean;
  onToggle: (projectId: string) => void;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const listRef = useListMotion<HTMLDivElement>();
  const bodiesRef = useListMotion<HTMLDivElement>({ foldPresence: true, fitHeight: true });
  // Read directly rather than taking it as a prop passed down from `App.tsx`: a status change
  // then only re-renders the one row it is about, instead of the whole tree (the selector returns
  // the same `GitStatus` reference until this project's own entry changes, so no `useShallow` is
  // needed).
  const gitStatus = useDaemonStore((s) => s.gitStatuses.get(project.id));
  // The same array until a session of this project changes, so an event about another project's
  // session leaves this row, its menu and its rows alone.
  const sessions = useProjectSessions(project.id);
  const rowRef = useRef<HTMLDivElement>(null);
  const rows = useDaemonStoreEqual((s) => {
    const index = indexSessions(s.sessions);
    return liveSessionRows(sessionsOf(index.byProject, project.id), index.delegatingIds, s.settings.sidebar_order_by_status);
  });
  const lastRootIndex = rows.reduce((last, row, index) => row.under ? last : index, -1);
  const archived = archivedSessions(sessions);
  const status = useProjectStatus(project.id);
  const noAgentToStart = useNoAgentToStart();
  const newSession = (): NewSessionRequest => ({ kind: "new-session", console: parentConsole, project, binding: { kind: "choose" } });
  const nameLabel = status
    ? t(project.pinned ? "session.status.namedPinned" : "session.status.named", { name: isolate(project.name), status: statusLabel(t, status) })
    : project.pinned
      ? t("sidebar.project.ariaLabelPinned", { name: isolate(project.name) })
      : project.name;

  return (
    <div>
      <TreeRow
        ref={rowRef}
        ariaLabel={withGitBadge(language, t, nameLabel, gitStatus)}
        onActivate={() => onToggle(project.id)}
        expanded={!isCollapsed}
      >
        {/* `min-w-16` is the floor itself: it has to sit on this flex item (`flex-1`'s basis is
            0%, so it never shrinks "from" anything the floor could clamp if placed on a child
            instead), leaving `GitBadge`'s branch track as the part that keeps giving way once
            this is reached. */}
        <div className="flex min-w-16 flex-1 items-center">
          <RowLabel title={project.name} className="min-w-0 shrink">
            <span className="font-medium">{project.name}</span>
          </RowLabel>
          {/* After the name rather than before it: while expanded it shows only on hover, which a
              leading chevron could not do without the names jumping sideways. Two glyphs rather than
              one rotated: a rotated chevron that is already mirrored would point the wrong way under
              right-to-left. While hidden it takes no width and no margin, like `RowControls`, so
              the name gets the room. */}
          {isCollapsed ? (
            <ChevronRight aria-hidden="true" className="ms-2 size-3.5 shrink-0 text-app-muted rtl:-scale-x-100" />
          ) : (
            <ChevronDown aria-hidden="true" className="h-3.5 w-0 shrink-0 overflow-hidden text-app-muted opacity-0 transition-[opacity,width,margin] duration-150 motion-reduce:transition-none group-hover:ms-2 group-hover:w-3.5 group-hover:opacity-100 group-data-focus-visible:ms-2 group-data-focus-visible:w-3.5 group-data-focus-visible:opacity-100" />
          )}
        </div>
        {/* No touch popover: a tap on the project row folds it. A failure mark is a button that answers its own tap. */}
        <GitBadge status={gitStatus} decorative touchPopover={false} returnFocusTo={rowRef} />
        {isCollapsed && <TopStatusMarker status={status} />}
        <RowControls fixed={project.pinned ? <PinButton name={project.name} onUnpin={() => handlers.onSetPinned({ project }, false)} returnFocusTo={rowRef} /> : undefined}>
          <NewSessionButton label={t("sidebar.project.openSession")} quickLabel={t("sidebar.project.openSessionQuick")} target={newSession} onOpen={handlers.onOpenDialog} onQuickCreate={handlers.onQuickCreateSession} />
          <ActionMenu label={t("sidebar.project.actions", { name: isolate(project.name) })} items={projectMenu(t, handlers, project, archived, { gitRepository: gitStatus?.repository, pushing: gitStatus?.activity === "pushing" })} contextTargetRef={rowRef} />
        </RowControls>
      </TreeRow>
      <div ref={bodiesRef} className="relative flow-root">
        {!isCollapsed && (
          // Marked so collapsing every project at once can tell whether keyboard focus is standing in
          // a part about to go away (see `foldAll`).
          <div data-sessions data-list-motion="sessions" className={`${NESTED_RAIL_CLASS} mt-0.5`}>
            {rows.length === 0 ? (
              <EmptyPanel
                compact
                icon={MessageSquarePlus}
                message={t("sidebar.noSessions")}
                action={noAgentToStart ? undefined : { label: t("sidebar.project.openSession"), onPress: () => handlers.onOpenDialog(newSession()) }}
              />
            ) : (
              <div ref={listRef} className="relative flex flex-col gap-0.5">
                {rows.map(({ session, under }, index) => (
                  // One flat list, a session bound to a project session only inset
                  // (`NESTED_ROW_CLASS`), rather than a list nested inside the owner's row: the
                  // reorder animation reads the list's direct children (`useListMotion`), so a third level
                  // of its own would leave a team unanimated while its statuses change.
                  <div key={session.id} data-list-motion={session.id} className={under ? NESTED_ROW_CLASS : "relative"}>
                    <NestedBranch last={index >= lastRootIndex} ancestor={!!under} />
                    <SessionRow
                      handlers={handlers}
                      session={session}
                      under={under}
                      selected={session.id === selectedSessionId}
                    />
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
});
