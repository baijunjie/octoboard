import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import { useStableHandler } from "../../components/hooks/useLatest";
import type { ErrorPresentation } from "../../errors/errorPresentation";
import type { ViewerNavigation } from "../../viewer/viewerNavigation";
import type { ViewerSubject } from "../../viewer/content";
import type { ProjectBrowserState } from "../browserState";
import type { FileFilterState } from "../FileFilterField";
import { focusRow } from "../focusRow";
import { buildTree, compareFilePaths, fileEvidence, hasOpenFolder, listingSaysDirectory, parentPath, parseRowKey, rowKey, type DirListing } from "../tree";
import { neighboursIn, rereadFor, type Evidence } from "../viewedItem";
import { ROW_HEIGHT, scrollRowIntoView } from "../treeRow";
import { buildFileResults, resultDirectories } from "./filesView";
import { TREE_PADDING } from "./FileTree";
import { useDirectoryListings } from "./useDirectoryListings";
import { useFileReader } from "./useFileReader";
import { useFileSearch } from "./useFileSearch";

/** The key of the top-level folder a row of the tree is under, the row that stays on screen when
 * every folder folds; none for a row that is itself at the top level. A status row (loading, empty)
 * is under the folder it speaks of. */
function topLevelFolderKey(key: string): string | undefined {
  const parsed = parseRowKey(key);
  if (!parsed) return undefined;
  const path = parsed.kind === "path" ? parsed.path : parsed.dir;
  if (path === "") return undefined;
  const slash = path.indexOf("/");
  if (slash === -1) return parsed.kind === "path" ? undefined : rowKey(path);
  return rowKey(path.slice(0, slash));
}

export interface FilesMode {
  searching: boolean;
  /** Why the filter's search failed, while it has; `retrySearch` starts it again. */
  searchFailure: { message: ErrorPresentation } | undefined;
  retrySearch: () => void;
  /** Lists the directory `dir` ("" for the project's own) again, for the retry of a failed listing. */
  retryDirectory: (dir: string) => void;
  /** The listing of the project's own directory, if it is held. */
  root: DirListing | undefined;
  /** Whether what the panel waits for before it shows a tree is still out. */
  rootLoading: boolean;
  /** The rows on screen, and the row keys of the folders open among them. */
  tree: ReturnType<typeof buildTree>;
  expandedKeys: ReadonlySet<string>;
  onExpandedChange: (keys: Set<string>) => void;
  treeRef: React.RefObject<HTMLDivElement | null>;
  /** Whether a folder is open to fold, and folding them all. */
  canCollapseAll: boolean;
  collapseAll: () => void;
  /** Whether the filter's results were cut short. */
  searchIncomplete: boolean;
  /** Lists what is on screen again: the search while filtering, the directories otherwise. */
  refresh: () => Promise<void>;
  /** Selects `path` and shows it in the viewer, at the heading `fragment` names if given. */
  openFile: (path: string, fragment?: string) => void;
  /** Whether the listings say `path` is a directory, for the links of a document. */
  isDirectory: (path: string) => boolean;
  /** What the read-only viewer shows, while a file is open. */
  viewer: { subject: ViewerSubject; onClose: () => void; navigation: ViewerNavigation | undefined; anchor: string | undefined } | undefined;
}

/**
 * The Files mode of a project's browser: its directory listings or the filter's search results as
 * the tree's rows, the folders opened and folded, the selection kept in view, and the viewer that
 * opens over it. `active` is whether the browser is on screen and `shown` whether the Files mode is
 * the one it shows; only then are the listings refreshed on their own.
 *
 * The viewer moves through the tree's file rows on screen, top to bottom, and stops at either end.
 * What it shows is the selection, and closing it puts keyboard focus on that row. While it is open,
 * the listings decide when it reads its file again (`rereadFor`).
 */
