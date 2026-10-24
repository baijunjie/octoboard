import type { BrowseEntry } from "../protocol";
import { compareFilePaths, rowKey, treeEntries, type TreeModel, type TreeNode } from "./tree";

export function resultDirectories(paths: readonly string[]): string[] {
  const directories = new Set<string>();
  for (const path of paths) {
    for (let slash = path.indexOf("/"); slash !== -1; slash = path.indexOf("/", slash + 1)) {
      directories.add(path.slice(0, slash));
    }
  }
  return [...directories].sort(compareFilePaths);
}

/** The results of a file-name search as a tree. `ignored` are the result files Git ignores; a folder
 * counts as ignored when every result beneath it is. */
export function buildFileResults(
  paths: readonly string[],
  selected: string | undefined,
  collapsed: ReadonlySet<string>,
  ignored: ReadonlySet<string> = new Set(),
): { tree: TreeModel; expanded: Set<string> } {
  const sorted = [...new Set(paths)].sort(compareFilePaths);
  const ignoredDirectories = new Set<string>();
  if (ignored.size > 0) {
    const holds = new Map<string, { all: number; ignored: number }>();
    for (const path of sorted) {
      for (let slash = path.indexOf("/"); slash !== -1; slash = path.indexOf("/", slash + 1)) {
        const count = holds.get(path.slice(0, slash)) ?? { all: 0, ignored: 0 };
        count.all += 1;
        if (ignored.has(path)) count.ignored += 1;
        holds.set(path.slice(0, slash), count);
      }
    }
    for (const [directory, count] of holds) if (count.all === count.ignored) ignoredDirectories.add(directory);
  }
  const entry = (name: string, kind: "file" | "directory", path: string): BrowseEntry => ({
    name,
    kind,
    size: null,
    version: null,
    target: null,
    ...((kind === "file" ? ignored.has(path) : ignoredDirectories.has(path)) && { ignored: true }),
  });
  const file = (path: string, name: string): TreeNode => ({ type: "file", key: rowKey(path), path, selected: path === selected, entry: entry(name, "file", path) });
  const directories = resultDirectories(sorted);
  const directorySet = new Set(directories);
  const expanded = new Set(directories.filter((path) => !collapsed.has(path)).map(rowKey));
  const children = new Map<string, BrowseEntry[]>();
  for (const path of [...directories, ...sorted]) {
    const slash = path.lastIndexOf("/");
    const parent = slash === -1 ? "" : path.slice(0, slash);
    const name = path.slice(slash + 1);
    const entries = children.get(parent) ?? [];
    entries.push(entry(name, directorySet.has(path) ? "directory" : "file", path));
    children.set(parent, entries);
  }
  const files: string[] = [];
  const rows: string[] = [];
  const visit = (parent: string): TreeNode[] => treeEntries(children.get(parent) ?? []).map((item) => {
    const path = parent ? `${parent}/${item.name}` : item.name;
    rows.push(rowKey(path));
    if (item.kind === "directory") {
      return { type: "directory", key: rowKey(path), path, entry: item, children: collapsed.has(path) ? [] : visit(path) };
    }
    files.push(path);
    return file(path, item.name);
  });
  return { tree: { nodes: visit(""), files, rows, wanted: [] }, expanded };
}
