import { Archive, RotateCcw, Trash2, X } from "lucide-react";
import React, { useEffect, useId, useLayoutEffect, useRef, useState } from "react";

import { SessionAgentIcon } from "../components/icons/SessionAgentIcon";
import { EmptyPanel } from "../components/panel/EmptyPanel";
import { PANE_HEADER_FRAME_CLASS } from "../components/panel/paneHeaderFrame";
import { FadeOverflow } from "../components/text/FadeOverflow";
import { InlineReference } from "../i18n/InlineReference";
import { PaneTitle } from "../components/panel/PaneTitle";
import { ArchivedSessionMetadata, hasVisibleContext, SessionContextLabel } from "../components/status/SessionMetadata";
import { TitledControl } from "../components/hint/TitledControl";
import { FadedScroller } from "../components/scroll/scrollFade";
import { useLatest } from "../components/hooks/useLatest";
import type { DialogRequest } from "../dialogs/dialogRequest";
import { useEscapeWhileTooltipOpen } from "../components/dialog/useEscapeWhileTooltipOpen";
import { isolate } from "../i18n/isolate";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { Account, Console, Project, ProviderInfo, Session, Settings } from "../protocol";
import { sessionAccountModelLine, sessionDisplayTitle } from "../sessions/sessionLabel";
import { providerModelDisplay } from "../agents/providers";
import { inheritedLaunch, sessionModelEffort } from "../sessions/sessionModel";
import { archivedSessionRows, boundArchivedSessions } from "../sessions/order";
import { HOVER_ACTIONS_CLASS } from "../components/rows/hoverActions";
import { NESTED_ROW_CLASS, TreeRow } from "../components/rows/TreeRow";
import { useListMotion } from "../components/motion/useListMotion";
import { sessionRowModel } from "../sessions/sessionRowModel";
import { ringForAutomaticFocus } from "../input/lastInput";
import { Button, Spinner } from "../primitives";

/** How many rows the list adds each time its end scrolls into view. */
const PAGE = 30;

/** How long after it opens the view waits to take focus: the menu item that opened it hands focus back a task after its
 * menu closes, and that must not win. */
export const FOCUS_DELAY_MS = 50;

/** How long the view waits, after it opens and after a dialog over it opens or closes, before it moves focus lost to the
 * page on to Close. */
export const FOCUS_FALLBACK_DELAY_MS = 100;

/** How far an archive view was loaded and scrolled, which Back and Forward put back. */
export interface ArchiveViewState {
  shown: number;
  scrollTop: number;
}

/**
 * Every archived session of a project, every archived console session of a console, or the
 * archived sessions under a console session (bound to it, or to an archived bound coordinator bound to
 * it, each of those inset under that bound coordinator), newest first: the full archive the sidebar's
 * menus and focus mode lead to ("The archive view" in docs/product/archive-view.md). It covers the
 * terminal while open, which stays mounted beneath it. The list is rendered a page at a time,
 * adding the next page as its end scrolls into view; a page is counted in rows, so a session inset
 * under a bound coordinator is never shown before the bound coordinator it is inset under. The records
 * themselves are all in the daemon's snapshot already.
 */
