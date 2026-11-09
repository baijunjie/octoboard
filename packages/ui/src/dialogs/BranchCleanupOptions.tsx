import React, { useEffect, useId, useState } from "react";

import { BranchGroups, type BranchGroup, type BranchRow } from "../components/alerts/BranchGroups";
import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../components/announce/StatusAnnouncer";
import { useHandoffAfterRemoval } from "../components/focus/HandoffOnUnmount";
import { RetryStatusMark } from "../components/status/RetryStatusMark";
import { StatusMark } from "../components/status/StatusMark";
import { classifyError } from "../errors/errorRecovery";
import { Message, useT } from "../i18n/react";
import { display } from "../format/hostPath";
import { type SessionBranchItem } from "../protocol";
import { isProjectFolderUnknown } from "../errors/branchCleanupItems";
import { useHostPaths } from "../components/hooks/useHostPaths";
import { displayWirePath } from "../format/wirePath";
import type { ConsequenceLine } from "../components/dialog/ConfirmConsequences";
import { ConfirmOption } from "../components/form/ConfirmOption";
import { KeepFocusInDialog } from "../components/dialog/KeepFocusInDialog";
import { OptionDetails } from "../components/form/OptionDetails";
import { branchChoices, forcibleBranches, lockedWorktrees, type Inspected } from "./branchInspection";
import { useProjectRemovalBranches } from "./useBranchInspections";

/** `items` as rows of a cleanup list, in display form; `tagChanges` marks a worktree with
 * uncommitted changes where the group it is in does not say so already. */
function useRows(): (items: SessionBranchItem[], tagChanges?: boolean) => BranchRow[] {
  const paths = useHostPaths();
  return (items, tagChanges = false) => items.map((item) => ({
    id: `${item.kind}:${item.repository}:${item.path ?? item.branch}`,
    worktree: item.kind === "worktree",
    path: display(displayWirePath(item.path ?? ""), paths),
    branch: item.branch === null ? null : displayWirePath(item.branch),
    uncommitted: tagChanges && item.uncommitted,
  }));
}

/** The disclosure of the groups an option covers (see `OptionDetails`). */
function BranchDisclosure({ groups, describedBy, name }: {
  groups: BranchGroup[];
  describedBy: string;
  name: string;
}): React.ReactElement {
  return (
    <OptionDetails describedBy={describedBy} name={name}>
      <BranchGroups groups={groups} animate />
    </OptionDetails>
  );
}

/** One option over bound branches and worktrees a confirmation leaves unless the user says
 * otherwise, as a plain block of its own, apart from the consequences, which say what a ticked
 * option does: the checkbox (unchecked by default, by whoever holds
 * its state) with what it removes and how many in its label, its note, and under them the
 * disclosure of what it covers. Nothing shows while there are none; once an inspection takes the
 * last of them away, focus that was in the block goes back to the confirmation. */
function BranchOption({
  items,
  groups,
  label,
  hint,
  isSelected,
  onChange,
}: {
  items: SessionBranchItem[];
  groups: BranchGroup[];
  label: React.ReactNode;
  hint: React.ReactNode;
  isSelected: boolean;
  onChange: (selected: boolean) => void;
}): React.ReactElement {
  const t = useT();
  const optionId = useId();
  return (
    <KeepFocusInDialog>
      {items.length > 0 && (
        <div className="flex flex-col gap-2">
          <div id={optionId}>
            <ConfirmOption isSelected={isSelected} onChange={onChange} label={label} hint={hint} />
          </div>
          <div className="ps-7">
            <BranchDisclosure groups={groups} describedBy={optionId} name={t("branchCleanup.details")} />
          </div>
        </div>
      )}
    </KeepFocusInDialog>
  );
}

/** A checkbox's choice over the items it lists, which holds for that list only: one that changes
 * while the dialog is open, a session joining the scope, is unchecked again, so nothing the user
 * has not seen listed is taken. */
export function useChoiceOver(items: SessionBranchItem[]): [boolean, (selected: boolean) => void] {
  const listed = JSON.stringify(items.map(({ kind, repository, branch, path, state, uncommitted }) => [kind, repository, branch, path, state, uncommitted]));
  const [chosenFor, setChosenFor] = useState<string>();
  return [items.length > 0 && chosenFor === listed, (selected) => setChosenFor(selected ? listed : undefined)];
}

/** The bound branches and worktrees of an archive or deletion that git refuses to remove, with the
 * user's choices of taking them along: the worktrees with uncommitted changes or submodules and
 * the unmerged branches under one checkbox, each kind as a group of its own, the locked worktrees
 * under a checkbox of their own. `extraControl` renders them, `choices` is what the request names
 * (`force_remove`, and in `keep` what was listed and left unticked), `consequences` are the lines
 * the confirmation's consequence text adds while they are ticked, and `pending` holds Confirm while the
 * scope's inspection is on its way; `forcing` is whether either checkbox is ticked, for a
 * confirmation whose label, and for archiving its colour, say it deletes too. `inspected` is what the
 * scope's inspection found (`useArchiveInspection`, `useSessionBranches`). */
