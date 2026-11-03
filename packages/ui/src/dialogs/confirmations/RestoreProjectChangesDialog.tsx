import React from "react";

import { ConfirmDialog } from "../../components/dialog/ConfirmDialog";
import { InlineReference } from "../../i18n/InlineReference";
import { Message, useT } from "../../i18n/react";
import type { Project } from "../../protocol";
import { useDaemon } from "../../store";

/** Confirms restoring a project's working tree to its current commit, discarding the changes to its tracked files. */
export function RestoreProjectChangesDialog({ project, onClose }: { project: Project; onClose: () => void }): React.ReactElement {
  const t = useT();
  const { request, toastNotice } = useDaemon();
  return (
    <ConfirmDialog
      title={t("dialog.restoreProjectChanges.title")}
      errorTitle={t("dialog.restoreProjectChanges.failedTitle")}
      subject={<Message id="dialog.restoreProjectChanges.subject" params={{ name: <InlineReference dir="auto" wrap>{project.name}</InlineReference> }} />}
      message={t("dialog.restoreProjectChanges.message")}
      confirmLabel={t("dialog.restoreProjectChanges.confirm")}
      pendingLabel={t("dialog.restoreProjectChanges.pending")}
      destructive
      onCancel={onClose}
      onConfirm={async () => {
        await request({ type: "restore_project_changes", project: project.id });
        toastNotice(t("sidebar.project.changesRestored"));
        onClose();
      }}
    />
  );
}
