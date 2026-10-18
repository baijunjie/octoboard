import { Button, Spinner, Tabs } from "@heroui/react";
import { FolderOpen, FolderX, RefreshCw, SearchX } from "lucide-react";
import React, { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { Key } from "react-aria-components";

import { DaemonErrorMessage } from "../components/DaemonErrorMessage";
import { CenteredEmptyPanel } from "../components/EmptyPanel";
import { ErrorAlert } from "../components/ErrorAlert";
import { FadeOverflow } from "../components/FadeOverflow";
import { handFocusOff } from "../components/handFocusOff";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { FADE_SIZE } from "../components/scrollFade";
import { useTabScrollLabels } from "../components/tabScrollLabels";
import { TitledControl } from "../components/TitledControl";
import { ROW_ICON_BUTTON_CLASS, RowIconGroup } from "../components/RowControls";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { useStableHandler } from "../components/useLatest";
import type { ErrorPresentation } from "../errorPresentation";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import { AsidePane, type AsideLayout } from "../layout/AsidePane";
import type { Project } from "../protocol";
import { useDaemonStore } from "../store";
import { FileViewer } from "../viewer/FileViewer";
import { useProjectBrowserState } from "./browserState";
import { FileFilterField, useFileFilterState } from "./FileFilterField";
import { BrowserViewControls } from "./BrowserViewControls";
import { buildFileResults, resultDirectories } from "./filesView";
import { FileTree } from "./FileTree";
import { focusRow } from "./focusRow";
import { GitView } from "./GitView";
import { PaneNote } from "./paneNote";
import { buildTree, fileEvidence, listingSaysDirectory, neighbours, parentPath, rereadFor, rowKey, type DirListing, type FileEvidence } from "./tree";
import { RowPathScope } from "./RowActions";
import { ROW_HEIGHT } from "./treeRow";
import { useDirectoryListings } from "./useDirectoryListings";
import { useFileSearch } from "./useFileSearch";
import { useFileReader } from "./useFileReader";
import { useGitReview } from "./useGitReview";

/** The vertical padding the tree's scroller adds above its first row (`py-1`). */
const TREE_PADDING = 4;

/** How long the header's Refresh icon spins at least, so a refresh that settles at once still reads
 * as having happened: half a turn of the slow spin (2 s per turn). The icon looks the same every
 * half turn, so stopping there does not visibly jump. */
const MIN_SPIN_MS = 1000;

/**
 * A project's browser, the aside's content while the project owns it (`asideOwner.ts`), in two
 * modes: Files, the project's files live from its directory, and Git, the changes of one of its
 * repository's branches, uncommitted and committed (`useGitReview`); the read-only viewer opens
 * over any of them. It reads only the project, bar the selected session,
 * which the Git mode marks the branches of that it created and, when it is one of this project's,
 * follows in its selector.
 * The caller mounts one per project (keyed by its id), so nothing read for one project can land in
 * another's; what belongs to the project across mounts — the mode, the expanded directories, the
 * selections — is in `browserState.ts`. Only the mode on screen is refreshed on its own.
 *
 * In Files, the viewer moves through the tree's file rows on screen, top to bottom, and stops at
 * either end. What it shows is the selection, kept in view behind it, and closing it puts keyboard
 * focus on that row. While it is open, the listings decide when it reads its file again
 * (`rereadFor`).
 */
export function ProjectBrowser({
  project,
  layout,
  active,
  selectedSessionId,
}: {
  project: Project;
  layout: AsideLayout;
  /** Whether the browser is on screen, docked, as an open drawer or floating in; only then is it
   * refreshed on its own. */
  active: boolean;
  /** The session selected in the sidebar: the Git mode's branch lists mark the branches it
   * created, and its newest one fills the Git mode's selector. Nothing else here reads a
   * session. */
  selectedSessionId: string | undefined;
}): React.ReactElement {
  const t = useT();
  const tabScrollLabels = useTabScrollLabels();
  const browser = useProjectBrowserState(project.id);
  const mode = browser.mode;

  const filters = { files: useFileFilterState(), changes: useFileFilterState() };
  const tab = mode === "files" ? "files" : "changes";
  const activeFilter = filters[tab];
  const fileFilter = filters.files;
  const [collapsedFiles, setCollapsedFiles] = useState<ReadonlySet<string>>(new Set());
  const reader = useFileReader(project.id);
  // The selected session sets the Git mode's selector, when it is one of this project's.
  const sessionHere = useDaemonStore((s) => (selectedSessionId !== undefined && s.sessions.get(selectedSessionId)?.project_id === project.id ? selectedSessionId : undefined));
  const git = useGitReview(project.id, browser, active && mode === "git", sessionHere, filters.changes);
  const treeRef = useRef<HTMLDivElement>(null);
  const refreshRef = useRef<HTMLButtonElement>(null);
  const filesFocus = useFocusHandoff(() => refreshRef.current?.focus());

  const query = fileFilter.filter.trim();
  const searching = query !== "";
  const dirs = useDirectoryListings(project.id, active && mode === "files" && (!searching || reader.subject !== undefined));
  const search = useFileSearch(project.id, query, active && mode === "files" && searching);
  const directoryTree = useMemo(() => buildTree(dirs.listings, browser.expanded, browser.selected), [dirs.listings, browser.expanded, browser.selected]);
  const resultPaths = search.result.state === "loaded" ? search.result.paths : [];
  const resultModel = useMemo(() => buildFileResults(resultPaths, browser.selected, collapsedFiles), [resultPaths, browser.selected, collapsedFiles]);
  const tree = searching ? resultModel.tree : directoryTree;
  const searchParents = reader.subject?.path.split("/").slice(0, -1) ?? [];
  const wantedDirectories = !searching
    ? directoryTree.wanted
    : reader.subject
      ? ["", ...searchParents.map((_, index) => searchParents.slice(0, index + 1).join("/"))]
      : [];
  const wantedKey = wantedDirectories.join("\0");
  useEffect(() => {
    if (mode === "files") dirs.want(wantedDirectories);
  }, [wantedKey, mode, searching]);
  useEffect(() => browser.touch(), []);

  const directoryExpandedKeys = useMemo(() => new Set([...browser.expanded].map(rowKey)), [browser.expanded]);
  const expandedKeys = searching ? resultModel.expanded : directoryExpandedKeys;
  const onExpandedChange = (keys: Set<Key>) => {
    if (searching) {
      const next = new Set(collapsedFiles);
      for (const dir of resultDirectories(resultPaths)) {
        if (keys.has(rowKey(dir))) next.delete(dir);
        else next.add(dir);
      }
      setCollapsedFiles(next);
      return;
    }
    const paths = new Set([...keys].map((key) => String(key).slice(rowKey("").length)));
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

  // A heading a document link asked to show, kept until another file is opened. Previous and Next
  // open a file with no heading, which clears it.
  const [pendingAnchor, setPendingAnchor] = useState<{ path: string; fragment?: string }>();
  const openFile = (path: string, fragment?: string) => {
    browser.setSelected(path);
    reader.open(path);
    setPendingAnchor(fragment === undefined ? undefined : { path, fragment });
  };
  // A document link replaces the file on screen. Closing the change viewer here does not move
  // focus to its row: that would take focus out of the file viewer that is about to show.
  const openLinkedFile = (path: string, fragment?: string) => {
    git.dismissViewer();
    browser.setMode("files");
    openFile(path, fragment);
  };

  const closeViewer = () => {
    reader.close();
    if (browser.selected !== undefined) focusRow(treeRef.current, rowKey(browser.selected));
  };

  const refresh = () => (mode === "files" ? searching ? search.refresh() : dirs.refreshAll() : git.refresh());

  const branch = useDaemonStore((state) => {
    const status = state.gitStatuses.get(project.id);
    return status ? `${status.detached}:${status.branch ?? ""}` : undefined;
  });
  const refreshedBranch = useRef({ files: branch, git: branch });
  useEffect(() => {
    if (!active || refreshedBranch.current[mode] === branch) return;
    refreshedBranch.current[mode] = branch;
    refresh();
  }, [active, branch, mode]);

  // The Refresh icon spins while a refresh the button started is out, and for at least
  // `MIN_SPIN_MS`. Only the button's own refreshes count: the periodic ones would keep it turning.
  // Pressing again while it spins starts another refresh, and the icon spins until the last settles.
  const [spinning, setSpinning] = useState(0);
  const pressRefresh = () => {
    setSpinning((n) => n + 1);
    void Promise.all([refresh(), new Promise((resolve) => setTimeout(resolve, MIN_SPIN_MS))]).finally(() => setSpinning((n) => n - 1));
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
    const top = TREE_PADDING + index * ROW_HEIGHT;
    // Clear of the fade at the tree's edges, as `scroll-padding` keeps the rows react-aria scrolls.
    if (top - FADE_SIZE < element.scrollTop) element.scrollTop = top - FADE_SIZE;
    else if (top + ROW_HEIGHT + FADE_SIZE > element.scrollTop + element.clientHeight) element.scrollTop = top + ROW_HEIGHT + FADE_SIZE - element.clientHeight;
  }, [browser.selected, tree.rows, query]);

  // What the listings say of the file in the viewer decides whether it is read again (`rereadFor`):
  // gone, out of reach or at another version, or back after a failure. What they said the last
  // time, and the listing of the file's directory then, are kept per file, so only news leads to a
  // read. Nothing is taken as news while a read is still out; once it settles, the listings are
  // weighed again against what it brought.
  const lastEvidence = useRef<{ path: string; evidence: FileEvidence; folder?: DirListing } | undefined>(undefined);
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
    const { previous, next } = neighbours(tree.files, viewedPath);
    return {
      onPrevious: previous === undefined ? undefined : () => openNeighbour(previous),
      onNext: next === undefined ? undefined : () => openNeighbour(next),
    };
  }, [tree.files, viewedPath, openNeighbour]);

  const root = dirs.listings.get("");
  const rootLoading = searching ? search.result.state === "loading" : root === undefined || root.state === "loading";
  const loadingLabel = t(searching ? "browser.filter.searching" : "browser.loading");
  return (
    <AsidePane layout={layout} label={t("browser.label", { project: isolate(project.name) })} className="control-fills">
      <div className="project-browser-entry relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
        <Tabs
          selectedKey={tab}
          onSelectionChange={(key) => browser.setMode(key === "changes" ? "git" : "files")}
          className="project-browser-view flex min-h-0 flex-1 flex-col gap-0"
        >
          <div className="flex h-(--pane-header-height) shrink-0 items-center gap-1 border-b border-separator px-(--pane-inset) text-sm">
            <FolderOpen aria-hidden="true" className="size-4 shrink-0 text-app-muted" />
            <h2 className="flex min-w-0 max-w-40 font-medium">
              <FadeOverflow as="span" dir="auto" className="min-w-0 flex-1" titleWhenClipped={project.name}>{project.name}</FadeOverflow>
            </h2>
            <Tabs.ListContainer className="shrink-0 rounded-none bg-transparent" render={(props) => <div {...props}>{tabScrollLabels(props.children)}</div>}>
              {/* The scroller clips outside this padding; leave room for the focus ring and its offset. */}
              <Tabs.List aria-label={t("browser.modes")} className="w-max min-w-0 p-1.5">
                <Tabs.Tab id="files" className="h-7 scroll-mx-1.5 px-1.5 whitespace-nowrap hover:text-foreground hover:opacity-100">{t("browser.mode.files")}<Tabs.Indicator /></Tabs.Tab>
                <Tabs.Tab id="changes" className="h-7 scroll-mx-1.5 px-1.5 whitespace-nowrap hover:text-foreground hover:opacity-100">{t("git.view.changes")}<Tabs.Indicator /></Tabs.Tab>
              </Tabs.List>
            </Tabs.ListContainer>
            <RowIconGroup className="ms-auto">
              <BrowserViewControls
                filter={activeFilter}
                layout={mode === "git" ? { value: browser.layout, onChange: browser.setLayout } : undefined}
                byCommit={mode === "git" ? { value: browser.byCommit, onChange: browser.setByCommit } : undefined}
              />
              <TitledControl title={t("browser.refresh")}>
                <Button ref={refreshRef} isIconOnly size="sm" variant="ghost" className={ROW_ICON_BUTTON_CLASS} aria-label={t("browser.refresh")} preventFocusOnPress onPress={pressRefresh}>
                  <RefreshCw aria-hidden="true" className={spinning > 0 ? "size-4 motion-safe:animate-spin-slow" : "size-4"} />
                </Button>
              </TitledControl>
            </RowIconGroup>
          </div>
          <FileFilterField contextKey={tab} state={activeFilter} label={t("browser.filter.label")} placeholder={t(mode === "files" ? "browser.filter.placeholder" : "git.filter.placeholder")} />
          {mode === "files" ? <Tabs.Panel id="files" className="mt-0 flex min-h-0 flex-1 flex-col p-0">
            <div className="flex min-h-0 flex-1 flex-col" onFocus={filesFocus.onFocus} onBlur={filesFocus.onBlur}>
              <StatusAnnouncer text={rootLoading ? loadingLabel : undefined} />
              {rootLoading ? (
                <div aria-hidden="true" className="flex flex-1 items-center justify-center gap-2 text-sm text-app-muted">
                  <Spinner size="sm" aria-hidden="true" />
                  {loadingLabel}
                </div>
              ) : searching && search.result.state === "error" ? (
                <RootError title={t("browser.filter.error")} message={search.result.message} onRetry={(from) => { handFocusOff(from, refreshRef.current, true); search.refresh(); }} />
              ) : !searching && root?.state === "error" ? (
                <RootError
                  message={root.message}
                  onRetry={(from) => {
                  // The error and its button go as the listing starts again; focus on it moves to the
                  // header's Refresh, which stays.
                    handFocusOff(from, refreshRef.current, true);
                    dirs.refresh([""]);
                  }}
                />
              ) : tree.nodes.length === 0 && (searching || root?.state === "loaded" && root.complete) ? (
                <>
                  <CenteredEmptyPanel icon={searching ? SearchX : FolderX} message={t(searching ? "browser.filter.noMatch" : "browser.empty")} />
                  {searching && search.result.state === "loaded" && !search.result.complete && <PaneNote divided>{t("browser.filter.partial")}</PaneNote>}
                </>
              ) : (
                <>
                  <RowPathScope.Provider value={{ project: project.id }}>
                    <FileTree
                      nodes={tree.nodes}
                      expanded={expandedKeys}
                      label={t("browser.tree.label")}
                      onExpandedChange={onExpandedChange}
                      onOpenFile={openFile}
                      onRetry={(dir) => dirs.refresh([dir])}
                      treeRef={treeRef}
                    />
                  </RowPathScope.Provider>
                  {(searching ? search.result.state === "loaded" && !search.result.complete : root?.state === "loaded" && !root.complete) && <PaneNote divided>{t(searching ? "browser.filter.partial" : "browser.partial")}</PaneNote>}
                </>
              )}
            </div>
          </Tabs.Panel> : <Tabs.Panel id="changes" className="mt-0 flex min-h-0 flex-1 flex-col p-0">
            <RowPathScope.Provider value={{ project: project.id, worktree: git.worktree }}>
              <GitView
                {...git.view}
                selectedSessionId={selectedSessionId}
                onRetry={(from) => {
                  // The failure and its button go as the request starts again; focus on it moves to
                  // the header's Refresh, which stays.
                  handFocusOff(from, refreshRef.current, true);
                  refresh();
                }}
              />
            </RowPathScope.Provider>
          </Tabs.Panel>}
        </Tabs>
      </div>
      {reader.subject && (
        <FileViewer
          subject={reader.subject}
          onClose={closeViewer}
          navigation={navigation}
          location={{
            project: project.id,
            anchor: pendingAnchor && reader.subject.path === pendingAnchor.path ? pendingAnchor.fragment : undefined,
            onOpenFile: openLinkedFile,
            isDirectory: (path) => listingSaysDirectory(dirs.listings, path),
          }}
        />
      )}
      {git.viewer && (
        <FileViewer
          {...git.viewer}
          location={{
            project: project.id,
            onOpenFile: openLinkedFile,
            isDirectory: (path) => listingSaysDirectory(dirs.listings, path),
          }}
        />
      )}
    </AsidePane>
  );
}

/** The project's own directory could not be listed: why, and a way to try again. */
function RootError({ message, onRetry, title }: { title?: string; message: ErrorPresentation; onRetry: (from: Element | null) => void }): React.ReactElement {
  const t = useT();
  const ref = useRef<HTMLDivElement>(null);
  return (
    <div ref={ref} className="flex min-w-0 flex-1 flex-col items-center justify-center gap-3 p-4 text-sm">
      <ErrorAlert title={title ?? t("browser.error.root")} className="w-full max-w-lg">
        <DaemonErrorMessage error={message} showTitle={false} />
      </ErrorAlert>
      <Button size="sm" variant="secondary" preventFocusOnPress onPress={() => onRetry(ref.current)}>
        {t("error.tryAgain")}
      </Button>
    </div>
  );
}
