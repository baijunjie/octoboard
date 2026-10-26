import { Spinner } from "@heroui/react";
import React, { useEffect, useId, useMemo, useRef } from "react";
import { Collection, ListLayout, Tree, TreeItem, TreeItemContent, Virtualizer, type Key } from "react-aria-components";

import { Button } from "../components/Button";
import { failureParam, FailureBody } from "../components/errorParts";
import { FadeOverflow } from "../components/FadeOverflow";
import { StatusMark } from "../components/StatusMark";
import { useCollectionOptionTab } from "../components/collectionOptionTab";
import { useScrollFade } from "../components/scrollFade";
import type { ErrorPresentation } from "../errorPresentation";
import { retryCanHelp } from "../errorRecovery";
import { errorText } from "../errorText";
import { isolate } from "../i18n/isolate";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { BrowseEntry } from "../protocol";
import { wireBaseName } from "../wirePath";
import { RowActions } from "./RowActions";
import { FileIcon } from "./FileIcon";
import type { TreeNode } from "./tree";
import { BrowserListMotion, useBrowserListMotion } from "./useBrowserListMotion";
import { DirectoryRow, IGNORED_ROW_CLASS, ROW_CLASS, ROW_HEIGHT, RowContent, RowName } from "./treeRow";

/** The rows of the tree with their rendering kept for every row whose content did not change: the
 * tree caches a row's rendering by its object, so handing back the same object for an unchanged row
 * keeps a refresh, an expansion or a new selection from re-rendering every row. */
export function useStableNodes(nodes: TreeNode[]): TreeNode[] {
  const previous = useMemo(() => new Map<string, TreeNode>(), []);
  return useMemo(() => {
    const seen = new Map<string, TreeNode>();
    const stable = (list: TreeNode[]): TreeNode[] =>
      list.map((node) => {
        const next = node.type === "directory" ? { ...node, children: stable(node.children) } : node;
        const before = previous.get(node.key);
        const result = before !== undefined && sameNode(before, next) ? before : next;
        seen.set(node.key, result);
        return result;
      });
    const result = stable(nodes);
    // A cache kept across renders, which the React Compiler's rules forbid: this hook stays
    // uncompiled, as the sharing it exists for needs.
    // eslint-disable-next-line react-hooks/immutability
    previous.clear();
    // eslint-disable-next-line react-hooks/immutability
    for (const [key, node] of seen) previous.set(key, node);
    return result;
  }, [nodes]);
}

function sameEntry(a: BrowseEntry | undefined, b: BrowseEntry | undefined): boolean {
  return a === b || (a !== undefined && b !== undefined && a.kind === b.kind && a.size === b.size && a.target === b.target && a.ignored === b.ignored);
}

function sameMessage(before: ErrorPresentation | undefined, next: ErrorPresentation | undefined): boolean {
  return before === undefined || next === undefined ? before === next : errorText(before) === errorText(next);
}

function sameNode(before: TreeNode, next: TreeNode): boolean {
  if (before.type === "directory" && next.type === "directory") {
    const sameChildren = before.children.length === next.children.length && before.children.every((child, i) => child === next.children[i]);
    return sameEntry(before.entry, next.entry) && sameChildren && sameMessage(before.failure, next.failure) && before.partial === next.partial;
  }
  if (before.type === "file" && next.type === "file") {
    return sameEntry(before.entry, next.entry) && before.missing === next.missing && before.selected === next.selected;
  }
  if (before.type === "status" && next.type === "status") return before.status === next.status;
  return false;
}

/**
 * A project's files as a tree: react-aria-components' `Tree`, virtualized. HeroUI 3 has no tree, so
 * this is built on the react-aria component HeroUI itself builds on, which brings the treegrid
 * keyboard model (arrows, Home / End, type-ahead, Left / Right to collapse and expand). A row's
 * action is the one thing it does: a directory opens or closes, a file opens in the viewer; each file
 * or directory row carries at its end the button of the others (`RowActions`), the same menu a change's
 * row carries in the Git mode. Nothing is selectable in react-aria's sense; the file the viewer last
 * showed is tinted and named as selected in its row's label instead.
 *
 * A directory whose listing failed is drawn collapsed, with its mark on the row, and expanding it
 * (press, Right, the arrow) asks for the listing again, as Try again does. It stays in `expanded`, so
 * once it lists it is open again: the collapse is the failure's, not the user's choice.
 *
 * A press focuses the row, as a click into a report page focuses the page: the tree is content to
 * read and move through from the keyboard, not a control beside the terminal.
 */