export function useBranchCleanup({ items, pending }: Inspected): {
  choices: ReturnType<typeof branchChoices>;
  pending: boolean;
  forcing: boolean;
  /** Whether there is a checkbox to show, for a caller that heads them with a section of its own. */
  offered: boolean;
  consequences: ConsequenceLine[];
  extraControl: React.ReactNode;
} {
  const t = useT();
  const rows = useRows();
  const forcible = forcibleBranches(items);
  const locked = lockedWorktrees(items);
  const [force, setForce] = useChoiceOver(forcible);
  const [removeLocked, setRemoveLocked] = useChoiceOver(locked);
  const forced = [...(force ? forcible : []), ...(removeLocked ? locked : [])];
  const kept = [...(force ? [] : forcible), ...(removeLocked ? [] : locked)];
  const group = (key: string, lead: (count: number) => string, of: SessionBranchItem[]): BranchGroup => ({ key, lead: lead(of.length), rows: rows(of) });
  const changed = forcible.filter((item) => item.state === "uncommitted_changes");
  const submodules = forcible.filter((item) => item.state === "submodules");
  const unmerged = forcible.filter((item) => item.kind === "branch");
  return {
    choices: branchChoices(forced, kept),
    pending,
    forcing: forced.length > 0,
    offered: forcible.length > 0 || locked.length > 0,
    consequences: [
      ...(force ? [{ key: "force", text: <Message id="branchCleanup.consequence.force" params={{ count: forcible.length }} /> }] : []),
      ...(removeLocked ? [{ key: "locked", text: <Message id="branchCleanup.consequence.locked" params={{ count: locked.length }} /> }] : []),
    ],
    extraControl: (
      <>
        <BranchOption
          items={forcible}
          groups={[
            group("uncommitted", (count) => t("branchCleanup.group.uncommitted", { count }), changed),
            group("submodules", (count) => t("branchCleanup.group.submodules", { count }), submodules),
            group("unmerged", (count) => t("branchCleanup.group.unmerged", { count }), unmerged),
          ]}
          label={<Message id="dialog.sessionBranches.force" params={{ count: forcible.length }} />}
          hint={t("dialog.sessionBranches.forceHint", { count: forcible.length })}
          isSelected={force}
          onChange={setForce}
        />
        <BranchOption
          items={locked}
          groups={[{ key: "locked", lead: t("branchCleanup.group.locked", { count: locked.length }), rows: rows(locked, true) }]}
          label={<Message id="dialog.lockedWorktrees.remove" params={{ count: locked.length }} />}
          hint={t("dialog.lockedWorktrees.removeHint", { count: locked.length })}
          isSelected={removeLocked}
          onChange={setRemoveLocked}
        />
      </>
    ),
  };
}

/** Speaks `text` once it has stood for `AFTER_FOCUS_MOVE_DELAY_MS`: the failure it announces arrives as the Trash
 * checkbox is ticked or as a retry's answer moves focus (back to the mark, or to the checkbox when the mark it was on
 * goes), and VoiceOver drops what is written within about 300 ms of either. While `text` is `undefined` (a retry is
 * out) the region is empty, so a retry that fails again in the same words is written, and so spoken, again. */
function DelayedAnnouncer({ text }: { text: string | undefined }): React.ReactElement {
  const [written, setWritten] = useState<string>();
  if (text === undefined && written !== undefined) setWritten(undefined);
  useEffect(() => {
    if (text === undefined) return;
    const timer = setTimeout(() => setWritten(text), AFTER_FOCUS_MOVE_DELAY_MS);
    return () => clearTimeout(timer);
  }, [text]);
  return <StatusAnnouncer text={written} />;
}

/** The Trash option of a project's removal and what goes along with its folder, asked afresh whenever the option is
 * ticked: under the option, the disclosure of the worktrees moved to the Trash and the branches deleted, each as a
 * group, and the disclosure of the worktrees that stay because whether they are or hold another project's folder could
 * not be told; then the locked worktrees as an option of their own. `consequences` are the lines the confirmation's
 * consequence text adds while the options are ticked. A lookup that failed, so that the daemon could not say what else
 * goes, is a mark after the Trash option's label instead, whose sentence is spoken once it appears; where it is a
 * missing Git its popover also holds Try again, whose success (or a retry ending in a lost connection) takes that mark
 * away, so focus it held goes to the Trash checkbox, and whose failure, in whatever words, is spoken again. A branch a
 * locked worktree has checked out goes only with it. `pending` holds Confirm while the answer is on its way. `choices`
 * carries the token of a refusal that a path that could not be resolved decided, for the removal to repeat it, or of an
 * answer that held worktrees back, for the removal to leave them too. */
