import { Disclosure } from "@heroui/react";
import React, { useId, useState } from "react";

import { BranchGroups, type BranchGroup, type BranchRow } from "../components/BranchGroups";
import { useT } from "../i18n/react";
import { abbreviateHome } from "../pathDisplay";
import type { SessionBranchItem } from "../protocol";
import { useDaemonStore } from "../store";
import { displayWirePath } from "../wirePath";
import { ConfirmOption } from "./ConfirmOption";
import { KeepFocusInDialog } from "./KeepFocusInDialog";
import { branchChoices, forcibleBranches, lockedWorktrees, useProjectRemovalBranches, useSessionBranches } from "./useSessionBranches";

/** A line the dialog's one warning adds while an option is ticked: `key` names its kind, so a line
 * whose count changes stays the same line; one with an `id` describes an option too. */
export type WarningLine = { key: string; text: string; id?: string };

/** `items` as rows of a cleanup list, in display form; `tagChanges` marks a worktree with
 * uncommitted changes where the group it is in does not say so already. */
function useRows(): (items: SessionBranchItem[], tagChanges?: boolean) => BranchRow[] {
  const homeDir = useDaemonStore((s) => s.homeDir);
  return (items, tagChanges = false) => items.map((item) => ({
    id: `${item.kind}:${item.repository}:${item.path ?? item.branch}`,
    worktree: item.kind === "worktree",
    path: abbreviateHome(displayWirePath(item.path ?? ""), homeDir),
    branch: item.branch === null ? null : displayWirePath(item.branch),
    uncommitted: tagChanges && item.uncommitted,
  }));
}

/** A disclosure, collapsed until asked, over the groups an option covers: a real button with a name
 * that stays put, its state carried by `aria-expanded` and the indicator, described by the option
 * it belongs to (`describedBy`). It is no heading, so several in one dialog add nothing to its
 * outline. It stays as the user left it for as long as it is shown. */
function BranchDisclosure({ groups, describedBy, name }: {
  groups: BranchGroup[];
  describedBy: string;
  name: string;
}): React.ReactElement {
  return (
    <Disclosure>
      <Disclosure.Trigger aria-describedby={describedBy} className="flex items-center gap-1 text-sm font-medium text-foreground">
        {name}
        <Disclosure.Indicator className="ms-0" />
      </Disclosure.Trigger>
      <Disclosure.Content>
        <Disclosure.Body className="px-0">
          <BranchGroups groups={groups} animate />
        </Disclosure.Body>
      </Disclosure.Content>
    </Disclosure>
  );
}

/** One option over bound branches and worktrees a confirmation leaves unless the user says
 * otherwise, as a plain block of its own — never inside a warning, which is the dialog's one
 * callout and says what a ticked option does: the checkbox (unchecked by default, by whoever holds
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
  isReadOnly,
}: {
  items: SessionBranchItem[];
  groups: BranchGroup[];
  label: string;
  hint: string;
  isSelected: boolean;
  onChange: (selected: boolean) => void;
  isReadOnly: boolean;
}): React.ReactElement {
  const t = useT();
  const optionId = useId();
  return (
    <KeepFocusInDialog>
      {items.length > 0 && (
        <div className="flex flex-col gap-2">
          <div id={optionId}>
            <ConfirmOption isSelected={isSelected} onChange={onChange} isReadOnly={isReadOnly} label={label} hint={hint} />
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
 * the dialog's one warning adds while they are ticked, and `pending` holds Confirm while the
 * scope's inspection is on its way. */