export function FileTree({
  nodes,
  expanded,
  label,
  onExpandedChange,
  onOpenFile,
  onRetry,
  treeRef,
}: {
  nodes: TreeNode[];
  /** Row keys of the expanded directories. */
  expanded: ReadonlySet<string>;
  label: string;
  onExpandedChange: (keys: Set<Key>) => void;
  onOpenFile: (path: string) => void;
  /** Try again was pressed in the mark of a directory that could not be listed: list it again. */
  onRetry: (dir: string) => void;
  treeRef: React.Ref<HTMLDivElement>;
}): React.ReactElement {
  const language = useCurrentLanguage();
  const treeId = useId();
  const items = useStableNodes(nodes);
  const { visibleKeys, folds } = useMemo(() => {
    const keys = new Set<string>();
    const parents = new Map<string, string>();
    // A directory opens once its rows are listed: rows that arrive while it is still loading, or
    // after it failed to list, unfold with it rather than entering one by one.
    const open = new Set<string>();
    const walk = (rows: TreeNode[], parent?: string) => {
      for (const node of rows) {
        keys.add(node.key);
        if (parent !== undefined) parents.set(node.key, parent);
        if (node.type === "directory" && expanded.has(node.key) && node.failure === undefined) {
          if (node.children.some((child) => child.type !== "status" || child.status !== "loading")) open.add(node.key);
          if (node.children.length > 0) walk(node.children, node.key);
          else {
            keys.add(`collapsed:${node.path}`);
            parents.set(`collapsed:${node.path}`, node.key);
          }
        }
      }
    };
    walk(items);
    return { visibleKeys: keys, folds: { parents, open } };
  }, [items, expanded]);
  const motion = useBrowserListMotion(treeRef, visibleKeys, folds);
  const collectionRef = useCollectionOptionTab(motion.ref);
  const fade = useScrollFade({ ref: collectionRef, className: "min-h-0 flex-1 overflow-auto py-1 outline-none" });
  // react-aria-components 1.21.1 decides whether a row can expand — its `aria-expanded` and Left /
  // Right — from the children it holds, not from `hasChildItems`, which it hands only to the
  // chevron slot. So a collapsed directory, whose rows are not built, holds one
  // stand-in row that is never on screen (a collapsed row's children are not rows of the tree), the
  // same object every time. It costs the tree's `aria-rowcount` one row per collapsed directory.
  // TODO: drop the stand-ins once react-aria-components hands `hasChildItems` to `useTreeItem`.
  const standIns = useMemo(() => new Map<string, TreeNode[]>(), []);
  const childrenOf = (node: Extract<TreeNode, { type: "directory" }>): TreeNode[] => {
    if (node.children.length > 0) return node.children;
    let standIn = standIns.get(node.path);
    if (!standIn) {
      standIn = [{ type: "status", key: `collapsed:${node.path}`, dir: node.path, status: "loading" }];
      // eslint-disable-next-line react-hooks/immutability -- kept across renders on purpose, which leaves the tree uncompiled
      standIns.set(node.path, standIn);
    }
    return standIn;
  };
  const byKey = useMemo(() => {
    const map = new Map<Key, TreeNode>();
    const walk = (list: TreeNode[]) =>
      list.forEach((node) => {
        map.set(node.key, node);
        if (node.type === "directory") walk(node.children);
      });
    walk(items);
    return map;
  }, [items]);

  // The directories whose listing failed, drawn collapsed whatever `expanded` says.
  const failed = useMemo(() => new Set([...byKey].filter(([, node]) => node.type === "directory" && node.failure !== undefined).map(([key]) => key)), [byKey]);
  const shownExpanded = useMemo(() => new Set([...expanded].filter((key) => !failed.has(key))), [expanded, failed]);
  // Opening a failed directory asks for its listing again; nothing else about `expanded` changes.
  const retryOpened = (keys: Iterable<Key>) => {
    for (const key of keys) {
      const node = byKey.get(key);
      if (node?.type === "directory" && retryCanHelp(node.failure)) onRetry(node.path);
    }
  };
  const changeExpanded = (keys: Set<Key>) => {
    retryOpened([...keys].filter((key) => failed.has(key)));
    // The failed ones were left out of what the tree was given and stay expanded in `expanded`.
    onExpandedChange(new Set([...keys, ...[...failed].filter((key) => expanded.has(String(key)))]));
  };

  // ArrowRight (Left, right to left) on a failed folder's row walks to its mark. React-aria's row
  // handler would take the key to expand the collapsed row, and it is reached through React's root
  // listener, so the key is taken first, on the document in the capture phase.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const row = event.target instanceof Element ? event.target.closest<HTMLElement>("[role=row][data-listing-failed]") : null;
      // Only a row of this tree, not of another one on the page.
      if (!row || event.target !== row || row.closest("[data-file-tree]")?.getAttribute("data-file-tree") !== treeId) return;
      const toward = getComputedStyle(row).direction === "rtl" ? "ArrowLeft" : "ArrowRight";
      if (event.key !== toward || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
      const mark = row.querySelector<HTMLElement>("[data-status-mark]");
      if (!mark) return;
      event.preventDefault();
      event.stopPropagation();
      mark.focus();
    };
    document.addEventListener("keydown", onKeyDown, true);
    return () => document.removeEventListener("keydown", onKeyDown, true);
  }, []);

  const onAction = (key: Key) => {
    const node = byKey.get(key);
    if (!node) return;
    if (node.type === "file") onOpenFile(node.path);
    else if (node.type === "directory" && failed.has(key)) retryOpened([key]);
    else if (node.type === "directory") {
      const next = new Set<Key>(expanded);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      onExpandedChange(next);
    }
  };

  const renderNode = (node: TreeNode): React.ReactElement => {
    if (node.type === "status") return <StatusItem node={node} />;
    return (
      <FileNodeRow node={node} onRetry={onRetry}>
        {node.type === "directory" && <Collection items={childrenOf(node)} dependencies={[language]}>{renderNode}</Collection>}
      </FileNodeRow>
    );
  };

  return (
    <BrowserListMotion keys={visibleKeys} folds={folds} prepare={motion.prepare}>
      <Virtualizer layout={ListLayout} layoutOptions={{ rowHeight: ROW_HEIGHT }}>
        <Tree
          {...fade}
          aria-label={label}
          data-file-tree={treeId}
          items={items}
          expandedKeys={shownExpanded}
          onExpandedChange={changeExpanded}
          onAction={onAction}
          dependencies={[language]}
        >
          {renderNode}
        </Tree>
      </Virtualizer>
    </BrowserListMotion>
  );
}

