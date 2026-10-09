import { Button, Spinner } from "@heroui/react";
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
import { useProjectBrowserState } from "./browserState";
import { FileTree, ROW_HEIGHT } from "./FileTree";
import { buildTree, fileEvidence, neighbours, parentPath, rereadFor, rowKey, type DirListing, type FileEvidence } from "./tree";
import { useDirectoryListings } from "./useDirectoryListings";
import { useFileReader } from "./useFileReader";

/** The vertical padding the tree's scroller adds above its first row (`py-1`). */
const TREE_PADDING = 4;

/**
 * A project's browser, the aside's content while the project owns it (`asideOwner.ts`): the
 * project's files, live from its directory through the daemon, and the read-only viewer over them.
 * It needs no session: the project is all it reads from. The caller mounts one per project (keyed by
 * its id), so nothing read for one project can land in another's; what belongs to the project
 * across mounts — the expanded directories and the selected file — is in `browserState.ts`.
 *
 * The viewer moves through the tree's file rows as they are on screen, top to bottom, and stops at
 * either end. The file it shows is the tree's selection, kept in view behind it, and closing it
 * puts keyboard focus on that row. While the viewer is open, the listings decide when it reads the
 * file again (`rereadFor`).
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
  const dirs = useDirectoryListings(project.id, active);
  const reader = useFileReader(project.id);
  const treeRef = useRef<HTMLDivElement>(null);
  const headerRef = useRef<HTMLDivElement>(null);

  const tree = useMemo(() => buildTree(dirs.listings, browser.expanded, browser.selected), [dirs.listings, browser.expanded, browser.selected]);
  const wantedKey = tree.wanted.join("\0");
  useEffect(() => dirs.want(tree.wanted), [wantedKey]);
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
    const selected = browser.selected;
    if (selected === undefined) return;
    // After the dialog's own focus restore, which puts focus back on the row it opened from.
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        const treeElement = treeRef.current;
        const row = treeElement?.querySelector<HTMLElement>(`[data-key="${CSS.escape(rowKey(selected))}"]`);
        if (row && (document.activeElement === document.body || treeElement?.contains(document.activeElement))) row.focus();
      }),
    );
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
  return (
    <AsidePane layout={layout} label={t("browser.label", { project: project.name })}>
      <div ref={headerRef} className="flex h-10 shrink-0 items-center gap-2 border-b border-separator ps-3 pe-2 text-sm">
        <FolderOpen aria-hidden="true" className="size-4 shrink-0 text-muted" />
        <h2 className="flex min-w-0 flex-1 font-medium">
          <FadeOverflow as="span" dir="auto" className="min-w-0 flex-1" titleWhenClipped={project.name}>
            {project.name}
          </FadeOverflow>
        </h2>
        <TitledControl title={t("browser.refresh")}>
          <Button isIconOnly size="sm" variant="ghost" aria-label={t("browser.refresh")} preventFocusOnPress onPress={dirs.refreshAll}>
            <RefreshCw aria-hidden="true" className="size-4" />
          </Button>
        </TitledControl>
      </div>
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
      {reader.subject && <FileViewer subject={reader.subject} onClose={closeViewer} navigation={navigation} />}
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
