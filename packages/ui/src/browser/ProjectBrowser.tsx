import { Button, Spinner, Tabs } from "@heroui/react";
import { FolderOpen, FolderX, RefreshCw, TriangleAlert } from "lucide-react";
import React, { useEffect, useLayoutEffect, useMemo, useRef } from "react";
import type { Key } from "react-aria-components";

import { EmptyPanel } from "../components/EmptyPanel";
import { handFocusOff } from "../components/handFocusOff";
import { FadeOverflow } from "../components/FadeOverflow";
import { TitledControl } from "../components/TitledControl";
import { useT } from "../i18n/react";
import { AsidePane, type AsideLayout } from "../layout/AsidePane";
import type { Project } from "../protocol";
import { FileViewer } from "../viewer/FileViewer";
import { useProjectBrowserState, type BrowserMode } from "./browserState";
import { CHANGE_ROW_HEIGHT, changeRowOffset } from "./ChangeList";
import { changeEvidence, changeNeighbours, rereadChange, type ChangeEvidence, type ChangeItem } from "./changes";
import { FileTree, ROW_HEIGHT } from "./FileTree";
import { GitView } from "./GitView";
import { buildTree, fileEvidence, neighbours, parentPath, rereadFor, rowKey, type DirListing, type FileEvidence } from "./tree";
import { useChangeList } from "./useChangeList";
import { useChangeReader } from "./useChangeReader";
import { useDirectoryListings } from "./useDirectoryListings";
import { useFileReader } from "./useFileReader";
import { useProjectSource } from "./useProjectSource";

/** The vertical padding the tree's scroller adds above its first row (`py-1`). */
const TREE_PADDING = 4;

/**
 * A project's browser, the aside's content while the project owns it (`asideOwner.ts`), in two
 * modes: Files, the project's files live from its directory, and Git, a worktree's uncommitted
 * changes to them; the read-only viewer opens over either. It needs no session: the project is all
 * it reads from. The caller mounts one per project (keyed by its id), so nothing read for one
 * project can land in another's; what belongs to the project across mounts — the mode, the expanded
 * directories, the selections — is in `browserState.ts`. Only the mode on screen is refreshed on
 * its own.
 *
 * The viewer moves through the rows on screen, top to bottom, and stops at either end: the tree's
 * file rows, or the change list's rows, from one section into the next. What it shows is the
 * selection, kept in view behind it, and closing it puts keyboard focus on that row. While it is
 * open, the listings decide when it reads its subject again (`rereadFor`, `rereadChange`).
 */
