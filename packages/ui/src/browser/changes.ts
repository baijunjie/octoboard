// The Git mode's change list rules: what identifies a change, the order the list shows changes in,
// and how the viewer moves through them. Pure, so the list's rendering and the viewer's navigation
// are two readings of one order and can never disagree, as the tree's are (`tree.ts`).
import type { PlainMessageKey } from "../i18n/catalog";
import type { ChangeEntry, ChangeGroup, ChangeSide, ConflictKind } from "../protocol";
import { changeStatus } from "../viewer/content";
import type { StatusKey } from "../viewer/statusMarks";
import { displayWirePath } from "../wirePath";
import type { ChangeLayout } from "./changeLayout";
import { compareFilePaths } from "./tree";

/** A part of the change list: one of a worktree's change groups, the paths in conflict, or the one
 * section of a branch comparison, which never shares a list with the others. */
export type ChangeSection = ChangeGroup | "conflicted" | "committed";

/** The sections top to bottom, in the order `git status` tells them: what a commit would take, what
 * stops one, then the rest. */
export const SECTION_ORDER: readonly ChangeSection[] = ["staged", "conflicted", "unstaged", "untracked", "committed"];

/** Each section's name, which is what the list heads it with and, for a staged or unstaged change,
 * what the viewer tags it with. */
export const SECTION_LABELS: Record<ChangeSection, PlainMessageKey> = {
  staged: "git.group.staged",
  unstaged: "git.group.unstaged",
  untracked: "git.group.untracked",
  conflicted: "git.group.conflicted",
  committed: "git.group.committed",
};

/** How each kind of conflict reads above the conflicted file. */
export const CONFLICT_LABELS: Record<ConflictKind, PlainMessageKey> = {
  both_modified: "git.conflict.bothModified",
  both_added: "git.conflict.bothAdded",
  both_deleted: "git.conflict.bothDeleted",
  added_by_us: "git.conflict.addedByUs",
  added_by_them: "git.conflict.addedByThem",
  deleted_by_us: "git.conflict.deletedByUs",
  deleted_by_them: "git.conflict.deletedByThem",
};

/** One row of the change list. */
export interface ChangeItem {
  /** The change's identity: its group and both of its sides (`changeKey`). */
  key: string;
  section: ChangeSection;
  entry: ChangeEntry;
  /** The wire path the change is named and ordered by: its new side's, or its old side's when the
   * new one is absent or outside the project. */
  path: string;
  status: StatusKey;
  /**
   * The versions the listing gave, as one string: a listing that changes it has news of the change.
   */
  versions: string;
}

function sideKey(side: ChangeSide): string {
  switch (side.state) {
    case "present":
      return `p:${side.path}`;
    case "absent":
      return "-";
    case "out_of_scope":
      return `o:${side.repository_path}`;
  }
}

/** A change's identity: its group and both sides. A path alone is not one — a file staged and then
 * changed again is two changes, one per group, and a rename names two paths. */
export function changeKey(entry: ChangeEntry): string {
  if (entry.group === "conflicted") return JSON.stringify([entry.group, entry.path]);
  return JSON.stringify([entry.group, sideKey(entry.old), sideKey(entry.new)]);
}

// An untracked file has an absent old side, which `changeStatus` would call added; it is a status of
// its own, here once for the list and the viewer both.
function entryStatus(entry: ChangeEntry): StatusKey {
  if (entry.group === "conflicted") return "conflicted";
  return entry.group === "untracked" ? "untracked" : changeStatus(entry);
}

function entryPath(entry: ChangeEntry): string {
  if (entry.group === "conflicted") return entry.path;
  if (entry.new.state === "present") return entry.new.path;
  return entry.old.state === "present" ? entry.old.path : "";
}

export function changeItem(entry: ChangeEntry): ChangeItem {
  const section: ChangeSection = entry.group;
  const versions = entry.group === "conflicted" ? entry.conflict : JSON.stringify([entry.old, entry.new]);
  return { key: changeKey(entry), section, entry, path: entryPath(entry), status: entryStatus(entry), versions };
}

/** Two changes in list order: by section, then by path as the file tree orders paths, then by key,
 * so the order is total. */
export function compareChanges(a: ChangeItem, b: ChangeItem): number {
  return (
    SECTION_ORDER.indexOf(a.section) - SECTION_ORDER.indexOf(b.section) ||
    compareFilePaths(a.path, b.path) ||
    (a.key < b.key ? -1 : a.key > b.key ? 1 : 0)
  );
}

/** A listing's changes as the list shows them, top to bottom. */
export function changeItems(entries: readonly ChangeEntry[]): ChangeItem[] {
  return entries.map(changeItem).sort(compareChanges);
}

/** The changes before and after `current` among `items` (the list in order). Navigation crosses
 * from one section into the next, as the tree's crosses folders; a change the list no longer has is
 * placed where it would be, so moving on from it still reaches its neighbours. The ends do not
 * wrap. */
export function changeNeighbours(items: readonly ChangeItem[], current: ChangeItem): { previous?: ChangeItem; next?: ChangeItem } {
  const index = items.findIndex((item) => item.key === current.key);
  if (index !== -1) return { previous: items[index - 1], next: items[index + 1] };
  const after = items.findIndex((item) => compareChanges(current, item) < 0);
  const insertAt = after === -1 ? items.length : after;
  return { previous: items[insertAt - 1], next: items[insertAt] };
}