export function ArchiveView({
  console: owner,
  project,
  boundTo,
  sessions,
  consoles,
  projects,
  accounts,
  defaultNames,
  providers,
  selectedSessionId,
  onSelect,
  onReopen,
  onOpenDialog,
  dialogOpen,
  onClose,
  restore,
  onViewState,
  active = true,
}: {
  console: Console;
  project?: Project;
  /** The console session whose archived sessions this lists (bound to it, or to an archived coordinator
   * session bound to it, inset under it), instead of a project's or the console's. Never set
   * together with `project`. */
  boundTo?: Session;
  /** The scope's sessions; only the archived ones are listed, or, with `boundTo`, only the ones
   * archived and under it. */
  sessions: Session[];
  /** Every console and project, for the model and effort a row's session inherits where it set none. */
  consoles: Map<string, Console>;
  projects: Map<string, Project>;
  /** Every stored account, to name the one each row's session ran under. */
  accounts: Account[];
  /** The names the user gave the default accounts, by agent. */
  defaultNames: Settings["default_account_names"];
  /** The providers the daemon serves, which say how each row's model is shown. */
  providers: ProviderInfo[];
  /** The selected session, whose row is marked. */
  selectedSessionId?: string;
  /** Selects a row's session, whose saved terminal then shows read-only in place of the view; the
   * view is closed by that, as by any selection. */
  onSelect: (session: Session) => void;
  onReopen: (session: Session) => void;
  onOpenDialog: (dialog: DialogRequest) => void;
  /** Whether a dialog is open, which a deletion's confirmation is. */
  dialogOpen: boolean;
  onClose: () => void;
  /** Where a previous visit to this scope left the list, applied as the view opens. The offset is
   * clamped by the browser if the list has since become shorter. */
  restore?: ArchiveViewState;
  /** Told where the list is as it is scrolled or loads more, for a later `restore`. */
  onViewState?: (state: ArchiveViewState) => void;
  active?: boolean;
}): React.ReactElement {
  const t = useT();
  const listRef = useListMotion<HTMLUListElement>();
  const archived = archivedSessionRows(boundTo ? boundArchivedSessions(sessions, boundTo.id) : sessions);
  const [shown, setShown] = useState(restore?.shown ?? PAGE);
  const scrollerRef = useRef<HTMLDivElement>(null);
  const sentinelRef = useRef<HTMLLIElement>(null);
  const rootRef = useRef<HTMLElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  // The view's own Escape below is a React handler, which an open tooltip's takes the key from.
  useEscapeWhileTooltipOpen(() => active ? rootRef.current : null, onClose);
  const more = shown < archived.length;
  const latestViewState = useLatest(onViewState);
  const reportViewState = () => {
    latestViewState.current?.({ shown, scrollTop: scrollerRef.current?.scrollTop ?? 0 });
  };

  useEffect(() => {
    reportViewState();
  }, [shown]);

  // The rows a restore asks for are rendered by the time this runs, so the offset can be reached.
  useLayoutEffect(() => {
    if (restore && scrollerRef.current) scrollerRef.current.scrollTop = restore.scrollTop;
    // Only as the view opens.
  }, []);

  useEffect(() => {
    const sentinel = sentinelRef.current;
    if (!sentinel) return;
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) setShown((n) => n + PAGE);
    });
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [more, shown]);

  // The view takes keyboard focus as it opens, so keys stop reaching the terminal it covers.
  // Scrolling to reveal the translated view would cancel its slide-in. Focus moves a
  // moment later rather than at once: the menu item that opened it hands focus back to where the
  // menu took it from (`ActionMenu`) a task after it closes, which would otherwise win.
  useEffect(() => {
    if (!active) return;
    const timer = setTimeout(() => rootRef.current?.focus({ preventScroll: true }), FOCUS_DELAY_MS);
    return () => clearTimeout(timer);
  }, [active]);

  // Deleting a row, or all of them, unmounts the button the dialog would hand focus back to, which
  // drops it to `<body>`; a Delete all pressed with the pointer, which takes no focus, has the
  // dialog hand it back to the ring-less section instead. The delay lets the dialog close and
  // restore focus to a control that still exists (a cancelled one) first; only a focus still lost
  // then, or left on the section by a dialog that deleted something, is brought here. It lands on
  // Close, which is always present; the user did not ask for the move, so the ring follows the last
  // input. `dialogCount` is how many rows there were when the dialog opened, kept until the
  // check after it closes.
  const dialogCount = useRef<number | null>(null);
  useEffect(() => {
    if (!active) return;
    if (dialogOpen && dialogCount.current === null) dialogCount.current = archived.length;
    const timer = setTimeout(() => {
      const before = dialogCount.current;
      if (!dialogOpen) dialogCount.current = null;
      const deleted = !dialogOpen && before !== null && archived.length < before;
      const focused = document.activeElement;
      if (focused !== document.body && !(deleted && focused === rootRef.current)) return;
      ringForAutomaticFocus();
      closeRef.current?.focus({ preventScroll: true });
    }, FOCUS_FALLBACK_DELAY_MS);
    return () => clearTimeout(timer);
  }, [archived.length, dialogOpen, active]);

  const heading = project
    ? t("archive.title.project", { name: isolate(project.name) })
    : boundTo
      ? t("archive.title.boundSessions", { name: isolate(sessionDisplayTitle(t, boundTo)) })
      : t("archive.title.consoleSessions", { name: isolate(owner.name) });
  const deleteAll = () =>
    onOpenDialog({ kind: "delete-archived", console: owner, project, consoleSession: boundTo });

  return (
    <section
      ref={rootRef}
      tabIndex={-1}
      data-region="archive"
      aria-label={heading}
      className="absolute inset-0 z-10 flex flex-col bg-panel outline-none"
      onKeyDown={(e) => {
        if (active && e.key === "Escape" && !e.defaultPrevented) {
          e.preventDefault();
          onClose();
        }
      }}
    >
      {/* Where the archive is lives in the top bar's breadcrumb, so the heading names only what it lists; only a clipped heading's tooltip names the place too. */}
      <header className={`${PANE_HEADER_FRAME_CLASS} gap-1 px-(--pane-inset)`}>
        <PaneTitle
          heading={t(project ? "archive.heading.project" : boundTo ? "archive.heading.boundSessions" : "archive.heading.consoleSessions")}
          headingTitle={<Message id={project ? "archive.title.project" : boundTo ? "archive.title.boundSessions" : "archive.title.consoleSessions"} params={{ name: <InlineReference dir="auto" wrap>{project ? project.name : boundTo ? sessionDisplayTitle(t, boundTo) : owner.name}</InlineReference> }} />}
          overline={t("archive.count", { count: archived.length })}
        />
        {archived.length > 0 && (
          <Button size="sm" variant="danger-soft" focusOnPress={false} onPress={deleteAll} disabled={archived.some(({ session }) => session.locked)}>
            <Trash2 aria-hidden="true" className="size-4" />
            {t("archive.deleteAll")}
          </Button>
        )}
        <TitledControl title={t("common.close")}>
          <Button ref={closeRef} iconOnly size="sm" variant="ghost" label={t("common.close")} onPress={onClose}>
            <X aria-hidden="true" className="size-4" />
          </Button>
        </TitledControl>
      </header>
      {/* Only the empty state is centred; the list of rows starts at the top. */}
      <FadedScroller ref={scrollerRef} onScroll={reportViewState} className={`min-h-0 flex-1 ${archived.length === 0 ? "flex flex-col justify-center-safe" : ""}`}>
        {archived.length === 0 ? (
          <EmptyPanel icon={Archive} message={t("sidebar.archive.empty")} />
        ) : (
          <ul ref={listRef} className="relative flex flex-col gap-0.5 p-2">
            {archived.slice(0, shown).map(({ session, under }) => (
              <li key={session.id} data-list-motion={session.id} className={under ? NESTED_ROW_CLASS : ""}>
                <ArchiveRow
                  session={session}
                  under={under}
                  consoles={consoles}
                  projects={projects}
                  accounts={accounts}
                  defaultNames={defaultNames}
                  providers={providers}
                  selected={session.id === selectedSessionId}
                  onSelect={onSelect}
                  onReopen={onReopen}
                  onOpenDialog={onOpenDialog}
                />
              </li>
            ))}
            {more && (
              <li ref={sentinelRef} className="flex justify-center py-4">
                <Spinner size="sm" label={t("archive.loadingMore")} />
              </li>
            )}
          </ul>
        )}
      </FadedScroller>
    </section>
  );
}

