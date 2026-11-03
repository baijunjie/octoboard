import React from "react";
import { useShallow } from "zustand/react/shallow";

import { ConfirmDialog } from "../../components/dialog/ConfirmDialog";
import { ConsequenceLines, SessionList } from "../../components/dialog/ConfirmConsequences";
import { InlineReference } from "../../i18n/InlineReference";
import { Message, useT } from "../../i18n/react";
import type { Session } from "../../protocol";
import { boundSessionsArchivedWith } from "../../sessions/order";
import { sessionDisplayTitle } from "../../sessions/sessionLabel";
import { indexSessions } from "../../sessions/sessionIndex";
import { runsInBackground } from "../../sessions/sessionStatus";
import { useDaemon, useDaemonStore, useSession } from "../../store";
import { useBranchCleanup } from "../BranchCleanupOptions";
import { useProcessStopping } from "../ProcessStopOptions";
import type { SessionInspection } from "../branchInspection";
import { useArchiveInspection } from "../useBranchInspections";
import { ConfirmSection } from "./ConfirmSection";

/** Confirms archiving a session and every unarchived session under it, listing the bound branches
 * and worktrees git refuses to remove with the choice of forcing them, and the processes the
 * archive stops with the choice of keeping any of them running, each as a section of its own with
 * the consequences of its options right under them. The session and the ones that go along are read
 * live: sessions opened or bound while the dialog is open join the list, and the message follows
 * background work starting or finishing in the session itself. `requested` is the session as the
 * dialog was asked for, which stands in once the store has dropped it. */
export function ArchiveSessionDialog({ requested, inspection, onClose }: {
  requested: Session;
  /** An inspection already made for this archive. */
  inspection?: SessionInspection;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const session = useSession(requested.id) ?? requested;
  const withIt = useDaemonStore(useShallow((s) => boundSessionsArchivedWith(Array.from(s.sessions.values()), requested.id, indexSessions(s.sessions).delegatingIds)));
  const inspected = useArchiveInspection([session.id, ...withIt.map((s) => s.id)], inspection);
  const cleanup = useBranchCleanup(inspected);
  const processes = useProcessStopping(inspected.processes);
  const showBound = withIt.length > 0;
  const showCleanup = cleanup.offered;
  const showProcesses = inspected.processes.length > 0;
  return (
    <ConfirmDialog
      title={t("dialog.archiveSession.title")}
      subject={<Message id="dialog.archiveSession.subject" params={{ title: <InlineReference dir="auto" wrap>{sessionDisplayTitle(t, session)}</InlineReference> }} />}
      message={t(runsInBackground(session) ? "dialog.archiveSession.messageBackground" : "dialog.archiveSession.message")}
      // Nothing in it: the sections are in `extraControl`, bound sessions first. It stays defined for the warning icon.
      consequences={null}
      confirmLabel={t(cleanup.forcing ? "dialog.archiveSession.confirmDelete" : "dialog.archiveSession.confirm")}
      destructive={cleanup.forcing}
      waiting={cleanup.pending}
      extraControl={(busy) => (
        <>
          <ConfirmSection heading={t("dialog.archiveSession.boundHeading", { count: withIt.length })} shown={showBound} divided={false}>
            <SessionList sessions={withIt} />
          </ConfirmSection>
          <ConfirmSection heading={t("dialog.archiveSession.branchesHeading")} shown={showCleanup} divided={showBound}>
            {cleanup.extraControl(busy)}
            <ConsequenceLines lines={cleanup.consequences} />
          </ConfirmSection>
          <ConfirmSection heading={t("dialog.archiveSession.processesHeading")} shown={showProcesses} divided={showBound || showCleanup}>
            {processes.extraControl(busy)}
            <ConsequenceLines lines={processes.consequences} />
          </ConfirmSection>
        </>
      )}
      errorTitle={t("dialog.archiveSession.failedTitle")}
      onCancel={onClose}
      onConfirm={async () => {
        await request({ type: "archive_session", session: session.id, ...cleanup.choices, ...processes.choices });
        onClose();
      }}
    />
  );
}
