import { Button, ScrollShadow, Spinner } from "@heroui/react";
import { Archive, RotateCcw, Trash2, X } from "lucide-react";
import React, { useEffect, useRef, useState } from "react";

import { AGENT_LABEL } from "../agents";
import { AgentAccountText } from "../components/AgentAccountText";
import { AgentIcon } from "../components/AgentIcon";
import { EmptyPanel } from "../components/EmptyPanel";
import { FadeOverflow } from "../components/FadeOverflow";
import { TitledControl } from "../components/TitledControl";
import type { DialogRequest } from "../dialogs/dialogRequest";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { Account, Console, Project, Session } from "../protocol";
import { formatRelativeTime } from "../relativeTime";
import { sessionAccountName } from "../sessionLabel";
import { archivedSessions, boundArchivedSessions } from "../sidebar/order";

/** How many rows the list adds each time its end scrolls into view. */
const PAGE = 30;

/**
 * Every archived session of a project, every archived console session of a console, or a console
 * session's own archived bound sessions, newest first: the full archive the sidebar's menus and
 * focus mode lead to ("The archive view" in docs/product/sidebar.md). It covers the terminal while
 * open, which stays mounted beneath it. The list is rendered a page at a time, adding the next page
 * as its end scrolls into view; the records themselves are all in the daemon's snapshot already.
 */
export function ArchiveView({
  console: owner,
  project,
  boundTo,
  sessions,
  accounts,
  onReopen,
  onOpenDialog,
  dialogOpen,
  onClose,
}: {
  console: Console;
  project?: Project;
  /** The console session whose own archived bound sessions this lists, instead of a project's or
   * the console's. Never set together with `project`. */
  boundTo?: Session;
  /** The scope's sessions; only the archived ones are listed, or, with `boundTo`, only the ones
   * archived and bound to it. */
  sessions: Session[];
  /** Every stored account, to name the one each row's session ran under. */
  accounts: Account[];
  onReopen: (session: Session) => void;
  onOpenDialog: (dialog: DialogRequest) => void;
  /** Whether a dialog is open, which a deletion's confirmation is. */
  dialogOpen: boolean;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const archived = boundTo ? boundArchivedSessions(sessions, boundTo.id) : archivedSessions(sessions);
  const [shown, setShown] = useState(PAGE);
  const sentinelRef = useRef<HTMLLIElement>(null);
  const rootRef = useRef<HTMLElement>(null);
  const more = shown < archived.length;

  useEffect(() => {
    const sentinel = sentinelRef.current;
    if (!sentinel) return;
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) setShown((n) => n + PAGE);
    });
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [more, shown]);

  // The view takes keyboard focus as it opens, so keys stop reaching the terminal it covers. A
  // moment later rather than at once: the menu item that opened it hands focus back to where the
  // menu took it from (`ActionMenu`) a task after it closes, which would otherwise win.
  useEffect(() => {
    const timer = setTimeout(() => rootRef.current?.focus(), 50);
    return () => clearTimeout(timer);
  }, []);

  // Deleting a row, or all of them, unmounts the button the dialog would hand focus back to, which
  // drops it to `<body>`. The delay lets the dialog close and restore focus to a control that
  // still exists (a cancelled one) first; only a focus still lost then is brought back here.
  useEffect(() => {
    const timer = setTimeout(() => {
      if (document.activeElement === document.body) rootRef.current?.focus();
    }, 100);
    return () => clearTimeout(timer);
  }, [archived.length, dialogOpen]);

  const heading = project
    ? t("archive.title.project", { name: project.name })
    : boundTo
      ? t("archive.title.boundSessions", { name: boundTo.title })
      : t("archive.title.consoleSessions", { name: owner.name });
  const deleteAll = () =>
    onOpenDialog({ kind: "delete-archived", console: owner, project, consoleSession: boundTo, count: archived.length });

  return (
    <section
      ref={rootRef}
      tabIndex={-1}
      data-region="archive"
      aria-label={heading}
      className="absolute inset-0 z-10 flex flex-col bg-surface outline-none"
      onKeyDown={(e) => {
        if (e.key === "Escape" && !e.defaultPrevented) {
          e.preventDefault();
          onClose();
        }
      }}
    >
      {/* Built like the sidebar's focus-mode header (a small line over a title) and given the same
          fixed height, so their bottom borders run on as one line. Where it
          is lives in the top bar's breadcrumb. */}
      <header className="flex h-14 shrink-0 items-center gap-1 border-b border-separator px-2 ps-4">
        <div className="min-w-0 flex-1">
          <div className="truncate text-xs text-muted">{t("archive.count", { count: archived.length })}</div>
          <h2 className="truncate text-sm font-semibold">
            {t(project ? "archive.heading.project" : boundTo ? "archive.heading.boundSessions" : "archive.heading.consoleSessions")}
          </h2>
        </div>
        {archived.length > 0 && (
          <Button size="sm" variant="danger-soft" preventFocusOnPress onPress={deleteAll}>
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
      <ScrollShadow size={24} className="min-h-0 flex-1">
        {archived.length === 0 ? (
          <EmptyPanel icon={Archive} message={t("sidebar.archive.empty")} />
        ) : (
          <ul className="flex flex-col gap-0.5 p-2">
            {archived.slice(0, shown).map((session) => (
              <li
                key={session.id}
                data-marquee-scope
                className="group flex min-h-12 items-center gap-3 rounded-lg px-2 transition-colors hover:bg-default"
              >
                <AgentIcon agent={session.agent} />
                <div className="min-w-0 flex-1">
                  <FadeOverflow dir="auto" className="text-sm" titleWhenClipped={session.title}>
                    {session.title}
                  </FadeOverflow>
                  <div className="truncate text-xs text-muted">
                    <Message
                      id="archive.meta"
                      params={{
                        agent: <AgentAccountText agent={AGENT_LABEL[session.agent]} account={sessionAccountName(t, session, accounts)} />,
                        archived: formatRelativeTime(language, session.ended_at ?? session.started_at),
                      }}
                    />
                  </div>
                </div>
                {/* Shown while the row is hovered or holds focus, like the sidebar rows' controls;
                    hidden by opacity, so they stay reachable with Tab. */}
                <div className="flex shrink-0 items-center gap-1 opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100">
                  <Button size="sm" variant="ghost" onPress={() => onReopen(session)}>
                    <RotateCcw aria-hidden="true" className="size-4" />
                    {t("sidebar.session.reopen")}
                  </Button>
                  <TitledControl title={t("archive.delete", { title: session.title })}>
                    <Button
                      isIconOnly
                      size="sm"
                      variant="ghost"
                      aria-label={t("archive.delete", { title: session.title })}
                      onPress={() => onOpenDialog({ kind: "delete-session", session })}
                      className="text-danger"
                    >
                      <Trash2 aria-hidden="true" className="size-4" />
                    </Button>
                  </TitledControl>
                </div>
              </li>
            ))}
            {more && (
              <li ref={sentinelRef} className="flex justify-center py-4">
                <Spinner size="sm" aria-label={t("archive.loadingMore")} />
              </li>
            )}
          </ul>
        )}
      </ScrollShadow>
    </section>
  );
}
