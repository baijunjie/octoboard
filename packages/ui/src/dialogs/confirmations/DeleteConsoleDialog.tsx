import React from "react";
import { useShallow } from "zustand/react/shallow";

import { ConfirmDialog } from "../../components/dialog/ConfirmDialog";
import { InlineReference } from "../../i18n/InlineReference";
import { Message, useT } from "../../i18n/react";
import { useProjectProcesses } from "../../process/useProjectProcesses";
import type { Console as ConsoleRecord } from "../../protocol";
import { useDaemon, useDaemonStore } from "../../store";
import { ProjectProcessesSection } from "./ProjectProcessesSection";

/** Confirms deleting a console, which the daemon refuses while a session of it runs, so the one thing it lists is
 * what the deletion stops besides: the project processes of every project under the console. */
export function DeleteConsoleDialog({ console: target, onClose }: { console: ConsoleRecord; onClose: () => void }): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const projectIds = useDaemonStore(useShallow((s) => Array.from(s.projects.values()).filter((project) => project.console_id === target.id).map((project) => project.id)));
  const projectProcesses = useProjectProcesses(projectIds);
  return (
    <ConfirmDialog
      title={t("dialog.deleteConsole.title")}
      errorTitle={t("dialog.deleteConsole.failedTitle")}
      subject={<Message id="dialog.deleteConsole.subject" params={{ name: <InlineReference dir="auto" wrap>{target.name}</InlineReference> }} />}
      message={t("dialog.deleteConsole.message")}
      extraControl={<ProjectProcessesSection processes={projectProcesses} divided={false} />}
      confirmLabel={t("common.delete")}
      waiting={projectProcesses.pending}
      typeToConfirm={t("dialog.typeToConfirm.delete")}
      destructive
      onCancel={onClose}
      onConfirm={async () => {
        await request({ type: "delete_console", console: target.id });
        onClose();
      }}
    />
  );
}