function FileNodeRow({ node, onRetry, children }: {
  node: Exclude<TreeNode, { type: "status" }>;
  onRetry: (dir: string) => void;
  children?: React.ReactNode;
}): React.ReactElement {
  const t = useT();
  const rowRef = useRef<HTMLDivElement>(null);
  const name = wireBaseName(node.path);
  // A failure arriving takes the rows beneath the folder with it, Try again takes the mark away, and so does a
  // listing that is no longer cut short: focus that fell to the page with any of them goes to the folder's row instead.
  const listing = node.type !== "directory" ? undefined : node.failure !== undefined ? { kind: "failed" as const, message: node.failure } : node.partial ? { kind: "partial" as const } : undefined;
  const markKind = listing?.kind;
  const hadMark = useRef(markKind);
  useEffect(() => {
    if (hadMark.current === markKind) return;
    hadMark.current = markKind;
    const row = rowRef.current;
    requestAnimationFrame(() => requestAnimationFrame(() => {
      const active = document.activeElement;
      // Focus that fell to the page, or to the tree itself, which react-aria falls back to.
      const lost = !active || active === document.body || active.getAttribute("role") === "treegrid";
      if (lost && row?.isConnected) row.focus();
    }));
  }, [markKind]);
  if (node.type === "directory") {
    const link = node.entry.kind === "symlink";
    const ignored = node.entry.ignored === true;
    const mark = listing?.kind === "failed"
      ? <ListingFailureMark message={listing.message} onRetry={() => onRetry(node.path)} />
      : listing?.kind === "partial"
        ? <StatusMark tone="warning" label={t("browser.partial")} />
        : undefined;
    return (
      <TreeItem data-list-motion={node.key} id={node.key} textValue={name} aria-label={ignored ? t("browser.tree.row.ignored", { name: isolate(name) }) : undefined} hasChildItems data-ignored={ignored || undefined} data-listing-failed={node.failure !== undefined || undefined} className={`${ROW_CLASS} ${IGNORED_ROW_CLASS}`} ref={rowRef}>
        <TreeItemContent>
          {({ isExpanded }) => (
            <DirectoryRow name={name} isExpanded={isExpanded} link={link} nameGrows={!mark}>
              {mark && (
                <>
                  {mark}
                  <span className="min-w-0 flex-1" />
                </>
              )}
              <RowActions contextTargetRef={rowRef} path={node.path} name={name} directory />
            </DirectoryRow>
          )}
        </TreeItemContent>
        {children}
      </TreeItem>
    );
  }
  const isSelected = node.selected;
  const missing = node.missing === "removed" ? t("browser.tree.removed") : node.missing === "unlisted" ? t("browser.tree.unlisted") : undefined;
  // An ignored file is dimmed, which nothing but the name tells assistive technology.
  const ignored = node.entry?.ignored === true;
  const ariaLabel =
    node.missing === "removed"
      ? t("browser.tree.row.removed", { name: isolate(name) })
      : node.missing === "unlisted"
        ? t("browser.tree.row.unlisted", { name: isolate(name) })
        : ignored
          ? t(isSelected ? "browser.tree.row.selectedIgnored" : "browser.tree.row.ignored", { name: isolate(name) })
          : isSelected
            ? t("browser.tree.row.selected", { name: isolate(name) })
            : name;
  // Marked `data-current`, not `data-selected`: react-aria-components writes the row's
  // `data-selected` itself, from a selection the tree does not have.
  return (
    <TreeItem data-list-motion={node.key} id={node.key} textValue={name} aria-label={ariaLabel} data-current={isSelected || undefined} data-ignored={ignored || undefined} className={`${ROW_CLASS} ${IGNORED_ROW_CLASS}`} ref={rowRef}>
      <TreeItemContent>
        <RowContent>
          <FileIcon entry={node.entry} path={node.path} />
          <RowName name={name} className={missing ? "text-app-muted line-through" : undefined} />
          {missing && <span className="shrink-0 text-xs text-app-muted">{missing}</span>}
          <RowActions contextTargetRef={rowRef} path={node.path} name={name} />
        </RowContent>
      </TreeItemContent>
    </TreeItem>
  );
}