export function useBranchCleanup(sessionIds: string[], initial?: SessionBranchItem[]): {
  choices: ReturnType<typeof branchChoices>;
  pending: boolean;
  consequences: WarningLine[];
  extraControl: (busy: boolean) => React.ReactNode;
} {
  const t = useT();
  const rows = useRows();
  const { items, pending } = useSessionBranches(sessionIds, initial);
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
    consequences: [
      ...(force ? [{ key: "force", text: t("branchCleanup.consequence.force", { count: forcible.length }) }] : []),
      ...(removeLocked ? [{ key: "locked", text: t("branchCleanup.consequence.locked", { count: locked.length }) }] : []),
    ],
    extraControl: (busy) => (
      <>
        <BranchOption
          items={forcible}
          groups={[
            group("uncommitted", (count) => t("branchCleanup.group.uncommitted", { count }), changed),
            group("submodules", (count) => t("branchCleanup.group.submodules", { count }), submodules),
            group("unmerged", (count) => t("branchCleanup.group.unmerged", { count }), unmerged),
          ]}
          label={t("dialog.sessionBranches.force", { count: forcible.length })}
          hint={t("dialog.sessionBranches.forceHint", { count: forcible.length })}
          isSelected={force}
          onChange={setForce}
          isReadOnly={busy}
        />
        <BranchOption
          items={locked}
          groups={[{ key: "locked", lead: t("branchCleanup.group.locked", { count: locked.length }), rows: rows(locked, true) }]}
          label={t("dialog.lockedWorktrees.remove", { count: locked.length })}
          hint={t("dialog.lockedWorktrees.removeHint", { count: locked.length })}
          isSelected={removeLocked}
          onChange={setRemoveLocked}
          isReadOnly={busy}
        />
      </>
    ),
  };
}

/** The Trash option of a project's removal and what goes along with its folder, asked afresh
 * whenever the option is ticked: under the option, the disclosure of the worktrees moved to the
 * Trash and the branches deleted, each as a group; then the locked worktrees as an option of their
 * own. `consequences` are the lines the dialog's one warning adds while the options are ticked —
 * among them that more may go, when the daemon could not say what goes. A branch a locked worktree has checked
 * out goes only with it. `pending` holds Confirm while the answer is on its way. */
export function useProjectRemovalCleanup(projectId: string, trash: boolean, setTrash: (trash: boolean) => void): {
  choices: ReturnType<typeof branchChoices>;
  pending: boolean;
  consequences: WarningLine[];
  extraControl: (busy: boolean) => React.ReactNode;
} {
  const t = useT();
  const rows = useRows();
  const trashId = useId();
  // The line saying more may go, while it shows, describes the Trash option as well.
  const unknownId = useId();
  const found = useProjectRemovalBranches(projectId, trash);
  const locked = lockedWorktrees(found.items);
  const [removeLocked, setRemoveLocked] = useChoiceOver(locked);
  const keptBranches = new Set(removeLocked ? [] : locked.map((item) => `${item.repository}:${item.branch}`));
  const trashed = found.items.filter((item) => item.kind === "worktree" && item.state !== "locked");
  const deleted = found.items.filter((item) => item.kind === "branch" && !keptBranches.has(`${item.repository}:${item.branch}`));
  const along = [...trashed, ...deleted];
  return {
    choices: branchChoices(trash && removeLocked ? locked : []),
    pending: found.pending,
    consequences: trash
      ? [
        ...(found.failed ? [{ key: "unknown", text: t("dialog.removeProject.alongUnknown"), id: unknownId }] : []),
        ...(trashed.length > 0 ? [{ key: "trashed", text: t("branchCleanup.consequence.trashed", { count: trashed.length }) }] : []),
        ...(deleted.length > 0 ? [{ key: "deleted", text: t("branchCleanup.consequence.deleted", { count: deleted.length }) }] : []),
        ...(removeLocked ? [{ key: "lockedTrash", text: t("branchCleanup.consequence.lockedTrash", { count: locked.length }) }] : []),
      ]
      : [],
    extraControl: (busy) => (
      <>
        <div className="flex flex-col gap-2">
          <div id={trashId}>
            <ConfirmOption isSelected={trash} onChange={setTrash} isReadOnly={busy} label={t("dialog.removeProject.trash")} hint={t("dialog.removeProject.trashHint")} describedBy={trash && found.failed ? unknownId : undefined} />
          </div>
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
        </div>
        {trash && (
          <BranchOption
            items={locked}
            groups={[{ key: "locked", lead: t("branchCleanup.group.locked", { count: locked.length }), rows: rows(locked, true) }]}
            label={t("dialog.removeProject.alongLockedRemove", { count: locked.length })}
            hint={t("dialog.removeProject.alongLockedHint", { count: locked.length })}
            isSelected={removeLocked}
            onChange={setRemoveLocked}
            isReadOnly={busy}
          />
        )}
      </>
    ),
  };
}
