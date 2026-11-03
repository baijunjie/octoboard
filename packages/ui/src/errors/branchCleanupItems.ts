import { abbreviateHome } from "../format/pathDisplay";
import { format } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import type { Language } from "../i18n/languages";
import { isKeptForItsFolder, isProjectFolderUnknown, isProtectedFolder, type MessageParams, type SessionBranchCause, type SessionBranchState } from "../protocol";
import { CODE } from "./codes";

/** One bound branch or worktree a `session_branches_not_removed` alert names: `name` is its path or
 * branch name as text, `state` why it is still there, `uncommitted` whether a worktree has
 * uncommitted changes, `detail` the reason given for a `failed` one, and `cause` why a `failed` one is left where the
 * UI words that itself (`detail` is then the daemon's English, not shown). */
export type LeftBranchItem = { kind: "worktree" | "branch"; name: string; branch: string | null; state: SessionBranchState; uncommitted?: boolean; detail: string | null; cause?: SessionBranchCause | null };

/** The groups a `session_branches_not_removed` alert sorts its items into, in the order shown, each
 * with the lead line that says once why its items are still there, and for some a note after the list on what to do
 * with them; what could not be removed at all comes first. A worktree kept because it is the home folder, the root of a
 * disk or a folder holding Octoboard's data, or because whether it is or holds another project's folder could not be
 * told, is not among what could not be removed, but in a group of its own beside the project folders. */
type LeftBranchLead =
  | "branchCleanup.group.notRemoved" | "branchCleanup.group.uncommitted" | "branchCleanup.group.submodules"
  | "branchCleanup.group.locked" | "branchCleanup.group.unmerged" | "branchCleanup.group.checkedOut"
  | "branchCleanup.group.projectFolder" | "branchCleanup.group.protectedFolder" | "branchCleanup.group.projectFolderUnknown";
type LeftBranchNote = "branchCleanup.note.projectFolderUnknown";
const LEFT_BRANCH_GROUPS: Array<{ key: string; lead: LeftBranchLead; note?: LeftBranchNote; states: SessionBranchState[]; holds: (item: LeftBranchItem) => boolean }> = [
  { key: "notRemoved", lead: "branchCleanup.group.notRemoved", states: ["failed", "removable"], holds: (item) => !isKeptForItsFolder(item) },
  { key: "uncommitted", lead: "branchCleanup.group.uncommitted", states: ["uncommitted_changes"], holds: () => true },
  { key: "submodules", lead: "branchCleanup.group.submodules", states: ["submodules"], holds: () => true },
  { key: "locked", lead: "branchCleanup.group.locked", states: ["locked"], holds: () => true },
  { key: "unmerged", lead: "branchCleanup.group.unmerged", states: ["unmerged"], holds: () => true },
  { key: "checkedOut", lead: "branchCleanup.group.checkedOut", states: ["checked_out"], holds: () => true },
  { key: "projectFolder", lead: "branchCleanup.group.projectFolder", states: ["project_folder"], holds: () => true },
  { key: "protectedFolder", lead: "branchCleanup.group.protectedFolder", states: ["failed"], holds: isProtectedFolder },
  {
    key: "projectFolderUnknown", lead: "branchCleanup.group.projectFolderUnknown", note: "branchCleanup.note.projectFolderUnknown",
    states: ["failed"], holds: isProjectFolderUnknown,
  },
];
const LEFT_BRANCH_STATES = new Set(LEFT_BRANCH_GROUPS.flatMap((group) => group.states));

/** `items` sorted into their groups, leaving out the empty ones. */
export function leftBranchGroups(items: LeftBranchItem[]): Array<{ key: string; lead: LeftBranchLead; note?: LeftBranchNote; items: LeftBranchItem[] }> {
  return LEFT_BRANCH_GROUPS
    .map(({ key, lead, note, states, holds }) => ({ key, lead, note, items: items.filter((item) => states.includes(item.state) && holds(item)) }))
    .filter((group) => group.items.length > 0);
}

/** The reason given for a `failed` item of a `session_branches_not_removed` alert, to show under its name: none for
 * one whose group says why in the UI's own words. */
export function leftBranchDetail(item: LeftBranchItem): string | null {
  return isKeptForItsFolder(item) ? null : item.detail;
}

/** How an item of a `session_branches_not_removed` alert is named to a person: a worktree's path
 * with the daemon host's home written as `~`, a branch's name as it is. */
export function leftBranchName(item: LeftBranchItem, homeDir: string | null | undefined): string {
  return item.kind === "worktree" ? abbreviateHome(item.name, homeDir) : item.name;
}

/** The items of a `session_branches_not_removed` alert, which travel as one JSON param so a name
 * holding a comma, or any other character, is never split. */
export function leftBranchItems(code: string, params: MessageParams): LeftBranchItem[] | undefined {
  if (code !== CODE.session_branches_not_removed || !params.items) return undefined;
  try {
    const items: unknown = JSON.parse(params.items);
    if (!Array.isArray(items) || items.length === 0) return undefined;
    const valid = items.every((item: Partial<LeftBranchItem> | null) =>
      !!item && typeof item.name === "string" && typeof item.state === "string" && LEFT_BRANCH_STATES.has(item.state));
    return valid ? items as LeftBranchItem[] : undefined;
  } catch {
    return undefined;
  }
}

/** An item of a `session_branches_not_removed` alert in plain text: its name, the branch a worktree
 * has checked out, and a locked worktree's uncommitted changes or the reason a removal failed. */
function leftBranchText(language: Language, item: LeftBranchItem, homeDir: string | null | undefined): string {
  const path = isolate(leftBranchName(item, homeDir));
  let name = item.kind === "branch"
    ? path
    : item.branch === null
      ? format(language, "branchCleanup.worktreeDetached", { path })
      : format(language, "branchCleanup.worktree", { path, branch: isolate(item.branch) });
  if (item.state === "locked" && item.uncommitted) name = format(language, "branchCleanup.withTag", { name, tag: format(language, "branchCleanup.uncommittedTag", {}) });
  const detail = leftBranchDetail(item);
  return detail ? format(language, "branchCleanup.failed", { name, detail: isolate(detail) }) : name;
}

/** The items of a `session_branches_not_removed` alert in plain text, group by group, each group's lead saying once
 * why its items are still there, and the groups' notes after the list. */
export function leftBranchesText(language: Language, items: LeftBranchItem[], homeDir: string | null | undefined): string {
  // A name or a detail may hold commas of its own, so the catalog sets the items of a group apart, and the groups,
  // with marks of its own.
  const joined = (key: "branchCleanup.plain.items" | "branchCleanup.plain.groups", parts: string[]) =>
    parts.reduce((before, after) => format(language, key, { before, after }));
  const groups = leftBranchGroups(items).map((group) => format(language, "branchCleanup.plain.group", {
    lead: format(language, group.lead, { count: group.items.length }),
    items: joined("branchCleanup.plain.items", group.items.map((item) => leftBranchText(language, item, homeDir))),
  }));
  // A group's note is a sentence of its own, after the list.
  const notes = leftBranchGroups(items).flatMap((group) => group.note ? [format(language, group.note, { count: group.items.length })] : []);
  return [format(language, "branchCleanup.plain.end", { groups: joined("branchCleanup.plain.groups", groups) }), ...notes]
    .join(format(language, "common.sentenceSeparator", {}));
}