export function useFilesMode(project: string, browser: ProjectBrowserState, filter: FileFilterState, active: boolean, shown: boolean): FilesMode {
  const [collapsedFiles, setCollapsedFiles] = useState<ReadonlySet<string>>(new Set());
  const reader = useFileReader(project);
  const treeRef = useRef<HTMLDivElement>(null);

  const query = filter.filter.trim();
  const searching = query !== "";
  const dirs = useDirectoryListings(project, active && shown && (!searching || reader.subject !== undefined));
  const search = useFileSearch(project, query, active && shown && searching, filter.includeIgnored);
  const directoryTree = useMemo(() => buildTree(dirs.listings, browser.expanded, browser.selected), [dirs.listings, browser.expanded, browser.selected]);
  const resultPaths = search.result.state === "loaded" ? search.result.paths : [];
  const resultIgnored = search.result.state === "loaded" ? search.result.ignored : undefined;
  const resultModel = useMemo(() => buildFileResults(resultPaths, browser.selected, collapsedFiles, resultIgnored), [resultPaths, browser.selected, collapsedFiles, resultIgnored]);
  const tree = searching ? resultModel.tree : directoryTree;
  const searchParents = reader.subject?.path.split("/").slice(0, -1) ?? [];
  const wantedDirectories = !searching
    ? directoryTree.wanted
    : reader.subject
      ? ["", ...searchParents.map((_, index) => searchParents.slice(0, index + 1).join("/"))]
      : [];
  const wantedKey = wantedDirectories.join("\0");
  useEffect(() => {
    if (shown) dirs.want(wantedDirectories);
  }, [wantedKey, shown, searching]);

  const directoryExpandedKeys = useMemo(() => new Set([...browser.expanded].map(rowKey)), [browser.expanded]);
  const expandedKeys = searching ? resultModel.expanded : directoryExpandedKeys;
  const onExpandedChange = (keys: Set<string>) => {
    if (searching) {
      const next = new Set(collapsedFiles);
      for (const dir of resultDirectories(resultPaths)) {
        if (keys.has(rowKey(dir))) next.delete(dir);
        else next.add(dir);
      }
      setCollapsedFiles(next);
      return;
    }
    const paths = new Set([...keys].map((key) => key.slice(rowKey("").length)));
    // A directory listed before is listed again as it opens; one never listed is asked for as it
    // comes on screen.
    const opened = [...paths].filter((path) => !browser.expanded.has(path) && dirs.listings.has(path));
    const next = new Set(collapsedFiles);
    for (const path of browser.expanded) if (!paths.has(path)) next.add(path);
    for (const path of paths) next.delete(path);
    setCollapsedFiles(next);
    browser.setExpanded(paths);
    dirs.refresh(opened);
  };

  // Folds every folder: the expanded ones of the tree, or every folder of the filter's results. A
  // focused row that goes with them hands focus to the top-level folder it was under, the nearest
  // row that stays.
  const collapseAll = () => {
    const focused = document.activeElement instanceof Element ? document.activeElement.closest<HTMLElement>("[role=row][data-key]") : null;
    const stays = focused && treeRef.current?.contains(focused) ? topLevelFolderKey(focused.dataset.key ?? "") : undefined;
    onExpandedChange(new Set());
    if (stays !== undefined) focusRow(treeRef.current, stays);
  };

  // A heading a document link asked to show, kept until another file is opened. Previous and Next
  // open a file with no heading, which clears it.
  const [pendingAnchor, setPendingAnchor] = useState<{ path: string; fragment?: string }>();
  const openFile = (path: string, fragment?: string) => {
    browser.setSelected(path);
    reader.open(path);
    setPendingAnchor(fragment === undefined ? undefined : { path, fragment });
  };

  const closeViewer = () => {
    reader.close();
    if (browser.selected !== undefined) focusRow(treeRef.current, rowKey(browser.selected));
  };

  // The selection is kept in view, so moving through files in the viewer moves the tree along
  // behind it, and its row is there to take focus when the viewer closes. Rows are one height, so
  // where a row is follows from its place in the list.
  const scrolledTo = useRef<string | undefined>(undefined);
  useLayoutEffect(() => {
    const selected = browser.selected;
    const element = treeRef.current;
    const scrollKey = `${query}\0${selected ?? ""}`;
    if (selected === undefined || scrollKey === scrolledTo.current || !element) return;
    const index = tree.rows.indexOf(rowKey(selected));
    if (index === -1) return;
    scrolledTo.current = scrollKey;
    scrollRowIntoView(element, TREE_PADDING + index * ROW_HEIGHT);
  }, [browser.selected, tree.rows, query]);

  // What the listings say of the file in the viewer decides whether it is read again (`rereadFor`):
  // gone, out of reach or at another version, or back after a failure. What they said the last
  // time, and the listing of the file's directory then, are kept per file, so only news leads to a
  // read. Nothing is taken as news while a read is still out; once it settles, the listings are
  // weighed again against what it brought.
  const lastEvidence = useRef<{ path: string; evidence: Evidence; folder?: DirListing } | undefined>(undefined);
  const viewedPath = reader.subject?.path;
  useEffect(() => {
    if (viewedPath === undefined || reader.shown.state === "loading") return;
    const evidence = fileEvidence(dirs.listings, viewedPath);
    const folder = dirs.listings.get(parentPath(viewedPath));
    const last = lastEvidence.current?.path === viewedPath ? lastEvidence.current : undefined;
    if (rereadFor(reader.shown, evidence, last?.evidence, folder !== last?.folder)) reader.reload();
    if (evidence.state !== "unknown") lastEvidence.current = { path: viewedPath, evidence, folder };
  }, [dirs.listings, viewedPath, reader.shown.state]);

  // One function for the component's life, so that `navigation` changes only with the neighbours.
  const openNeighbour = useStableHandler(openFile);
  const navigation = useMemo(() => {
    if (viewedPath === undefined) return undefined;
    const { previous, next } = neighboursIn(tree.files, viewedPath, (a, b) => a === b, compareFilePaths);
    return {
      onPrevious: previous === undefined ? undefined : () => openNeighbour(previous),
      onNext: next === undefined ? undefined : () => openNeighbour(next),
    };
  }, [tree.files, viewedPath, openNeighbour]);

  const root = dirs.listings.get("");
  return {
    searching,
    searchFailure: searching && search.result.state === "error" ? { message: search.result.message } : undefined,
    retrySearch: () => void search.refresh(),
    retryDirectory: (dir) => dirs.refresh([dir]),
    root,
    rootLoading: searching ? search.result.state === "loading" : root === undefined || root.state === "loading",
    tree,
    expandedKeys,
    onExpandedChange,
    treeRef,
    canCollapseAll: hasOpenFolder(tree.nodes, expandedKeys),
    collapseAll,
    searchIncomplete: searching && search.result.state === "loaded" && !search.result.complete,
    refresh: () => (searching ? search.refresh() : dirs.refreshAll()),
    openFile,
    isDirectory: (path) => listingSaysDirectory(dirs.listings, path),
    viewer: reader.subject && {
      subject: reader.subject,
      onClose: closeViewer,
      navigation,
      anchor: pendingAnchor && reader.subject.path === pendingAnchor.path ? pendingAnchor.fragment : undefined,
    },
  };
}
