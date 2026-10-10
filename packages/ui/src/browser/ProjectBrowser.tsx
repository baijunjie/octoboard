import { Button, Spinner, Tabs } from "@heroui/react";
import { FolderOpen, FolderX, RefreshCw, TriangleAlert } from "lucide-react";
import React, { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { Key } from "react-aria-components";

import { EmptyPanel } from "../components/EmptyPanel";
import { handFocusOff } from "../components/handFocusOff";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { FADE_SIZE } from "../components/useScrollFade";
import { FadeOverflow } from "../components/FadeOverflow";
import { TitledControl } from "../components/TitledControl";
import { useT } from "../i18n/react";
import { AsidePane, type AsideLayout } from "../layout/AsidePane";
import type { Project } from "../protocol";
import { FileViewer } from "../viewer/FileViewer";
import { useProjectBrowserState, type BrowserMode } from "./browserState";
import { FileTree } from "./FileTree";
import { focusRow } from "./focusRow";
import { GitView } from "./GitView";
import { buildTree, fileEvidence, neighbours, parentPath, rereadFor, rowKey, type DirListing, type FileEvidence } from "./tree";
import { ROW_HEIGHT } from "./treeRow";
import { useDirectoryListings } from "./useDirectoryListings";
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
 * modes: Files, the project's files live from its directory, and Git, a worktree's uncommitted
 * changes to them or the changes between two of its repository's branches (`useGitReview`); the
 * read-only viewer opens over any of them. It needs no session: the project is all it reads from.
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
  const git = useGitReview(project.id, browser, active && mode === "git");
  const treeRef = useRef<HTMLDivElement>(null);
  const headerRef = useRef<HTMLDivElement>(null);

  const tree = useMemo(() => buildTree(dirs.listings, browser.expanded, browser.selected), [dirs.listings, browser.expanded, browser.selected]);
  const wantedKey = tree.wanted.join("\0");
  useEffect(() => {
    if (mode === "files") dirs.want(tree.wanted);
  }, [wantedKey, mode]);
  useEffect(() => browser.touch(), []);

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

  const closeViewer = () => {
    reader.close();
    if (browser.selected !== undefined) focusRow(treeRef.current, rowKey(browser.selected));
  };

  const refresh = () => (mode === "files" ? dirs.refreshAll() : git.refresh());

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
    if (selected === undefined || selected === scrolledTo.current || !element) return;
    const index = tree.rows.indexOf(rowKey(selected));
    if (index === -1) return;
    scrolledTo.current = selected;
    const top = TREE_PADDING + index * ROW_HEIGHT;
    // Clear of the fade at the tree's edges, as `scroll-padding` keeps the rows react-aria scrolls.
    if (top - FADE_SIZE < element.scrollTop) element.scrollTop = top - FADE_SIZE;
    else if (top + ROW_HEIGHT + FADE_SIZE > element.scrollTop + element.clientHeight) element.scrollTop = top + ROW_HEIGHT + FADE_SIZE - element.clientHeight;
  }, [browser.selected, tree.rows]);

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

  const root = dirs.listings.get("");
  const rootLoading = root === undefined || root.state === "loading";
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
            <Button isIconOnly size="sm" variant="ghost" aria-label={t("browser.refresh")} preventFocusOnPress onPress={pressRefresh}>
              <RefreshCw aria-hidden="true" className={spinning > 0 ? "size-4 motion-safe:animate-spin-slow" : "size-4"} />
            </Button>
          </TitledControl>
        </div>
        <Tabs.Panel id="files" className="mt-0 flex min-h-0 flex-1 flex-col p-0">
      <StatusAnnouncer text={rootLoading ? t("browser.loading") : undefined} />
      {rootLoading ? (
        <div aria-hidden="true" className="flex flex-1 items-center justify-center gap-2 text-sm text-muted">
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
            {...git.view}
            onRetry={(from) => {
              // The failure and its button go as the request starts again; focus on it moves to
              // the header's Refresh, which stays.
              handFocusOff(from, headerRef.current, true);
              refresh();
            }}
          />
        </Tabs.Panel>
      </Tabs>
      {reader.subject && <FileViewer subject={reader.subject} onClose={closeViewer} navigation={navigation} />}
      {git.viewer && <FileViewer {...git.viewer} />}
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
