import type { ChangeStatus } from "./content";

/** What a change can be marked as: a change's own status, in conflict, or untracked. */
export type StatusKey = ChangeStatus | "untracked" | "conflicted";

/** How a change's status is marked wherever it is shown as a chip: a letter and a colour. A, M, D,
 * R and T are `git status --short`'s; there an untracked file is `??` and a conflict `U`, here they
 * are U (untracked) and C (conflicted), one letter each like the rest. Untracked has a colour of
 * its own, as the IDEs give unversioned files one: it is no kind of change, and must not read as
 * the green of added or the red of deleted. */
export const STATUS_MARKS: Record<StatusKey, { letter: string; color: "success" | "danger" | "warning" | "accent" | "untracked" }> = {
  added: { letter: "A", color: "success" },
  untracked: { letter: "U", color: "untracked" },
  deleted: { letter: "D", color: "danger" },
  modified: { letter: "M", color: "warning" },
  renamed: { letter: "R", color: "accent" },
  typeChanged: { letter: "T", color: "accent" },
  conflicted: { letter: "C", color: "danger" },
};
