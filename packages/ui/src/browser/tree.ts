// The project browser's tree rules: which entries it lists and in what order, the rows that make
// up what is on screen, and how the viewer moves through the files among them. Pure, so the tree's
// rendering and the viewer's navigation are two readings of one structure and can never disagree.
import type { ErrorPresentation } from "../errorPresentation";
import type { BrowseEntry } from "../protocol";
import { displayWirePath } from "../wirePath";

/** One directory of the project as the client holds it. `loaded` keeps its entries while a newer
 * listing of it is on the way (`refreshing`), so a refresh never blanks the tree. */
export type DirListing =
  | { state: "loading" }
  | { state: "loaded"; entries: BrowseEntry[]; complete: boolean; refreshing: boolean }
  | { state: "error"; message: ErrorPresentation };

/** Names the tree leaves out: Git's own metadata and the files an operating system leaves in any
 * folder it shows, never project content. Everything else is listed, ignored files included, since
 * the daemon reports no ignore rules and an agent's build output is often exactly what the user
 * wants to look at. The list is stated in docs/product/project-pane.md's "What is listed" section.
 * Kept in lowercase, the form `isHiddenName` folds a name to before looking it up. */
const HIDDEN_NAMES = new Set([
  ".git",
  ".ds_store",
  ".appledouble",
  ".spotlight-v100",
  ".trashes",
  ".fseventsd",
  ".temporaryitems",
  "thumbs.db",
  "desktop.ini",
]);

/** Whether the tree leaves out an entry of this name, ignoring ASCII case: the systems that write
 * these files do not keep to one spelling (Windows writes `Desktop.ini` too). Only ASCII letters are
 * folded, so a name with some other character that lowercases to one of them is still listed. `._`
 * starts the AppleDouble files macOS writes beside each file on a volume that cannot hold its
 * metadata. */
function isHiddenName(name: string): boolean {
  return HIDDEN_NAMES.has(name.replace(/[A-Z]/g, (letter) => letter.toLowerCase())) || name.startsWith("._");
}

