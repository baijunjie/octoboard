// The Git mode's change list rules: what identifies a change, the order the list shows changes in,
// and how the viewer moves through them. Pure, so the list's rendering and the viewer's navigation
// are two readings of one order and can never disagree, as the tree's are (`tree.ts`).
import type { PlainMessageKey } from "../i18n/catalog";
import type { ChangeEntry, ChangeGroup, ChangeSide, CommitChanges, ConflictKind } from "../protocol";
import { changeStatus } from "../viewer/content";
import type { StatusKey } from "../viewer/statusMarks";
import { displayWirePath, wireBaseName } from "../wirePath";
import type { ChangeLayout } from "./changeLayout";
import { compareFilePaths } from "./tree";

/** A section of the change list: one of a worktree's change groups, the paths in conflict, or what a
 * branch has committed since it forked. */
export type ChangeSection = ChangeGroup | "conflicted" | "committed";

/** The scope of one commit's changes (`commit` is a full id) inside the committed section, which
 * keys the commit's own row and what is under it. */
export type CommitScope = `commit:${string}`;
export const commitScope = (commit: string): CommitScope => `commit:${commit}`;

/** The sections top to bottom: a path in conflict first, since it is the most important thing to
 * deal with and stops a commit, then what a commit would take, then the rest, and then what has
 * been committed already. */
export const SECTION_ORDER: readonly ChangeSection[] = ["conflicted", "staged", "unstaged", "untracked", "committed"];

/** Each section's name, which is what the list heads it with and, for a staged or unstaged
 * change, what the viewer tags it with. */
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

/** A commit whose changes the committed section lists, as the commit list named it. */
export interface CommitRef {
  commit: string;
  /** Its first parent, which its changes are against; null for a commit with no parent. */
  parent: string | null;
  subject: string;
}

/** One row of the change list. */
export interface ChangeItem {
  /** The change's identity: its section and both of its sides (`changeKey`, and the commit for one
   * commit's change, so the same file changed by two commits is two rows). */
  key: string;
  section: ChangeSection;
  /** Where the change stands in list order, top to bottom: its section's place, and within the
   * committed section, when it is listed by commit, its commit's. */
  rank: number;
  /** The commit a change listed by commit is of, which the committed section groups under; none
   * elsewhere. */
  commit?: CommitRef;
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
  return { key: changeKey(entry), section, rank: SECTION_ORDER.indexOf(section), entry, path: entryPath(entry), status: entryStatus(entry), versions };
}

/** Two changes in list order: by section, then by path as the file tree orders paths, then by key,
 * so the order is total. */
export function compareChanges(a: ChangeItem, b: ChangeItem): number {
  return a.rank - b.rank || compareFilePaths(a.path, b.path) || (a.key < b.key ? -1 : a.key > b.key ? 1 : 0);
}

/** A listing's changes as the list shows them, top to bottom. */
export function changeItems(entries: readonly ChangeEntry[]): ChangeItem[] {
  return entries.map(changeItem).sort(compareChanges);
}

/** A commit list's changes as the list shows them: the committed section's changes by commit, in
 * the list's order (newest first), and within each commit the changes as `changeItems` orders
 * them. A change's key carries its commit, so a file every commit changed is a row of each. */
export function commitItems(commits: readonly CommitChanges[]): ChangeItem[] {
  return commits.flatMap(({ commit, parent, subject, changes }, index) => {
    const section: ChangeSection = "committed";
    const rank = SECTION_ORDER.length + index;
    const ref: CommitRef = { commit, parent, subject };
    return changes
      .map((entry): ChangeItem => {
        const item = changeItem(entry);
        const sides = entry.group === "conflicted" ? [entry.path] : [sideKey(entry.old), sideKey(entry.new)];
        return { ...item, key: JSON.stringify([commitScope(commit), ...sides]), section, rank, commit: ref };
      })
      .sort(compareChanges);
  });
}

/** The changes whose file name contains `text`, ignoring case and the whitespace around it; all of
 * them for an empty `text`. Only the name is matched (`wireBaseName`, as the row shows it), not the
 * directories before it, so a filter for "src" does not keep every file under `src/`. */
