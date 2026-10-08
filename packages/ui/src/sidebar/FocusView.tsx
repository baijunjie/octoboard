import { Button, ScrollShadow } from "@heroui/react";
import { Archive, ArrowLeft, List, MessageSquarePlus, Pin, Plus } from "lucide-react";
import React from "react";

import { AGENT_LABEL } from "../agents";
import { ActionMenu } from "../components/ActionMenu";
import { AgentAccountText } from "../components/AgentAccountText";
import { AgentIcon } from "../components/AgentIcon";
import { EmptyPanel } from "../components/EmptyPanel";
import { FadeOverflow } from "../components/FadeOverflow";
import { StatusIcon } from "../components/StatusIcon";
import { TitledControl } from "../components/TitledControl";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { Console, Project, Session } from "../protocol";
import { formatRelativeTime } from "../relativeTime";
import { sessionAccountName, sessionAgentLabel, sessionAriaLabel, statusLabel } from "../sessionLabel";
import { useDaemonStore } from "../store";
import { BindingBadge } from "./BindingBadge";
import { GitBadge } from "./GitBadge";
import { projectMenu, sessionMenu } from "./menus";
import { archivedSessions, liveSessions } from "./order";
import { RowControls, RowIconButton, RowLabel, SectionHeading, TreeRow } from "./rows";
import type { SidebarHandlers } from "./types";
import { useFlip } from "./useFlip";

/** How many archived sessions focus mode lists before "View all". */
const ARCHIVE_PREVIEW = 10;

/** A project's focus mode: the sidebar given over to one project, its sessions as cards and its
 * recent archive below them ("Focus mode" in docs/product/sidebar.md). */
export function FocusView({
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
  sessions: Session[];
  /** This console's console sessions, by id — a bound session's card names and colours its owner
   * by looking it up here. */
  owners: Map<string, Session>;
  selectedSessionId?: string;
}): React.ReactElement {
  const t = useT();
  // Read directly rather than taking it as a prop threaded through `Sidebar.tsx`: this project's
  // own entry, which keeps the same `GitStatus` reference until that entry itself changes, so no
  // `useShallow` is needed.
  const gitStatus = useDaemonStore((s) => s.gitStatuses.get(project.id));
  const live = liveSessions(sessions);
  const archived = archivedSessions(sessions);
  const listRef = useFlip<HTMLDivElement>();
  const openSession = () => handlers.onOpenDialog({ kind: "new-session", console: parentConsole, project });
  const viewAll = () => handlers.onOpenArchive({ console: project.console_id, project: project.id });

  return (
    <>
      {/* A card below sits in the scroll area, then inside its own border and padding
          (`px-2` + 1px + `p-3`), so the end padding matches that and this menu lines up with the
          menu on a card. The start stays put: only the trailing icons were inset short of the cards. */}
      <div className="flex h-14 shrink-0 items-center gap-1 border-b border-separator ps-2 pe-[21px]">
        <TitledControl title={t("sidebar.focus.exit")}>
          <Button
            isIconOnly
            size="sm"
            variant="ghost"
            aria-label={t("sidebar.focus.exit")}
            data-focus-exit
            preventFocusOnPress
            onPress={() => handlers.onFocusProject(undefined)}
          >
            <ArrowLeft aria-hidden="true" className="size-4 rtl:-scale-x-100" />
          </Button>
        </TitledControl>
        {/* `min-w-16` is the floor itself: this is the flex item (`flex-1`'s basis is 0%, so a
            floor on the `RowLabel` child below would clamp nothing), leaving `GitBadge`'s own
            `min-w-0` as the one that keeps giving way once this is reached. */}
        <div className="min-w-16 flex-1 px-1">
          <div className="truncate text-xs text-muted" dir="auto">
            {parentConsole.name}
          </div>
          <RowLabel title={project.name} className="block min-w-0">
            <span className="text-sm font-semibold">{project.name}</span>
          </RowLabel>
        </div>
        <GitBadge status={gitStatus} />
        <RowIconButton icon={Plus} label={t("sidebar.project.openSession")} onPress={openSession} />
        <ActionMenu
          label={t("sidebar.project.actions", { name: project.name })}
          items={projectMenu(t, handlers, project, archived, { inFocus: true })}
        />
      </div>
      <ScrollShadow size={24} className="min-h-0 flex-1 px-2 pb-2">
        <SectionHeading>{t("sidebar.focus.sessions", { count: live.length })}</SectionHeading>
        {live.length === 0 ? (
          <EmptyPanel
            icon={MessageSquarePlus}
            message={t("sidebar.noSessions")}
            action={{ label: t("sidebar.project.openSession"), icon: Plus, onPress: openSession }}
          />
        ) : (
          <div ref={listRef} className="relative flex flex-col gap-2">
            {live.map((session) => (
              <div key={session.id} data-flip={session.id}>
                <SessionCard
                  handlers={handlers}
                  session={session}
                  owner={session.bound_to ? owners.get(session.bound_to) : undefined}
                  selected={session.id === selectedSessionId}
                />
              </div>
            ))}
          </div>
        )}
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
            <Button size="sm" variant="ghost" fullWidth preventFocusOnPress onPress={viewAll} className="mt-1 justify-start font-normal text-muted hover:text-foreground">
              <List aria-hidden="true" className="size-4" />
              {t("sidebar.archive.viewAll", { count: archived.length })}
            </Button>
          </>
        )}
      </ScrollShadow>
    </>
  );
}

/** A session in focus mode: its status put into words, its agent and account, its title over two lines, when
 * it started, and — for a bound session — its binding badge and its owner's name. */
function SessionCard({
  handlers,
  session,
  owner,
  selected,
}: {
  handlers: SidebarHandlers;
  session: Session;
  owner?: Session;
  selected: boolean;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const accounts = useDaemonStore((s) => s.settings.accounts);
  return (
    <TreeRow
      ariaLabel={sessionAriaLabel(t, language, session, accounts, owner)}
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
          <ActionMenu label={t("sidebar.session.actions", { title: session.title })} items={sessionMenu(t, handlers, session)} />
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
        {owner && (
          <>
            <span aria-hidden="true">·</span>
            {/* The owner's name sits right beside the dot, so a tooltip repeating it would add
                nothing; see `decorative`'s own comment on `BindingBadge`. */}
            <BindingBadge owner={owner} decorative />
            <span dir="auto" className="truncate">
              {owner.title}
            </span>
          </>
        )}
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
        <ActionMenu label={t("sidebar.session.actions", { title: session.title })} items={sessionMenu(t, handlers, session)} />
      </RowControls>
    </TreeRow>
  );
}
