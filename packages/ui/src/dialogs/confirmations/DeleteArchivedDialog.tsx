import React, { useEffect, useState } from "react";

import { ConfirmConsequences } from "../../components/dialog/ConfirmConsequences";
import { ConfirmDialog } from "../../components/dialog/ConfirmDialog";
import { useT } from "../../i18n/react";
import { archivedDeletionScope } from "../../sessions/order";
import { useDaemon, useDaemonStoreEqual } from "../../store";
import type { DialogRequest } from "../dialogRequest";
import { useSessionDeletionOptions } from "../useSessionDeletionOptions";

/** The confirmation of deleting many archived sessions at once, with the option of also moving the
 * agent's own records of them to the Trash. How many the archive view lists, and how many archived
 * sessions go along, are what the store holds now, as the daemon deletes whatever is archived in the
 * scope when the deletion is confirmed. Once nothing is left to
 * delete — another window or client emptied the archive meanwhile — the confirmation closes by itself, unless it
 * is its own deletion that emptied it, which closes it once answered. */
export function DeleteArchivedDialog({ dialog, onClose }: {
  dialog: Extract<DialogRequest, { kind: "delete-archived" }>;
  onClose: () => void;
}): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const { count, withThem, targetIds } = useDaemonStoreEqual((s) => archivedDeletionScope(Array.from(s.sessions.values()), dialog));
  const { moveRecords, cleanup, lines, extraControl } = useSessionDeletionOptions(targetIds);
  // Its own deletion empties the archive too, which must not read as another client having emptied it.
  const [confirming, setConfirming] = useState(false);
  const gone = count === 0 && !confirming;
  useEffect(() => {
    if (gone) onClose();
  }, [gone]);
  return (
    <ConfirmDialog
      title={t("dialog.deleteArchived.title")}
      errorTitle={t("dialog.deleteArchived.failedTitle")}
      message={t("dialog.deleteArchived.message", { count })}
      consequences={(
        <ConfirmConsequences
          title={withThem > 0 ? t("dialog.deleteArchived.bound", { count: withThem }) : undefined}
          lines={lines(t("dialog.deleteArchived.trashRecordsLine"))}
        />
      )}
      confirmLabel={t("dialog.deleteArchived.confirm")}
      destructive
      waiting={cleanup.pending}
      extraControl={extraControl({ label: t("dialog.deleteArchived.trashRecords"), hint: t("dialog.deleteArchived.trashRecordsNote") })}
      onCancel={onClose}
      onConfirm={async () => {
        // Built outside the `try`: the React Compiler leaves a component with optional chaining inside one uncompiled.
        const body = {
          type: "delete_archived_sessions",
          console: dialog.console.id,
          project: dialog.project?.id,
          console_session: dialog.consoleSession?.id,
          trash_agent_records: moveRecords,
          ...cleanup.choices,
        } as const;
        setConfirming(true);
        try {
          await request(body);
        } catch (err) {
          setConfirming(false);
          throw err;
        }
        onClose();
      }}
    />
  );
}
