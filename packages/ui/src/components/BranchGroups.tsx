import { Folder, GitBranch } from "lucide-react";
import React, { useId } from "react";

import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import { DiagnosticBlock } from "./DiagnosticBlock";
import { StatusBadge } from "./StatusBadge";
import { useListMotion } from "./useListMotion";

/** One branch or worktree as a cleanup list shows it, in display form: a worktree by its path and
 * the branch it has checked out (`null` for a detached `HEAD`), a branch by its name. */
export type BranchRow = {
  id: string;
  worktree: boolean;
  path: string;
  branch: string | null;
  /** Marks a worktree as having uncommitted changes, where its group does not already say so. */
  uncommitted?: boolean;
  /** Git's message, or why a move failed, for one that could not be removed. */
  detail?: string | null;
};

/** A group of branches and worktrees standing the same way: `lead` says how, once, and the list
 * under it names each, as monospace text. A group whose heading already says it has no lead. */
export type BranchGroup = { key: string; lead?: string; rows: BranchRow[] };

/** A path or branch name shown on its own, so plain monospace text rather than a chip. */
function code(value: string): React.ReactElement {
  return <span dir="ltr" className="font-mono [overflow-wrap:anywhere]">{value}</span>;
}

/** A worktree's path and branch occupy separate lines with a shared icon column so wrapped names
 * keep their text indentation. A branch on its own is named without a path. The branch is announced
 * as one phrase of the catalog's ("on branch ..."), which speaks for the icon and name together, so the drawn name is
 * hidden from it rather than read a second time. */
function RowName({ row }: { row: BranchRow }): React.ReactElement {
  const t = useT();
  if (!row.worktree) return code(row.branch ?? row.path);
  return (
    <span className="inline-flex max-w-full flex-col align-top">
      <span className="flex min-w-0 items-start gap-1">
        <Folder aria-hidden="true" className="mt-1 size-3 shrink-0" />
        <span className="min-w-0">{code(row.path)}</span>
      </span>
      {row.branch === null
        ? <span className="ps-4">{" "}{t("branchCleanup.detached")}</span>
        : (
          <>
            <span className="sr-only select-none">{" "}{t("branchCleanup.onBranch", { branch: isolate(row.branch) })}</span>
            <span aria-hidden="true" className="flex min-w-0 items-start gap-1">
              <GitBranch className="mt-1 size-3 shrink-0" />
              <span className="min-w-0">{code(row.branch)}</span>
            </span>
          </>
        )}
    </span>
  );
}

/** One row: the name, then a badge for uncommitted changes at the end of the line (it wraps whole to
 * the next one, never into the name); or, for one whose removal failed, the name with git's own
 * message under it in a code block, as raw diagnostic text always is. */
function Row({ row }: { row: BranchRow }): React.ReactElement {
  const t = useT();
  const name = <RowName row={row} />;
  if (row.detail) {
    return (
      <div className="flex min-w-0 flex-col gap-1">
        <span>{name}</span>
        <DiagnosticBlock>{row.detail}</DiagnosticBlock>
      </div>
    );
  }
  if (!row.uncommitted) return name;
  return (
    <span className="flex min-w-0 flex-wrap items-baseline gap-x-2 gap-y-1">
      <span className="min-w-0">{name}</span>
      <StatusBadge variant="soft" color="warning">{t("branchCleanup.uncommittedTag")}</StatusBadge>
    </span>
  );
}

/** The groups, each a lead line over a real list. `animate` gives a list whose rows can come and go
 * while it is shown (a confirmation's, as its inspection changes) the shared list motion. */
export function BranchGroups({ groups, animate = false }: { groups: BranchGroup[]; animate?: boolean }): React.ReactElement {
  return (
    <div className="flex min-w-0 flex-col gap-2 self-stretch text-sm text-foreground">
      {groups.filter((group) => group.rows.length > 0).map((group) => (
        <Group key={group.key} group={group} animate={animate} />
      ))}
    </div>
  );
}

/** One group: its lead names the list it heads, for a screen reader moving through lists. */
function Group({ group, animate }: { group: BranchGroup; animate: boolean }): React.ReactElement {
  const listRef = useListMotion<HTMLUListElement>();
  const leadId = useId();
  return (
    <div className="flex min-w-0 flex-col gap-1">
      {group.lead && <p id={leadId}>{group.lead}</p>}
      <ul ref={animate ? listRef : undefined} aria-labelledby={group.lead ? leadId : undefined} className="relative flex min-w-0 list-disc flex-col gap-1 ps-5">
        {group.rows.map((row) => (
          <li key={row.id} data-list-motion={animate ? row.id : undefined} className="min-w-0">
            <Row row={row} />
          </li>
        ))}
      </ul>
    </div>
  );
}
