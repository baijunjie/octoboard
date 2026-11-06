import React, { useState } from "react";

import type { ConsequenceLine } from "../components/dialog/ConfirmConsequences";
import { KeepFocusInDialog } from "../components/dialog/KeepFocusInDialog";
import { ConfirmOption } from "../components/form/ConfirmOption";
import { useBranchCleanup } from "./BranchCleanupOptions";
import { useSessionBranches } from "./useBranchInspections";
import { useSessionRecords } from "./useSessionRecords";

/** The option of also moving the agent's own records to the Trash, which a completed inspection
 * can take away. */
function SessionRecordsTrashOption({ visible, ...props }: React.ComponentProps<typeof ConfirmOption> & { visible: boolean }): React.ReactElement {
  return <KeepFocusInDialog>{visible && <ConfirmOption {...props} />}</KeepFocusInDialog>;
}

/** What a dialog that deletes the sessions `ids` has in common: the branch cleanup it offers, and the option of also
 * moving the agent's own records of them to the Trash, which only shows while some exist. `lines` are the
 * consequence lines (the cleanup's, then `recordsLine` while the option is ticked) and `extraControl` the
 * options, the cleanup's then the Trash one with its `label` and `hint`. */
export function useSessionDeletionOptions(ids: string[]): {
  moveRecords: boolean;
  cleanup: ReturnType<typeof useBranchCleanup>;
  lines: (recordsLine: string) => ConsequenceLine[];
  extraControl: (option: { label: string; hint: string }) => React.ReactNode;
} {
  const [trash, setTrash] = useState(false);
  const presence = useSessionRecords(ids);
  const cleanup = useBranchCleanup(useSessionBranches(ids));
  const canTrash = presence !== "absent";
  const moveRecords = canTrash && trash;
  return {
    moveRecords,
    cleanup,
    lines: (recordsLine) => [...cleanup.consequences, ...(moveRecords ? [{ key: "records", text: recordsLine }] : [])],
    extraControl: ({ label, hint }) => (
      <>
        {cleanup.extraControl}
        <SessionRecordsTrashOption visible={canTrash} isSelected={trash} onChange={setTrash} label={label} hint={hint} />
      </>
    ),
  };
}