/** One row of the list: selects the session (`onSelect`), and holds the Reopen and Delete actions,
 * whose presses must not also select it. Its accessible name is the session's own
 * (`sessionRowName`, which also says which bound coordinator it was archived under, the inset alone
 * being no words), then its model and effort, which the agent line shows only visually; the
 * account is already in the first part. Its context size and dates are its description. */
function ArchiveRow({
  session,
  under,
  consoles,
  projects,
  accounts,
  defaultNames,
  providers,
  selected,
  onSelect,
  onReopen,
  onOpenDialog,
}: {
  session: Session;
  under?: Session;
  consoles: Map<string, Console>;
  projects: Map<string, Project>;
  accounts: Account[];
  defaultNames: Settings["default_account_names"];
  providers: ProviderInfo[];
  selected: boolean;
  onSelect: (session: Session) => void;
  onReopen: (session: Session) => void;
  onOpenDialog: (dialog: DialogRequest) => void;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const contextId = useId();
  const datesId = useId();
  const context = hasVisibleContext(session.context);
  const display = providerModelDisplay(providers, session.agent);
  const launch = sessionModelEffort(session, inheritedLaunch(session, consoles, projects, providers), display);
  const { modelLabel, name } = sessionRowModel(t, language, session, [under], "archived", { accounts, defaultNames, launch, providers });
  const agentLine = sessionAccountModelLine(t, session, accounts, defaultNames, modelLabel);
  return (
    <TreeRow
      ariaLabel={name}
      describedBy={context ? `${contextId} ${datesId}` : datesId}
      selected={selected}
      onActivate={() => onSelect(session)}
      className="min-h-12 flex-wrap items-center gap-x-3 gap-y-1 px-2 py-2"
    >
      <SessionAgentIcon providers={providers} session={session} accounts={accounts} defaultNames={defaultNames} launch={launch} touchPopover={false} />
      <div className="min-w-24 flex-1">
        <FadeOverflow dir="auto" className="text-sm" titleWhenClipped={sessionDisplayTitle(t, session)}>
          {sessionDisplayTitle(t, session)}
        </FadeOverflow>
        <div className="flex min-w-0 items-center gap-2 text-xs text-app-muted">
          <FadeOverflow className="min-w-0" titleWhenClipped={agentLine}>
            <span aria-hidden="true">{agentLine}</span>
          </FadeOverflow>
          {context && <span id={contextId}><SessionContextLabel context={session.context} touchPopover={false} /></span>}
        </div>
      </div>
      {/* The actions and the dates move to a line of their own together, so a narrow pane still
          has the dates at the row's end with the actions beside them. */}
      <div className="ms-auto flex min-w-0 flex-wrap items-center justify-end gap-y-1">
        {/* Shown as the sidebar rows' controls are (`HOVER_ACTIONS_CLASS`). The gap before the dates
            sits inside the clip, so a hidden pair leaves none. */}
        <div className={HOVER_ACTIONS_CLASS}>
          <div className="sidebar-fills flex shrink-0 items-center pe-3">
            {/* Only this wrapper, padding-free, keeps a press from reaching the row. */}
            <div className="flex items-center gap-1" onClick={(e) => e.stopPropagation()}>
              <Button size="sm" variant="tertiary" onPress={() => onReopen(session)}>
                <RotateCcw aria-hidden="true" className="size-4" />
                {t("sidebar.session.reopen")}
              </Button>
              <TitledControl title={<Message id="archive.delete" params={{ title: <InlineReference dir="auto" wrap>{sessionDisplayTitle(t, session)}</InlineReference> }} />}>
                <Button
                  iconOnly
                  size="sm"
                  variant="danger-soft"
                  disabled={session.locked}
                  label={t("archive.delete", { title: isolate(sessionDisplayTitle(t, session)) })}
                  describedBy=""
                  onPress={() => onOpenDialog({ kind: "delete-session", session })}
                >
                  <Trash2 aria-hidden="true" className="size-4" />
                </Button>
              </TitledControl>
            </div>
          </div>
        </div>
        <div id={datesId}><ArchivedSessionMetadata session={session} /></div>
      </div>
    </TreeRow>
  );
}
