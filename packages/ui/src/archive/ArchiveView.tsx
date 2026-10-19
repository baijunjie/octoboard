import { Button, Spinner } from "@heroui/react";
import { Archive, RotateCcw, Trash2, X } from "lucide-react";
import React, { useEffect, useId, useLayoutEffect, useRef, useState } from "react";

import { AGENT_LABEL } from "../agents";
import { AgentIcon } from "../components/AgentIcon";
import { EmptyPanel } from "../components/EmptyPanel";
import { FadeOverflow } from "../components/FadeOverflow";
import { PaneTitle } from "../components/PaneTitle";
import { ArchivedSessionMetadata, hasVisibleUsage, SessionUsageLabel } from "../components/SessionMetadata";
import { TitledControl } from "../components/TitledControl";
import { FadedScroller } from "../components/scrollFade";
import { useLatest } from "../components/useLatest";
import type { DialogRequest } from "../dialogs/dialogRequest";
import { useEscapeWhileTooltipOpen } from "../dialogs/useEscapeWhileTooltipOpen";
import { isolate } from "../i18n/isolate";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { Account, Console, Project, Session, Settings } from "../protocol";
import { sessionAgentLabel, sessionAriaLabel, sessionDisplayTitle } from "../sessionLabel";
import { archivedSessionRows, boundArchivedSessions } from "../sidebar/order";
import { NESTED_ROW_CLASS, TreeRow } from "../sidebar/rows";
import { useFlip } from "../sidebar/useFlip";

/** How many rows the list adds each time its end scrolls into view. */
const PAGE = 30;

/** How far an archive view was loaded and scrolled, which Back and Forward put back. */
export interface ArchiveViewState {
  shown: number;
  scrollTop: number;
}

/**
 * Every archived session of a project, every archived console session of a console, or the
 * archived sessions under a console session (bound to it, or to an archived lead session bound to
 * it, each of those inset under that lead session), newest first: the full archive the sidebar's
 * menus and focus mode lead to ("The archive view" in docs/product/sidebar.md). It covers the
 * terminal while open, which stays mounted beneath it. The list is rendered a page at a time,
 * adding the next page as its end scrolls into view; a page is counted in rows, so a session inset
 * under a lead session is never shown before the lead session it is inset under. The records
 * themselves are all in the daemon's snapshot already.
 */
