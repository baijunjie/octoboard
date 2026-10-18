import { Archive, Trash2 } from "lucide-react";
import React, { type RefObject } from "react";

import { ActionMenu } from "../components/ActionMenu";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import type { Account, Session } from "../protocol";
import { sessionDisplayTitle } from "../sessionLabel";
import { indexSessions, sessionsOf } from "../sessions/sessionIndex";
import { useDaemonStore } from "../store";
import { offersDelete, sessionMenu } from "./menus";
import { archiveNeedsConfirmation, isUnusedSession } from "./order";
import { RowIconButton } from "./rows";
import type { SidebarHandlers } from "./types";

/** A session row's or card's controls: its action menu, and before it a Delete or Archive shortcut
 * only while the sessions alone call for no confirmation of that action. An archive through the
 * shortcut can still ask, once the daemon says a branch or worktree bound to the session needs the
 * user's word. When a shortcut disappears while holding keyboard focus, focus moves to the row
 * (`rowRef`). */
export function SessionActions({
  handlers,
  session,
  accounts,
  hasBoundSessions,
  rowRef,
}: {
  handlers: SidebarHandlers;
  session: Session;
  accounts: Account[];
  hasBoundSessions: boolean;
  rowRef: RefObject<HTMLElement | null>;
}): React.ReactElement {
  const t = useT();
  // Only what is bound to the session can ask for more than its status does, and that is all in its
  // console, so a row reads its console's sessions alone, and none when nothing is bound to it.
  const atOnce = useDaemonStore((s) => {
    const index = indexSessions(s.sessions);
    const related = index.boundOwnerIds.has(session.id) ? sessionsOf(index.byConsole, session.console_id) : [];
    return !archiveNeedsConfirmation(related, session);
  });
  const handoff = useFocusHandoff(() => rowRef.current?.focus());
  const title = sessionDisplayTitle(t, session);
  const deletable = isUnusedSession(session, hasBoundSessions);
  const archivable = !offersDelete(session, hasBoundSessions) && atOnce;
  return (
    <>
      {!session.locked && (deletable || archivable) && (
        <div className="contents" {...handoff}>
          <RowIconButton
            icon={deletable ? Trash2 : Archive}
            label={t(deletable ? "sidebar.session.delete" : "sidebar.session.archive")}
            ariaLabel={t(deletable ? "sidebar.session.deleteItem" : "sidebar.session.archiveItem", { title: isolate(title) })}
            onPress={() => deletable
              ? handlers.onOpenDialog({ kind: "delete-session", session })
              : handlers.onArchiveSession(session)}
          />
        </div>
      )}
      <ActionMenu label={t("sidebar.session.actions", { title: isolate(title) })} items={sessionMenu(t, handlers, session, accounts, { hasBoundSessions })} contextTargetRef={rowRef} />
    </>
  );
}
