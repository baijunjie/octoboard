import React from "react";
import { useShallow } from "zustand/react/shallow";

import { ConfirmConsequences } from "../../components/dialog/ConfirmConsequences";
import { ConfirmDialog } from "../../components/dialog/ConfirmDialog";
import { InlineReference } from "../../i18n/InlineReference";
import { Message, useT } from "../../i18n/react";
import type { Session } from "../../protocol";
import { boundArchivedSessions, isUnusedSession } from "../../sessions/order";
import { sessionDisplayTitle } from "../../sessions/sessionLabel";
import { useDaemon, useDaemonStore, useHasBoundSessions, useSession } from "../../store";
import { useSessionDeletionOptions } from "../useSessionDeletionOptions";
import { DeleteUnusedSession } from "./DeleteUnusedSession";

/** Deletes the session `requested` names: outright when it has never been used, otherwise after the
 * confirmation. It reads the session live, since whether it is unused can change while the dialog is
 * open; `requested` is the session as the dialog was asked for, which stands in once the store has
 * dropped it. */
export function DeleteSessionDialog({ requested, onClose }: { requested: Session; onClose: () => void }): React.ReactElement {
  const session = useSession(requested.id) ?? requested;
  const hasBound = useHasBoundSessions(requested.id);
  return isUnusedSession(session, hasBound)
    ? <DeleteUnusedSession key={requested.id} sessionId={requested.id} onClose={onClose} />
    : <DeleteSessionConfirmation session={session} onClose={onClose} />;
}

/** The confirmation of a session's deletion, with the option of also moving the agent's own
 * records of it to the Trash. The archived sessions that go along with an archived owner are read
 * live. */
function DeleteSessionConfirmation({ session, onClose }: { session: Session; onClose: () => void }): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  // An archived owner takes the archived sessions bound to it, and those bound to those, along.
  const withIt = useDaemonStore(useShallow((s) => boundArchivedSessions(Array.from(s.sessions.values()), session.id)));
  const { moveRecords, cleanup, lines, extraControl } = useSessionDeletionOptions([session.id, ...withIt.map((s) => s.id)]);
  return (
    <ConfirmDialog
      title={t("dialog.deleteSession.title")}
      errorTitle={t("dialog.deleteSession.failedTitle")}
      subject={<Message id="dialog.deleteSession.subject" params={{ title: <InlineReference dir="auto" wrap>{sessionDisplayTitle(t, session)}</InlineReference> }} />}
      message={t("dialog.deleteSession.message")}
      consequences={(
        <ConfirmConsequences
          title={withIt.length > 0 ? t("dialog.deleteSession.bound", { count: withIt.length }) : undefined}
          sessions={withIt.length > 0 ? withIt : undefined}
          lines={lines(t(withIt.length > 0 ? "dialog.deleteSession.trashRecordsLineBound" : "dialog.deleteSession.trashRecordsLine"))}
        />
      )}
      confirmLabel={t(moveRecords || cleanup.forcing ? "dialog.deleteSession.confirmAll" : "common.delete")}
      destructive
      waiting={cleanup.pending}
      extraControl={extraControl({
        label: t("dialog.deleteSession.trashRecords"),
        hint: t(withIt.length > 0 ? "dialog.deleteSession.trashRecordsNoteBound" : "dialog.deleteSession.trashRecordsNote"),
      })}
      onCancel={onClose}
      onConfirm={async () => {
        await request({ type: "delete_session", session: session.id, trash_agent_records: moveRecords, ...cleanup.choices });
        onClose();
      }}
    />
  );
}
