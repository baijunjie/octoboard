import type { StatusBadgeColor } from "../components/StatusBadge";
import type { PlainMessageKey } from "../i18n/catalog";
import type { ChangeStatus } from "./content";

/** What a change can be marked as: a change's own status, in conflict, or untracked. */
export type StatusKey = ChangeStatus | "untracked" | "conflicted";

/** How a change's status is marked wherever it is shown as a badge: a letter and a colour. A, M, D,
 * R and T are `git status --short`'s; there an untracked file is `??` and a conflict `U`, here they
 * are U (untracked) and C (conflicted), one letter each like the rest. Untracked is neutral: it is
 * no kind of change, and must not read as the green of added or the red of deleted. */
export const STATUS_MARKS: Record<StatusKey, { letter: string; color: StatusBadgeColor }> = {
  added: { letter: "A", color: "success" },
  untracked: { letter: "U", color: "default" },
  deleted: { letter: "D", color: "danger" },
  modified: { letter: "M", color: "warning" },
  renamed: { letter: "R", color: "accent" },
  typeChanged: { letter: "T", color: "accent" },
  conflicted: { letter: "C", color: "danger" },
};

/** The word that names each status on its own, capitalised: what the viewer's header chip shows for
 * an opened change, and what the change list's letter says in its tooltip. */
export const STATUS_LABELS: Record<StatusKey, PlainMessageKey> = {
  added: "viewer.change.added",
  untracked: "viewer.change.untracked",
  conflicted: "viewer.change.conflicted",
  deleted: "viewer.change.deleted",
  renamed: "viewer.change.renamed",
  typeChanged: "viewer.change.typeChanged",
  modified: "viewer.change.modified",
};
