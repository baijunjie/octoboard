import React from "react";

import { ConfirmDialog } from "../../components/dialog/ConfirmDialog";
import { ConsequenceLines, SessionList } from "../../components/dialog/ConfirmConsequences";
import { InlineReference } from "../../i18n/InlineReference";
import { Message, useT } from "../../i18n/react";
import type { Session } from "../../protocol";
import { sessionDisplayTitle } from "../../sessions/sessionLabel";
import { useDaemon } from "../../store";
import { useProcessStopping } from "../ProcessStopOptions";
import { useArchiveScope } from "../useBranchInspections";
import { ConfirmSection } from "./ConfirmSection";

/** Confirms opening a session that is not archived in its agent's desktop application: the daemon archives it first,
 * with every session bound to it as an archive does, and keeps its branches and worktrees, which is why this dialog
 * has no section for them. The confirmation lists the bound sessions that go along and the process trees the archive
 * stops, with the choice of keeping any of them running, each as a section of its own as the archive's does. The
 * session and the ones that go along are read live. `requested` is the session as the dialog was asked for, which
 * stands in once the store has dropped it; `label` is the provider's wording of the action, the dialog's title and its
 * confirm button. */
export function OpenSessionInAppDialog({ requested, label, onClose }: {
  requested: Session;
  label: string;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const { session, withIt, inspected } = useArchiveScope(requested);
  const processes = useProcessStopping(inspected.processes);
  const showBound = withIt.length > 0;
  const showProcesses = inspected.processes.length > 0;
  return (
    <ConfirmDialog
      title={label}
      subject={<Message id="dialog.openSessionInApp.subject" params={{ title: <InlineReference dir="auto" wrap>{sessionDisplayTitle(t, session)}</InlineReference> }} />}
      message={t("dialog.openSessionInApp.message")}
      // Nothing in it: the sections are in `extraControl`. It stays defined for the warning icon.
      consequences={null}
      confirmLabel={label}
      waiting={inspected.pending}
      extraControl={(
        <>
          <ConfirmSection heading={t("dialog.archiveSession.boundHeading", { count: withIt.length })} shown={showBound} divided={false}>
            <SessionList sessions={withIt} />
          </ConfirmSection>
          <ConfirmSection heading={t("dialog.archiveSession.processesHeading")} shown={showProcesses} divided={showBound}>
            {processes.extraControl}
            <ConsequenceLines lines={processes.consequences} />
          </ConfirmSection>
        </>
      )}
      errorTitle={t("sidebar.session.openInAppFailedTitle")}
      onCancel={onClose}
      onConfirm={async () => {
        await request({ type: "open_session_in_app", session: session.id, ...processes.choices });
        onClose();
      }}
    />
  );
}