/** The listed changes split into their sections, in order, leaving out empty ones. */
export function sections(items: readonly ChangeItem[]): { section: ChangeSection; items: ChangeItem[] }[] {
  return SECTION_ORDER.map((section) => ({ section, items: items.filter((item) => item.section === section) })).filter(
    (group) => group.items.length > 0,
  );
}

/** One row of a section in its tree form: a directory, with what is under it, or a change. A chain
 * of directories that each hold nothing but the next is one row, named by the whole chain. */
export type ChangeNode =
  | { type: "directory"; key: string; path: string; name: string; children: ChangeNode[] }
  | { type: "change"; key: string; item: ChangeItem };

/** The row key of the directory at `path` in `section`. A change's key (`changeKey`) is JSON, so
 * the two never collide. */
export const directoryKey = (section: ChangeSection, path: string): string => `dir:${section}:${path}`;

/** The changes of one section (already in list order) grouped under their directories, directories
 * before changes at every level as the file tree orders paths, and single-child directory chains
 * compacted into one row. */
export function changeTree(section: ChangeSection, items: readonly ChangeItem[]): ChangeNode[] {
  interface Draft {
    path: string;
    name: string;
    children: (Draft | ChangeItem)[];
  }
  const root: Draft = { path: "", name: "", children: [] };
  const directories = new Map<string, Draft>();
  for (const item of items) {
    const parts = item.path.split("/");
    let parent = root;
    for (let i = 0; i < parts.length - 1; i++) {
      const path = parts.slice(0, i + 1).join("/");
      let directory = directories.get(path);
      if (!directory) {
        directory = { path, name: parts[i], children: [] };
        directories.set(path, directory);
        parent.children.push(directory);
      }
      parent = directory;
    }
    parent.children.push(item);
  }
  const toNode = (child: Draft | ChangeItem): ChangeNode => {
    if (!("children" in child)) return { type: "change", key: child.key, item: child };
    let directory = child;
    let name = child.name;
    while (directory.children.length === 1 && "children" in directory.children[0]) {
      directory = directory.children[0];
      name += `/${directory.name}`;
    }
    return { type: "directory", key: directoryKey(section, directory.path), path: directory.path, name: displayWirePath(name), children: directory.children.map(toNode) };
  };
  return root.children.map(toNode);
}

/** The rows of `nodes` on screen, top to bottom: a collapsed directory keeps its own row and loses
 * everything under it. */
export function visibleNodes(nodes: readonly ChangeNode[], collapsed: ReadonlySet<string>): ChangeNode[] {
  return nodes.flatMap((node) => (node.type === "directory" && !collapsed.has(node.key) ? [node, ...visibleNodes(node.children, collapsed)] : [node]));
}

/** The sections of the list, each with its changes and, in the layout's form, its rows: one per
 * change when flat, the directory tree when `tree`. */
export function changeGroups(
  items: readonly ChangeItem[],
  layout: ChangeLayout,
): { section: ChangeSection; items: ChangeItem[]; nodes: ChangeNode[] }[] {
  return sections(items).map((group) => ({
    ...group,
    nodes: layout === "tree" ? changeTree(group.section, group.items) : group.items.map((item) => ({ type: "change", key: item.key, item })),
  }));
}

/** The changes with a row on screen, in the order the rows are: what the viewer moves through. All
 * of them when flat; in the tree, those not under a collapsed directory. */
export function visibleChanges(items: readonly ChangeItem[], layout: ChangeLayout, collapsed: ReadonlySet<string>): ChangeItem[] {
  if (layout === "flat") return [...items];
  return changeGroups(items, layout).flatMap((group) =>
    visibleNodes(group.nodes, collapsed).flatMap((node) => (node.type === "change" ? [node.item] : [])),
  );
}

/** What the change list says of the change with `key`: listed, at `versions`; gone from a complete
 * list; or nothing yet (no list, a list being refreshed or one cut short before it). */
export type ChangeEvidence = { state: "listed"; versions: string } | { state: "gone" } | { state: "unknown" };

export function changeEvidence(
  list: { items: readonly ChangeItem[]; complete: boolean; refreshing: boolean } | undefined,
  key: string,
): ChangeEvidence {
  if (!list || list.refreshing) return { state: "unknown" };
  const item = list.items.find((candidate) => candidate.key === key);
  if (item) return { state: "listed", versions: item.versions };
  return list.complete ? { state: "gone" } : { state: "unknown" };
}

/** What the viewer shows of a change, as far as reading it again goes: read, failed (`changing`
 * when its sources kept moving as it was read), or nothing yet. */
export type ShownChange = { state: "read" } | { state: "error"; changing?: boolean } | { state: "loading" };

/**
 * Whether the change the viewer shows is read again, given what the list now says of it and what it
 * said when this was last asked for the same change. Only news counts: a listing at other versions,
 * or the change gone from the list (read again, it shows what is there now). A change whose sources
 * kept moving while it was read is read again at every fresh listing that still has it, so it
 * shows once they settle.
 */
export function rereadChange(shown: ShownChange, evidence: ChangeEvidence, previous: ChangeEvidence | undefined, freshListing: boolean): boolean {
  if (shown.state === "loading" || evidence.state === "unknown") return false;
  if (shown.state === "error" && shown.changing) return freshListing && evidence.state === "listed";
  const key = (e: ChangeEvidence | undefined) => (e?.state === "listed" ? `listed:${e.versions}` : e?.state);
  return previous !== undefined && key(evidence) !== key(previous);
}