export function useProjectRemovalCleanup(projectId: string, trash: boolean, setTrash: (trash: boolean) => void): {
  choices: ReturnType<typeof branchChoices> & { inspection?: string };
  pending: boolean;
  /** Asks again what goes along, as after a refused removal, so that a retry carries out what is shown. */
  reinspect: () => void;
  consequences: ConsequenceLine[];
  extraControl: React.ReactNode;
} {
  const t = useT();
  const rows = useRows();
  const trashId = useId();
  const markId = useId();
  const handoffFrom = useHandoffAfterRemoval(() => document.getElementById(trashId)?.querySelector("input"));
  const found = useProjectRemovalBranches(projectId, trash);
  const locked = lockedWorktrees(found.items);
  const [removeLocked, setRemoveLocked] = useChoiceOver(locked);
  const keptBranches = new Set(removeLocked ? [] : locked.map((item) => `${item.repository}:${item.branch}`));
  // A worktree held back, because whether it is or holds another project's folder could not be told, stays.
  const staying = found.items.filter(isProjectFolderUnknown);
  const trashed = found.items.filter((item) => item.kind === "worktree" && item.state !== "locked" && !isProjectFolderUnknown(item));
  const deleted = found.items.filter((item) => item.kind === "branch" && !keptBranches.has(`${item.repository}:${item.branch}`));
  const along = [...trashed, ...deleted];
  // A missing Git says so, with the link to get it and Try again, which asks what goes along again.
  const failure = found.error === undefined ? undefined : classifyError(found.error);
  const gitMissing = failure?.kind === "actionable" && failure.recovery === "installGit" ? found.error : undefined;
  const unknownShown = trash && found.failed;
  const unknownMark = gitMissing !== undefined
    ? (
      <>
        {/* The check answers as a retry succeeds or the connection is lost; focus the mark or its popover held goes to
            the Trash option. */}
        {handoffFrom(`[data-status-mark-id="${markId}"]`)}
        <RetryStatusMark markId={markId} tone="warning" label={t("dialog.removeProject.alongUnknownMark")} error={gitMissing} onRetry={found.reinspect} retrying={found.pending}>
          <p>{t("dialog.removeProject.alongUnknown")}</p>
        </RetryStatusMark>
      </>
    )
    : <StatusMark tone="warning" label={t("dialog.removeProject.alongUnknownMark")}>{t("dialog.removeProject.alongUnknown")}</StatusMark>;
  return {
    // The branches go only as listed, at the commits shown, so they are named back with the locked worktrees forced.
    choices: { ...branchChoices(trash ? [...(removeLocked ? locked : []), ...deleted] : []), ...(trash && found.inspection ? { inspection: found.inspection } : {}) },
    pending: found.pending,
    reinspect: found.reinspect,
    consequences: trash
      ? [
        ...(trashed.length > 0 ? [{ key: "trashed", text: t("branchCleanup.consequence.trashed", { count: trashed.length }) }] : []),
        ...(deleted.length > 0 ? [{ key: "deleted", text: t("branchCleanup.consequence.deleted", { count: deleted.length }) }] : []),
        ...(removeLocked ? [{ key: "lockedTrash", text: t("branchCleanup.consequence.lockedTrash", { count: locked.length }) }] : []),
      ]
      : [],
    extraControl: (
      <>
        <div className="flex flex-col gap-2">
          <div id={trashId}>
            <ConfirmOption isSelected={trash} onChange={setTrash} label={t("dialog.removeProject.trash")} hint={<Message id="dialog.removeProject.trashHint" />}
              mark={unknownShown ? unknownMark : undefined} />
          </div>
          <DelayedAnnouncer text={unknownShown && !found.pending ? t("dialog.removeProject.alongUnknown") : undefined} />
          <KeepFocusInDialog>
            {trash && along.length > 0 && (
              <div className="ps-7">
                <BranchDisclosure
                  groups={[
                    { key: "trashed", lead: t("branchCleanup.group.trashed"), rows: rows(trashed) },
                    { key: "deleted", lead: t("branchCleanup.group.deleted"), rows: rows(deleted) },
                  ]}
                  describedBy={trashId}
                  name={t("branchCleanup.along", { count: along.length })}
                />
              </div>
            )}
          </KeepFocusInDialog>
          <KeepFocusInDialog>
            {trash && staying.length > 0 && (
              <div className="ps-7">
                <BranchDisclosure
                  groups={[{ key: "staysUnknown", lead: t("branchCleanup.group.staysUnknown", { count: staying.length }), rows: rows(staying) }]}
                  describedBy={trashId}
                  name={t("branchCleanup.stays", { count: staying.length })}
                />
              </div>
            )}
          </KeepFocusInDialog>
        </div>
        {trash && (
          <BranchOption
            items={locked}
            groups={[{ key: "locked", lead: t("branchCleanup.group.locked", { count: locked.length }), rows: rows(locked, true) }]}
            label={t("dialog.removeProject.alongLockedRemove", { count: locked.length })}
            hint={t("dialog.removeProject.alongLockedHint", { count: locked.length })}
            isSelected={removeLocked}
            onChange={setRemoveLocked}
          />
        )}
      </>
    ),
  };
}