/** The mark the row of a directory whose listing failed carries (drawn collapsed), after its name: the
 * glyph names the failure and its popover holds the reason and the raw text, and Try again where asking
 * again can help (a failure that is the state of the folder, outside the project, has none). Nothing
 * is listed beneath the row. */
function ListingFailureMark({ message, onRetry }: { message: ErrorPresentation; onRetry: () => void }): React.ReactElement {
  const t = useT();
  const label = t("browser.tree.errorLabel");
  return (
    <StatusMark
      label={label}
      actions={retryCanHelp(message) ? (close) => (
        <Button
          size="sm"
          variant="secondary"
          preventFocusOnPress
          onPress={() => {
            close();
            onRetry();
          }}
        >
          {t("error.tryAgain")}
        </Button>
      ) : undefined}
    >
      <FailureBody error={message} summary={<Message id="browser.tree.error" params={{ message: failureParam(message) }} />} />
    </StatusMark>
  );
}

/** A row that says what a directory's listing is: still loading or empty. */
function StatusItem({ node }: { node: Extract<TreeNode, { type: "status" }> }): React.ReactElement {
  const t = useT();
  const text = node.status === "loading" ? t("browser.tree.loading") : t("browser.tree.empty");
  return (
    <TreeItem data-list-motion={node.key} id={node.key} textValue={text} className={ROW_CLASS}>
      <TreeItemContent>
        <RowContent>
          {node.status === "loading" ? <Spinner size="sm" aria-hidden="true" className="size-4 shrink-0" /> : <span className="size-4 shrink-0" />}
          <FadeOverflow as="span" className="min-w-0 flex-1 text-app-muted italic" titleWhenClipped={text}>
            {text}
          </FadeOverflow>
        </RowContent>
      </TreeItemContent>
    </TreeItem>
  );
}
