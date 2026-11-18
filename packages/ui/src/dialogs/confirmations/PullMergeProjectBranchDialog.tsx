import React, { useEffect, useState } from "react";

import { acknowledgementViewOf } from "../../components/alerts/acknowledgementView";
import type { AcknowledgementView } from "../../components/alerts/errorAlerts";
import { focusAfterModalCloses } from "../../components/focus/modalFocus";
import { ConfirmDialog } from "../../components/dialog/ConfirmDialog";
import { CodedError } from "../../errors/codedError";
import { pullNotice } from "../../git/pullNotice";
import { InlineReference } from "../../i18n/InlineReference";
import { Message, useT } from "../../i18n/react";
import type { Project } from "../../protocol";
import { useDaemon, useDaemonStore } from "../../store";

/** Confirms pulling a project's current branch with a merge commit, once it has diverged from its upstream by `ahead`
 * commits of its own and `behind` new ones. Merging loses nothing, so it is no destructive confirmation. It comes after
 * a fetch, perhaps while the user types elsewhere, so it starts on the dialog itself rather than on Confirm. A merge
 * that meets conflicts is undone by the daemon and would meet them again, which ends the dialog's purpose: it turns
 * into a failure modal. Any other refusal is an alert over it, as a retry can help. Closes without a word when the
 * project is removed while it is open, handing focus on as a closed failure does. */
export function PullMergeProjectBranchDialog({ project, ahead, behind, onClose }: { project: Project; ahead: number; behind: number; onClose: () => void }): React.ReactElement {
  const t = useT();
  const { request, showNotice } = useDaemon();
  // What the failure form is about: a confirmation for other figures or another project is a new question, which must not
  // show it, and it is the same dialog (never re-keyed) that moves on, as `resetKey` says.
  const resetKey = `${project.id}:${ahead}:${behind}`;
  const [held, setHeld] = useState<{ key: string; failure: AcknowledgementView }>();
  const failure = held?.key === resetKey ? held.failure : undefined;
  const gone = useDaemonStore((state) => !state.projects.has(project.id));
  useEffect(() => {
    if (!gone) return;
    onClose();
    focusAfterModalCloses();
  }, [gone]);
  return (
    <ConfirmDialog
      title={t("dialog.pullMerge.title")}
      errorTitle={t("sidebar.project.pullFailedTitle")}
      subject={
        <Message
          id="dialog.pullMerge.subject"
          params={{ name: <InlineReference dir="auto" wrap>{project.name}</InlineReference>, ahead: t("dialog.pullMerge.ahead", { count: ahead }), behind: t("dialog.pullMerge.behind", { count: behind }) }}
        />
      }
      message={t("dialog.pullMerge.message")}
      confirmLabel={t("dialog.pullMerge.confirm")}
      unprompted
      resetKey={resetKey}
      failure={failure}
      onCancel={onClose}
      onConfirm={async () => {
        let outcome;
        try {
          ({ outcome } = await request({ type: "pull_project_branch", project: project.id, merge: true }));
        } catch (error) {
          const conflict = error instanceof CodedError && error.code === "pull_merge_conflict" ? acknowledgementViewOf(t, error, t("sidebar.project.pullFailedTitle")) : undefined;
          if (!conflict) throw error;
          setHeld({ key: resetKey, failure: conflict });
          return;
        }
        // Only a request that does not ask to merge is answered with a diverged branch.
        if (outcome.kind === "needs_merge") throw new Error("The daemon asked for a merge that the request already confirmed.");
        showNotice(pullNotice(t, outcome));
        onClose();
      }}
    />
  );
}