export function ArchiveView({
  console: owner,
  project,
  boundTo,
  sessions,
  accounts,
  defaultNames,
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
  /** The console session whose archived sessions this lists (bound to it, or to an archived lead
   * session bound to it, inset under it), instead of a project's or the console's. Never set
   * together with `project`. */
  boundTo?: Session;
  /** The scope's sessions; only the archived ones are listed, or, with `boundTo`, only the ones
   * archived and under it. */
  sessions: Session[];
  /** Every stored account, to name the one each row's session ran under. */
  accounts: Account[];
  /** The names the user gave the default accounts, by agent. */
  defaultNames: Settings["default_account_names"];
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
  const listRef = useFlip<HTMLUListElement>();
  const archived = archivedSessionRows(boundTo ? boundArchivedSessions(sessions, boundTo.id) : sessions);
  const [shown, setShown] = useState(restore?.shown ?? PAGE);
  const scrollerRef = useRef<HTMLDivElement>(null);
  const sentinelRef = useRef<HTMLLIElement>(null);
  const rootRef = useRef<HTMLElement>(null);
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
    const timer = setTimeout(() => rootRef.current?.focus({ preventScroll: true }), 50);
    return () => clearTimeout(timer);
  }, [active]);

  // Deleting a row, or all of them, unmounts the button the dialog would hand focus back to, which
  // drops it to `<body>`. The delay lets the dialog close and restore focus to a control that
  // still exists (a cancelled one) first; only a focus still lost then is brought back here.
  useEffect(() => {
    if (!active) return;
    const timer = setTimeout(() => {
      if (document.activeElement === document.body) rootRef.current?.focus({ preventScroll: true });
    }, 100);
    return () => clearTimeout(timer);
  }, [archived.length, dialogOpen, active]);

  const heading = project
    ? t("archive.title.project", { name: isolate(project.name) })
    : boundTo
      ? t("archive.title.boundSessions", { name: isolate(sessionDisplayTitle(t, boundTo)) })
      : t("archive.title.consoleSessions", { name: isolate(owner.name) });
  const deleteAll = () =>
    onOpenDialog({ kind: "delete-archived", console: owner, project, consoleSession: boundTo, count: archived.length });

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
      {/* The title is the same `PaneTitle` (a small line over a heading) as the sidebar's focus-mode
          header, which has the same height (`--pane-header-height`), so their bottom lines run on as
          one line. Where it is lives in the top bar's breadcrumb. */}
      <header className="flex h-(--pane-header-height) shrink-0 items-center gap-1 groove-b px-2 ps-4">
        <PaneTitle
          heading={t(project ? "archive.heading.project" : boundTo ? "archive.heading.boundSessions" : "archive.heading.consoleSessions")}
          headingTitle={heading}
          overline={t("archive.count", { count: archived.length })}
        />
        {archived.length > 0 && (
          <Button size="sm" variant="danger-soft" preventFocusOnPress onPress={deleteAll} isDisabled={archived.some(({ session }) => session.locked)}>
            <Trash2 aria-hidden="true" className="size-4" />
            {t("archive.deleteAll")}
          </Button>
        )}
        <TitledControl title={t("common.close")}>
          <Button isIconOnly size="sm" variant="ghost" aria-label={t("common.close")} onPress={onClose}>
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
              <li key={session.id} data-flip={session.id} className={under ? NESTED_ROW_CLASS : ""}>
                <ArchiveRow
                  session={session}
                  under={under}
                  accounts={accounts}
                  defaultNames={defaultNames}
                  selected={session.id === selectedSessionId}
                  onSelect={onSelect}
                  onReopen={onReopen}
                  onOpenDialog={onOpenDialog}
                />
              </li>
            ))}
            {more && (
              <li ref={sentinelRef} className="flex justify-center py-4">
                <Spinner size="sm" aria-label={t("archive.loadingMore")} />
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
 * (`sessionAriaLabel`, which also says which lead session it was archived under, the inset alone
 * being no words); its usage and dates are its description. */
function ArchiveRow({
  session,
  under,
  accounts,
  defaultNames,
  selected,
  onSelect,
  onReopen,
  onOpenDialog,
}: {
  session: Session;
  under?: Session;
  accounts: Account[];
  defaultNames: Settings["default_account_names"];
  selected: boolean;
  onSelect: (session: Session) => void;
  onReopen: (session: Session) => void;
  onOpenDialog: (dialog: DialogRequest) => void;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const usageId = useId();
  const datesId = useId();
  const usage = hasVisibleUsage(session.usage);
  return (
    <TreeRow
      ariaLabel={sessionAriaLabel(t, language, session, accounts, under, defaultNames)}
      describedBy={usage ? `${usageId} ${datesId}` : datesId}
      selected={selected}
      onActivate={() => onSelect(session)}
      className="min-h-12 flex-wrap items-center gap-x-3 gap-y-1 px-2 py-2"
    >
      <AgentIcon agent={session.agent} tooltip={sessionAgentLabel(t, session, accounts, defaultNames)} />
      <div className="min-w-24 flex-1">
        <FadeOverflow dir="auto" className="text-sm" titleWhenClipped={sessionDisplayTitle(t, session)}>
          {sessionDisplayTitle(t, session)}
        </FadeOverflow>
        <div className="flex min-w-0 items-center gap-2 text-xs text-app-muted">
          <FadeOverflow className="min-w-0" titleWhenClipped={AGENT_LABEL[session.agent]}>
            <span aria-hidden="true">{AGENT_LABEL[session.agent]}</span>
          </FadeOverflow>
          {usage && <span id={usageId}><SessionUsageLabel usage={session.usage} /></span>}
        </div>
      </div>
      {/* The actions and the dates move to a line of their own together, so a narrow pane still
          has the dates at the row's end with the actions beside them. */}
      <div className="ms-auto flex min-w-0 flex-wrap items-center justify-end gap-y-1">
        {/* Shown while the row is hovered or holds focus, like the sidebar rows' controls.
            Hidden by clipping to no width rather than removed, so they take no room from
            the title yet stay reachable with Tab; focus lifts the clip so the focus ring
            can paint. The gap before the dates sits inside the clip, so a hidden pair
            leaves none. */}
        <div className="flex max-w-0 shrink-0 overflow-hidden opacity-0 transition-opacity group-hover:max-w-none group-hover:opacity-100 group-focus-within:max-w-none group-focus-within:overflow-visible group-focus-within:opacity-100">
          <div className="sidebar-fills flex shrink-0 items-center pe-3">
            {/* Only this wrapper, padding-free, keeps a press from reaching the row. */}
            <div className="flex items-center gap-1" onClick={(e) => e.stopPropagation()}>
              <Button size="sm" variant="tertiary" onPress={() => onReopen(session)}>
                <RotateCcw aria-hidden="true" className="size-4" />
                {t("sidebar.session.reopen")}
              </Button>
              <TitledControl title={t("archive.delete", { title: isolate(sessionDisplayTitle(t, session)) })}>
                <Button
                  isIconOnly
                  size="sm"
                  variant="danger-soft"
                  isDisabled={session.locked}
                  aria-label={t("archive.delete", { title: isolate(sessionDisplayTitle(t, session)) })}
                  onPress={() => onOpenDialog({ kind: "delete-session", session })}
                  className="text-app-danger"
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
