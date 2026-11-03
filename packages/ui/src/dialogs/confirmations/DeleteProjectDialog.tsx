import React, { useState } from "react";

import { ConfirmDialog } from "../../components/dialog/ConfirmDialog";
import { ConsequenceLines, SessionList } from "../../components/dialog/ConfirmConsequences";
import { InlineReference } from "../../i18n/InlineReference";
import { Message, useT } from "../../i18n/react";
import { useProjectProcesses } from "../../process/useProjectProcesses";
import { isLive, type Project } from "../../protocol";
import { compareSessions, ordersByStatus } from "../../sessions/order";
import { useDaemon, useDaemonStore, useDelegatingIds, useProjectSessions } from "../../store";
import { useProjectRemovalCleanup } from "../BranchCleanupOptions";
import { isRemovalRefusal } from "../branchInspection";
import { ConfirmSection } from "./ConfirmSection";
import { ProjectProcessesSection } from "./ProjectProcessesSection";

/** Confirms removing a project. Its project processes are stopped with it, listed in a section of their own.
 * The folder stays on disk unless the user opts in to moving it to
 * the Trash, which the daemon does before removing the project and refuses (keeping the project)
 * when it cannot. With the folder go the worktrees outside it that the daemon lists and, where the
 * repository outlives the folder, the branches of the project's sessions, listed once the option is
 * chosen; a locked worktree only when the user also says so, with the branch it has checked out.
 * The running sessions it stops are read live: a session that starts or stops while the dialog is open
 * joins or leaves the list. */
export function DeleteProjectDialog({ project, onClose }: { project: Project; onClose: () => void }): React.ReactElement {
  const t = useT();
  const { request } = useDaemon();
  const [trash, setTrash] = useState(false);
  const cleanup = useProjectRemovalCleanup(project.id, trash, setTrash);
  const projectProcesses = useProjectProcesses([project.id]);
  const projectSessions = useProjectSessions(project.id);
  const delegatingIds = useDelegatingIds();
  const byStatus = useDaemonStore((s) => ordersByStatus(s.settings));
  const running = projectSessions
    .filter((s) => isLive(s.status))
    .sort((a, b) => compareSessions(a, b, delegatingIds, byStatus));
  const showRunning = running.length > 0;
  return (
    <ConfirmDialog
      title={t("dialog.removeProject.title")}
      errorTitle={t("dialog.removeProject.failedTitle")}
      subject={<Message id="dialog.removeProject.subject" params={{ name: <InlineReference dir="auto" wrap>{project.name}</InlineReference> }} />}
      message={t(trash ? "dialog.removeProject.messageTrash" : "dialog.removeProject.message")}
      confirmLabel={t(running.length > 0 ? "dialog.removeProject.confirmRunning" : "common.remove")}
      destructive
      waiting={cleanup.pending || projectProcesses.pending}
      extraControl={(busy) => (
        <>
          <ConfirmSection heading={t("dialog.removeProject.runningHeading", { count: running.length })} shown={showRunning} divided={false}>
            <SessionList sessions={running} />
          </ConfirmSection>
          <ConfirmSection heading={t("dialog.removeProject.folderHeading")} shown divided={showRunning}>
            {cleanup.extraControl(busy)}
            <ConsequenceLines lines={cleanup.consequences} />
          </ConfirmSection>
          <ProjectProcessesSection processes={projectProcesses} divided />
        </>
      )}
      onCancel={onClose}
      onConfirm={async () => {
        try {
          await request({
            type: "delete_project",
            project: project.id,
            stop_sessions: running.length > 0,
            trash_directory: trash,
            ...cleanup.choices,
          });
        } catch (err) {
          // What goes along is asked again, and Confirm waits on it, so a retry carries out what is shown then.
          if (trash && isRemovalRefusal(err)) cleanup.reinspect();
          throw err;
        }
        onClose();
      }}
    />
  );
}