export function ProjectBrowser({
  project,
  layout,
  active,
}: {
  project: Project;
  layout: AsideLayout;
  /** Whether the browser is on screen, docked, as an open drawer or floating in; only then is it
   * refreshed on its own. */
  active: boolean;
}): React.ReactElement {
  const t = useT();
  const browser = useProjectBrowserState(project.id);
  const mode = browser.mode;
  const dirs = useDirectoryListings(project.id, active && mode === "files");
  const reader = useFileReader(project.id);
  const source = useProjectSource(project.id, active && mode === "git");
  // Changes are listed only once the source says there is a repository with the chosen worktree in
  // it: a project in none, or a worktree that has gone, has nothing to ask for until that changes.
  const git = source.source.state === "loaded" ? source.source.source.git : null;
  const listable = git !== null && (browser.worktree === undefined || git.worktrees.some((w) => w.id === browser.worktree));
  const changes = useChangeList(project.id, browser.worktree, active && mode === "git" && listable);
  const changeReader = useChangeReader(project.id);
  const treeRef = useRef<HTMLDivElement>(null);
  const changeListRef = useRef<HTMLDivElement>(null);
  const headerRef = useRef<HTMLDivElement>(null);

  const tree = useMemo(() => buildTree(dirs.listings, browser.expanded, browser.selected), [dirs.listings, browser.expanded, browser.selected]);
  const wantedKey = tree.wanted.join("\0");
  useEffect(() => {
    if (mode === "files") dirs.want(tree.wanted);
  }, [wantedKey, mode]);
  useEffect(() => browser.touch(), []);

  // A worktree found gone may have been replaced by another at its place, or others added: what
  // the selector offers is asked for again.
  useEffect(() => {
    if (changes.list.state === "error" && changes.list.unavailable) source.refresh();
  }, [changes.list]);

  const expandedKeys = useMemo(() => new Set([...browser.expanded].map(rowKey)), [browser.expanded]);
  const onExpandedChange = (keys: Set<Key>) => {
    const paths = new Set([...keys].map((key) => String(key).slice(rowKey("").length)));
    // A directory listed before is listed again as it opens; one never listed is asked for as it
    // comes on screen.
    const opened = [...paths].filter((path) => !browser.expanded.has(path) && dirs.listings.has(path));
    browser.setExpanded(paths);
    dirs.refresh(opened);
  };

  const openFile = (path: string) => {
    browser.setSelected(path);
    reader.open(path);
  };

  /** Puts keyboard focus on the row `key` names in `list` once the viewer has closed. */
  const focusRow = (list: HTMLElement | null, key: string) =>
    // After the dialog's own focus restore, which puts focus back on the row it opened from.
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        const row = list?.querySelector<HTMLElement>(`[data-key="${CSS.escape(key)}"]`);
        if (row && (document.activeElement === document.body || list?.contains(document.activeElement))) row.focus();
      }),
    );

  const closeViewer = () => {
    reader.close();
    if (browser.selected !== undefined) focusRow(treeRef.current, rowKey(browser.selected));
  };

  // What the list said of the change when it was opened, and the list then: a later listing that
  // says otherwise is news, and the change is read again (`rereadChange`).
  const lastChangeEvidence = useRef<{ key: string; evidence: ChangeEvidence; list: unknown } | undefined>(undefined);
  const openChange = (item: ChangeItem) => {
    browser.setSelectedChange(item.key);
    lastChangeEvidence.current = { key: item.key, evidence: { state: "listed", versions: item.versions }, list: evidenceList };
    changeReader.open(item, browser.worktree);
  };

  const closeChange = () => {
    changeReader.close();
    if (browser.selectedChange !== undefined) focusRow(changeListRef.current, browser.selectedChange);
  };

  const changeWorktree = (worktree: string | undefined) => {
    browser.setSelectedChange(undefined);
    browser.setWorktree(worktree);
  };

  const refresh = () => {
    if (mode === "files") return dirs.refreshAll();
    source.refresh();
    changes.refresh();
  };

  // The selection is kept in view, so moving through files in the viewer moves the tree along
  // behind it, and its row is there to take focus when the viewer closes. Rows are one height, so
  // where a row is follows from its place in the list.
  const scrolledTo = useRef<string | undefined>(undefined);
  useLayoutEffect(() => {
    const selected = browser.selected;
    const element = treeRef.current;
    if (selected === undefined || selected === scrolledTo.current || !element) return;
    const index = tree.rows.indexOf(rowKey(selected));
    if (index === -1) return;
    scrolledTo.current = selected;
    const top = TREE_PADDING + index * ROW_HEIGHT;
    if (top < element.scrollTop) element.scrollTop = top;
    else if (top + ROW_HEIGHT > element.scrollTop + element.clientHeight) element.scrollTop = top + ROW_HEIGHT - element.clientHeight;
  }, [browser.selected, tree.rows]);

  // Likewise for the change list's selection, while the viewer moves through the changes.
  const scrolledToChange = useRef<string | undefined>(undefined);
  useLayoutEffect(() => {
    const selected = browser.selectedChange;
    const element = changeListRef.current;
    if (selected === undefined || selected === scrolledToChange.current || !element || changes.list.state !== "loaded") return;
    const top = changeRowOffset(changes.list.items, selected);
    if (top === undefined) return;
    scrolledToChange.current = selected;
    if (top < element.scrollTop) element.scrollTop = top;
    else if (top + CHANGE_ROW_HEIGHT > element.scrollTop + element.clientHeight) element.scrollTop = top + CHANGE_ROW_HEIGHT - element.clientHeight;
  }, [browser.selectedChange, changes.list]);

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

  const navigation = useMemo(() => {
    if (viewedPath === undefined) return undefined;
    const { previous, next } = neighbours(tree.files, viewedPath);
    return {
      onPrevious: previous === undefined ? undefined : () => openFile(previous),
      onNext: next === undefined ? undefined : () => openFile(next),
    };
  }, [tree.files, viewedPath]);

  // The change list's say about the change in the viewer, as `rereadFor` weighs the listings' say
  // about a file; a fresh list counts as news only for a change whose sources kept moving.
  const viewedChange = changeReader.item;
  // A worktree found gone — by its list, or by the source no longer listing it — has no changes
  // left: the open change counts as gone from it, and is read again to say so.
  const worktreeGone = (changes.list.state === "error" && changes.list.unavailable) || (git !== null && !listable);
  const evidenceList = useMemo(
    () => (worktreeGone ? { items: [], complete: true, refreshing: false } : changes.list.state === "loaded" ? changes.list : undefined),
    [worktreeGone, changes.list],
  );
  useEffect(() => {
    if (viewedChange === undefined || changeReader.shown.state === "loading") return;
    const evidence = changeEvidence(evidenceList, viewedChange.key);
    const last = lastChangeEvidence.current?.key === viewedChange.key ? lastChangeEvidence.current : undefined;
    if (rereadChange(changeReader.shown, evidence, last?.evidence, evidenceList !== last?.list)) changeReader.reload();
    if (evidence.state !== "unknown") lastChangeEvidence.current = { key: viewedChange.key, evidence, list: evidenceList };
  }, [evidenceList, viewedChange?.key, changeReader.shown.state]);

  const changeNavigation = useMemo(() => {
    if (viewedChange === undefined) return undefined;
    const { previous, next } = changeNeighbours(changes.list.state === "loaded" ? changes.list.items : [], viewedChange);
    return {
      onPrevious: previous === undefined ? undefined : () => openChange(previous),
      onNext: next === undefined ? undefined : () => openChange(next),
    };
  }, [changes.list, viewedChange]);

  const root = dirs.listings.get("");
  return (
    <AsidePane layout={layout} label={t("browser.label", { project: project.name })}>
      <Tabs selectedKey={mode} onSelectionChange={(key) => browser.setMode(key as BrowserMode)} className="flex min-h-0 flex-1 flex-col gap-0">
        <div ref={headerRef} className="flex h-10 shrink-0 items-center gap-2 border-b border-separator ps-3 pe-2 text-sm">
          <FolderOpen aria-hidden="true" className="size-4 shrink-0 text-muted" />
          <h2 className="flex min-w-0 flex-1 font-medium">
            <FadeOverflow as="span" dir="auto" className="min-w-0 flex-1" titleWhenClipped={project.name}>
              {project.name}
            </FadeOverflow>
          </h2>
          <Tabs.List aria-label={t("browser.modes")} className="w-auto min-w-0 shrink-0 p-0.5">
            {/* HeroUI dims a hovered tab to 70%, which takes its muted label under WCAG AA's 4.5:1;
                the hover darkens it instead. */}
            <Tabs.Tab id="files" className="h-7 px-3 whitespace-nowrap hover:text-foreground hover:opacity-100">
              {t("browser.mode.files")}
              <Tabs.Indicator />
            </Tabs.Tab>
            <Tabs.Tab id="git" className="h-7 px-3 whitespace-nowrap hover:text-foreground hover:opacity-100">
              {t("browser.mode.git")}
              <Tabs.Indicator />
            </Tabs.Tab>
          </Tabs.List>
          <TitledControl title={t("browser.refresh")}>
            <Button isIconOnly size="sm" variant="ghost" aria-label={t("browser.refresh")} preventFocusOnPress onPress={refresh}>
              <RefreshCw aria-hidden="true" className="size-4" />
            </Button>
          </TitledControl>
        </div>
        <Tabs.Panel id="files" className="mt-0 flex min-h-0 flex-1 flex-col p-0">
      {root === undefined || root.state === "loading" ? (
        <div role="status" className="flex flex-1 items-center justify-center gap-2 text-sm text-muted">
          <Spinner size="sm" aria-hidden="true" />
          {t("browser.loading")}
        </div>
      ) : root.state === "error" ? (
        <RootError
          message={root.message}
          onRetry={(from) => {
            // The error and its button go as the listing starts again; focus on it moves to the
            // header's Refresh, which stays.
            handFocusOff(from, headerRef.current, true);
            dirs.refresh([""]);
          }}
        />
      ) : tree.nodes.length === 0 && root.complete ? (
        <EmptyPanel icon={FolderX} message={t("browser.empty")} />
      ) : (
        <>
          <FileTree
            nodes={tree.nodes}
            expanded={expandedKeys}
            label={t("browser.tree.label")}
            onExpandedChange={onExpandedChange}
            onOpenFile={openFile}
            onRetry={(dir) => dirs.refresh([dir])}
            treeRef={treeRef}
          />
          {!root.complete && <p className="shrink-0 border-t border-separator px-3 py-2 text-xs text-muted">{t("browser.partial")}</p>}
        </>
      )}
        </Tabs.Panel>
        <Tabs.Panel id="git" className="mt-0 flex min-h-0 flex-1 flex-col p-0">
          <GitView
            source={source.source}
            list={changes.list}
            worktree={browser.worktree}
            onWorktreeChange={changeWorktree}
            selectedChange={browser.selectedChange}
            onOpen={openChange}
            onRetry={(from) => {
              // The failure and its button go as the request starts again; focus on it moves to
              // the header's Refresh, which stays.
              handFocusOff(from, headerRef.current, true);
              refresh();
            }}
            listRef={changeListRef}
          />
        </Tabs.Panel>
      </Tabs>
      {reader.subject && <FileViewer subject={reader.subject} onClose={closeViewer} navigation={navigation} />}
      {changeReader.subject && <FileViewer subject={changeReader.subject} onClose={closeChange} navigation={changeNavigation} />}
    </AsidePane>
  );
}

/** The project's own directory could not be listed: why, and a way to try again. */
function RootError({ message, onRetry }: { message: string; onRetry: (from: Element | null) => void }): React.ReactElement {
  const t = useT();
  const ref = useRef<HTMLDivElement>(null);
  return (
    <div ref={ref} className="flex flex-1 flex-col items-center justify-center gap-3 px-4 text-center text-sm">
      <TriangleAlert aria-hidden="true" className="size-6 text-danger" />
      <div role="alert" className="flex flex-col gap-1">
        <p className="font-medium">{t("browser.error.root")}</p>
        <p className="text-muted">{message}</p>
      </div>
      <Button size="sm" variant="secondary" preventFocusOnPress onPress={() => onRetry(ref.current)}>
        {t("error.tryAgain")}
      </Button>
    </div>
  );
}