export function filterChanges(items: ChangeItem[], text: string): ChangeItem[] {
  const needle = text.trim().toLowerCase();
  if (needle === "") return items;
  return items.filter((item) => wireBaseName(item.path).toLowerCase().includes(needle));
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

/** One section of the list as it is shown: its changes. */
export interface SectionGroup {
  section: ChangeSection;
  items: ChangeItem[];
}

/** The listed changes split into their sections, in order, leaving out empty ones, except those in
 * `kept`: a section whose failure is marked on its heading keeps the heading with no rows. */
export function sections(items: readonly ChangeItem[], kept: Iterable<ChangeSection> = []): SectionGroup[] {
  const groups = new Map<ChangeSection, SectionGroup>();
  for (const item of items) {
    const group = groups.get(item.section);
    if (group) group.items.push(item);
    else groups.set(item.section, { section: item.section, items: [item] });
  }
  const emptied: ChangeSection[] = [];
  for (const section of kept) if (!groups.has(section)) emptied.push(section);
  const rank = (group: SectionGroup) => group.items[0]?.rank ?? SECTION_ORDER.indexOf(group.section);
  for (const section of emptied) groups.set(section, { section, items: [] });
  return [...groups.values()].sort((a, b) => rank(a) - rank(b));
}

/** One row of a section in its tree form: a directory, with what is under it, or a change. A chain
 * of directories that each hold nothing but the next is one row, named by the whole chain. In the
 * committed section listed by commit, a commit is a row too, holding its changes (`count` of them)
 * as the layout shows them. */
export type ChangeNode =
  | { type: "directory"; key: string; path: string; name: string; children: ChangeNode[] }
  | { type: "commit"; key: string; commit: CommitRef; count: number; children: ChangeNode[] }
  | { type: "change"; key: string; item: ChangeItem };

/** The row key of the directory at `path` in `scope`: a section, or one commit of the committed
 * section, whose directories are its own. A change's key (`changeKey`) is JSON, so the two never
 * collide. */
export const directoryKey = (scope: ChangeSection | CommitScope, path: string): string => `dir:${scope}:${path}`;

/** The row key of `section`'s own heading, which the same set of folded-away rows holds beside the
 * directories' and the commits'. A change's key is JSON and a directory's starts with `dir:`, so
 * no two of the four can collide. */
export const sectionKey = (section: ChangeSection): string => `section:${section}`;

/** The row key of the commit's own row, folded away like a section is, and so kept by the commit
 * whatever else the branch commits meanwhile. */
export const commitKey = (commit: string): string => `row:${commitScope(commit)}`;

/** The changes of one scope (already in list order) grouped under their directories, directories
 * before changes at every level as the file tree orders paths, and single-child directory chains
 * compacted into one row. */
export function changeTree(scope: ChangeSection | CommitScope, items: readonly ChangeItem[]): ChangeNode[] {
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
    return { type: "directory", key: directoryKey(scope, directory.path), path: directory.path, name: displayWirePath(name), children: directory.children.map(toNode) };
  };
  return root.children.map(toNode);
}

/** The rows of `nodes` on screen, top to bottom: a collapsed directory or commit keeps its own row
 * and loses everything under it. */
export function visibleNodes(nodes: readonly ChangeNode[], collapsed: ReadonlySet<string>): ChangeNode[] {
  return nodes.flatMap((node) => (node.type !== "change" && !collapsed.has(node.key) ? [node, ...visibleNodes(node.children, collapsed)] : [node]));
}

/** One scope's changes as rows in `layout`: one per change when flat, the directory tree when `tree`. */
function layoutNodes(scope: ChangeSection | CommitScope, items: readonly ChangeItem[], layout: ChangeLayout): ChangeNode[] {
  return layout === "tree" ? changeTree(scope, items) : items.map((item) => ({ type: "change", key: item.key, item }));
}

/** The sections of the list, each with its changes and its rows in the layout's form. A section's
 * changes that are listed by commit (the committed one) are rows under their commits, newest first,
 * each holding its changes in the layout's form. */
export function changeGroups(items: readonly ChangeItem[], layout: ChangeLayout, kept: Iterable<ChangeSection> = []): (SectionGroup & { nodes: ChangeNode[] })[] {
  return sections(items, kept).map((group) => {
    if (!group.items.some((item) => item.commit)) return { ...group, nodes: layoutNodes(group.section, group.items, layout) };
    // The changes are in list order, so a commit's are together.
    const commits = new Map<string, { commit: CommitRef; items: ChangeItem[] }>();
    for (const item of group.items) {
      const held = commits.get(item.commit!.commit);
      if (held) held.items.push(item);
      else commits.set(item.commit!.commit, { commit: item.commit!, items: [item] });
    }
    const nodes = [...commits.values()].map(({ commit, items: own }): ChangeNode => ({
      type: "commit",
      key: commitKey(commit.commit),
      commit,
      count: own.length,
      children: layoutNodes(commitScope(commit.commit), own, layout),
    }));
    return { ...group, nodes };
  });
}

/** The changes with a row on screen, in the order the rows are: what the viewer moves through. A
 * folded-away section keeps its heading alone, so none of its changes are among them, in either
 * layout; those under a folded commit or, in the tree, a folded directory are left out as well. */
export function visibleChanges(items: readonly ChangeItem[], layout: ChangeLayout, collapsed: ReadonlySet<string>): ChangeItem[] {
  return changeGroups(items, layout).flatMap((group) =>
    collapsed.has(sectionKey(group.section))
      ? []
      : visibleNodes(group.nodes, collapsed).flatMap((node) => (node.type === "change" ? [node.item] : [])),
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