/** Whether an entry opens as a directory: one, or a link to one inside the project. */
export function isDirectory(entry: BrowseEntry): boolean {
  return entry.kind === "directory" || (entry.kind === "symlink" && entry.target === "directory");
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

/** Two names in tree order: by their display text, case-insensitively and in natural order ("item 2"
 * before "item 10"), as the sidebar orders names; two names that read the same are told apart by
 * their wire forms, so the order is total and stable. */
function compareNames(a: string, b: string): number {
  return collator.compare(displayWirePath(a), displayWirePath(b)) || (a < b ? -1 : a > b ? 1 : 0);
}

/** Directories before everything else, then by name. */
function compareEntries(a: BrowseEntry, b: BrowseEntry): number {
  return Number(isDirectory(b)) - Number(isDirectory(a)) || compareNames(a.name, b.name);
}

/** A listing's entries as the tree shows them: the hidden names left out, in tree order. */
export function treeEntries(entries: BrowseEntry[]): BrowseEntry[] {
  return entries.filter((entry) => !isHiddenName(entry.name)).sort(compareEntries);
}

/** The wire path of `name` inside the directory at `dir` (empty for the project's own). */
export function childPath(dir: string, name: string): string {
  return dir === "" ? name : `${dir}/${name}`;
}

/** Whether a loaded listing says `path` is a directory. An unlisted path is not: the caller may
 * still try to open it as a file. */
export function listingSaysDirectory(listings: ReadonlyMap<string, DirListing>, path: string): boolean {
  const parent = parentPath(path);
  const name = parent === "" ? path : path.slice(parent.length + 1);
  const listing = listings.get(parent);
  if (!listing || listing.state !== "loaded") return false;
  const entry = listing.entries.find((item) => item.name === name);
  return entry ? isDirectory(entry) : false;
}

/** The directory holding `path`, empty for an entry of the project's own directory. */
export function parentPath(path: string): string {
  const slash = path.lastIndexOf("/");
  return slash === -1 ? "" : path.slice(0, slash);
}

/**
 * Two file paths in the order the tree shows them, whatever is expanded: at the first component
 * where they part, the one that goes on below it is a directory there and comes first, and two
 * components of the same kind go by name. This is what places a file the tree no longer lists among
 * the ones it does.
 */
export function compareFilePaths(a: string, b: string): number {
  const left = a.split("/");
  const right = b.split("/");
  for (let i = 0; i < Math.min(left.length, right.length); i++) {
    if (left[i] === right[i]) continue;
    const leftIsDir = i < left.length - 1;
    const rightIsDir = i < right.length - 1;
    return Number(rightIsDir) - Number(leftIsDir) || compareNames(left[i], right[i]);
  }
  return left.length - right.length;
}

export type StatusRow = "loading" | "empty";

/** One row of the tree. A `file` without an `entry` is the selected file where its directory no
 * longer lists it: `removed` when the listing is complete, `unlisted` when it was cut short. An
 * expanded `directory` whose listing failed has no children and carries the failure, which its own
 * row marks; one whose listing was cut short is `partial`, which its own row marks too. */
export type TreeNode =
  | { type: "directory"; key: string; path: string; entry: BrowseEntry; children: TreeNode[]; failure?: ErrorPresentation; partial?: true }
  | { type: "file"; key: string; path: string; entry?: BrowseEntry; missing?: "removed" | "unlisted"; selected: boolean }
  | { type: "status"; key: string; dir: string; status: StatusRow };

/** Whether the top-level `nodes` hold a folder drawn open: expanded and not failed to list. A folder
 * deeper down is shown only under open parents, so it needs no look of its own; what `expanded`
 * holds beneath a folded folder, or on a failed one, is not on screen. */
export function hasOpenFolder(nodes: readonly TreeNode[], expanded: ReadonlySet<string>): boolean {
  return nodes.some((node) => node.type === "directory" && expanded.has(node.key) && node.failure === undefined);
}

export interface TreeModel {
  /** The rows under the project's directory; empty until its listing has loaded. */
  nodes: TreeNode[];
  /** The file rows on screen, top to bottom: what the viewer moves through. */
  files: string[];
  /** Every row key on screen, top to bottom, for scrolling one into view. */
  rows: string[];
  /** The directories on screen whose listing is wanted: the project's own and every expanded one
   * whose parents are all expanded and listed. */
  wanted: string[];
}

export const rowKey = (path: string): string => `path:${path}`;
const statusKey = (dir: string, status: StatusRow): string => `status:${status}:${dir}`;

/**
 * The rows on screen for the listings held and the directories expanded. A directory's rows follow
 * its own, after its status while it is loading or empty (an unreadable one has no rows, and its own
 * row carries the failure, as a cut-short one carries that it is); a collapsed
 * directory's descendants are not rows at all. `selected` is shown where its directory is on screen
 * even when that directory's listing no longer has it, so a selection that went away stays visible
 * as such.
 */
export function buildTree(
  listings: ReadonlyMap<string, DirListing>,
  expanded: ReadonlySet<string>,
  selected: string | undefined,
): TreeModel {
  const files: string[] = [];
  const rows: string[] = [];
  const wanted: string[] = [];

  const status = (dir: string, row: StatusRow): TreeNode => {
    rows.push(statusKey(dir, row));
    return { type: "status", key: statusKey(dir, row), dir, status: row };
  };

  const childrenOf = (dir: string): TreeNode[] => {
    wanted.push(dir);
    const listing = listings.get(dir);
    if (!listing || listing.state === "loading") return dir === "" ? [] : [status(dir, "loading")];
    if (listing.state === "error") return [];
    const entries: (BrowseEntry | string)[] = [...listing.entries];
    const missing = selected !== undefined && parentPath(selected) === dir && !listing.entries.some((e) => childPath(dir, e.name) === selected);
    if (missing) {
      const ghost: BrowseEntry = { name: selected.slice(dir === "" ? 0 : dir.length + 1), kind: "file", size: null, version: null, target: null };
      const at = listing.entries.findIndex((entry) => compareEntries(ghost, entry) < 0);
      entries.splice(at === -1 ? entries.length : at, 0, selected);
    }
    const nodes: TreeNode[] = [];
    for (const item of entries) {
      if (typeof item === "string") {
        rows.push(rowKey(item));
        files.push(item);
        nodes.push({ type: "file", key: rowKey(item), path: item, missing: listing.complete ? "removed" : "unlisted", selected: true });
        continue;
      }
      const path = childPath(dir, item.name);
      rows.push(rowKey(path));
      if (isDirectory(item)) {
        const children = expanded.has(path) ? childrenOf(path) : [];
        const listed = expanded.has(path) ? listings.get(path) : undefined;
        nodes.push({ type: "directory", key: rowKey(path), path, entry: item, children, ...(listed?.state === "error" && { failure: listed.message }), ...(listed?.state === "loaded" && !listed.complete && { partial: true }) });
      } else {
        files.push(path);
        nodes.push({ type: "file", key: rowKey(path), path, entry: item, selected: path === selected });
      }
    }
    if (dir !== "") {
      if (listing.entries.length === 0 && !missing) nodes.push(status(dir, "empty"));
    }
    return nodes;
  };

  return { nodes: childrenOf(""), files, rows, wanted };
}

/** The files before and after `current` among `files` (the tree's file rows in order). A file the
 * tree does not show is placed where it would be, so moving on from it still goes to its neighbours;
 * the ends do not wrap. */
export function neighbours(files: readonly string[], current: string): { previous?: string; next?: string } {
  const index = files.indexOf(current);
  if (index !== -1) return { previous: files[index - 1], next: files[index + 1] };
  const after = files.findIndex((path) => compareFilePaths(current, path) < 0);
  const insertAt = after === -1 ? files.length : after;
  return { previous: files[insertAt - 1], next: files[insertAt] };
}

/** What the listings held say about the file at `path`: listed, with its version; gone from a
 * complete listing of its directory or of a directory above it; under a directory that cannot be
 * listed; or nothing yet (a listing still loading, refreshing or cut short before the name). */
export type FileEvidence = { state: "listed"; version: string | null } | { state: "gone" } | { state: "unreadable" } | { state: "unknown" };

export function fileEvidence(listings: ReadonlyMap<string, DirListing>, path: string): FileEvidence {
  const parts = path.split("/");
  for (let depth = 0; depth < parts.length; depth++) {
    const listing = listings.get(parts.slice(0, depth).join("/"));
    if (!listing || listing.state === "loading" || (listing.state === "loaded" && listing.refreshing)) return { state: "unknown" };
    if (listing.state === "error") return { state: "unreadable" };
    const entry = listing.entries.find((e) => e.name === parts[depth]);
    if (!entry) return listing.complete ? { state: "gone" } : { state: "unknown" };
    if (depth === parts.length - 1) return { state: "listed", version: entry.version };
  }
  return { state: "unknown" };
}

/** What the viewer shows of a file, as far as reading it again goes: its body at `version`, a
 * failure (`changing` when the file kept changing while it was read), or nothing yet. */
export type ShownFile = { state: "file"; version: string } | { state: "error"; changing?: boolean } | { state: "loading" };

/**
 * Whether the file the viewer shows has to be read again, given what the listings now say of it
 * (`evidence`) and what they said when this was last asked for the same file (`previous`). Only news
 * counts, so a read that fails again is not repeated until the file changes once more: a body is read
 * again when its file is gone, cannot be reached, or is listed at another version (a rewrite of the
 * same size, or another file put in its place, included); a failure is read again when the file is
 * listed anew, so a file that comes back shows again, and at every new listing of its directory
 * (`freshListing`) that has it while it kept changing as it was read, so it shows once it settles.
 */
export function rereadFor(shown: ShownFile, evidence: FileEvidence, previous: FileEvidence | undefined, freshListing: boolean): boolean {
  if (shown.state === "loading" || evidence.state === "unknown") return false;
  const key = (e: FileEvidence | undefined) => (e?.state === "listed" ? `listed:${e.version}` : e?.state);
  if (shown.state === "error" && shown.changing) return freshListing && evidence.state === "listed";
  if (key(evidence) === key(previous)) return false;
  if (shown.state === "error") return evidence.state === "listed";
  if (evidence.state !== "listed") return true;
  return evidence.version !== null && evidence.version !== shown.version;
}
