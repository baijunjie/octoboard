import { useEffect, useRef, useState } from "react";

import { useLatest, useStableHandler } from "../components/hooks/useLatest";
import { forcibleBranches, inspectSessionBranches, lockedWorktrees } from "../dialogs/branchInspection";
import type { DialogRequest } from "../dialogs/dialogRequest";
import { useT } from "../i18n/react";
import type { Session } from "../protocol";
import { archiveNeedsConfirmation } from "../sessions/order";
import { holdsAccountSwitch } from "../sessions/sessionStatus";
import { useDaemon, useDaemonStore } from "../store";
import { raiseError } from "../components/alerts/raiseError";
import { ringForAutomaticFocus } from "../input/lastInput";

/**
 * Archiving a session from its row, its menu or a shortcut: at once when nothing would be lost, otherwise through the
 * archive dialog (`openDialog`), which a dialog already open (`dialogRequest`) or the launch question
 * (`launchQuestionOpen`) keeps from opening after the round trip. `resumingIds` is the sessions whose resume is in
 * flight.
 */
export function useArchiveSession({
  resumingIds,
  dialogRequest,
  launchQuestionOpen,
  openDialog,
  focusTerminal,
}: {
  resumingIds: ReadonlySet<string>;
  dialogRequest: DialogRequest | undefined;
  launchQuestionOpen: boolean;
  openDialog: (dialog: DialogRequest) => void;
  focusTerminal: () => void;
}) {
  const t = useT();
  const { request, store } = useDaemon();
  // Archiving at once takes the row away from under a menu that returns focus to its trigger in
  // that row, so once the session reads archived focus is handed to the terminal, where the
  // keyboard user works next. The session's status is read here, as a value, because nothing else
  // renders `App` for a session that is not selected.
  const [archivingId, setArchivingId] = useState<string>();
  const archivingStatus = useDaemonStore((s) => (archivingId ? s.sessions.get(archivingId)?.status : undefined));
  useEffect(() => {
    if (archivingStatus !== "archived") return;
    setArchivingId(undefined);
    if (document.activeElement !== document.body) return;
    ringForAutomaticFocus();
    focusTerminal();
  }, [archivingStatus]);
  // A session being resumed, or switched to another account, still asks: archiving it would cut that
  // short. So does one with a bound branch or worktree git refuses to remove, or a locked worktree,
  // which only the confirmation can take along, or with a process that has run for a while, which
  // only the confirmation can keep running; the daemon removes the rest once the process has
  // ended. Finding that out takes a round trip, during which the session can change, a repeated
  // press is ignored, and a dialog opened meanwhile is left alone.
  const resuming = useLatest(resumingIds);
  const openedDialog = useLatest(dialogRequest);
  const launchQuestionLatest = useLatest(launchQuestionOpen);
  const inspectingArchive = useRef(new Set<string>());
  const asksBeforeArchiving = (session: Session) =>
    resuming.current.has(session.id) || holdsAccountSwitch(session, store.getState().queuedAccountSwitches) || archiveNeedsConfirmation(Array.from(store.getState().sessions.values()), session);
  const archiveSession = (session: Session) => {
    const live = store.getState().sessions.get(session.id) ?? session;
    if (asksBeforeArchiving(live)) {
      openDialog({ kind: "archive-session", session });
      return;
    }
    if (inspectingArchive.current.has(live.id)) return;
    inspectingArchive.current.add(live.id);
    void inspectSessionBranches(request, [live.id], true).then((inspection) => {
      inspectingArchive.current.delete(live.id);
      const now = store.getState().sessions.get(live.id);
      if (!now || now.status === "archived") return;
      if (asksBeforeArchiving(now) || forcibleBranches(inspection.items).length > 0 || lockedWorktrees(inspection.items).length > 0 || inspection.processes.length > 0) {
        if (!openedDialog.current && !launchQuestionLatest.current) openDialog({ kind: "archive-session", session: now, inspection });
        return;
      }
      setArchivingId(now.id);
      request({ type: "archive_session", session: now.id }).catch((err: unknown) => {
        setArchivingId(undefined);
        raiseError(err, { title: t("dialog.archiveSession.failedTitle"), session: now.id });
      });
    });
  };
  const stableArchiveSession = useStableHandler(archiveSession);
  return { archiveSession, stableArchiveSession };
}
