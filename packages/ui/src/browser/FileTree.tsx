import { Spinner } from "@heroui/react";
import {
  RotateCw,
  TriangleAlert,
} from "lucide-react";
import React, { useMemo, useRef } from "react";
import { Collection, ListLayout, Tree, TreeItem, TreeItemContent, Virtualizer, type Key } from "react-aria-components";

import { CodeToneContext } from "../components/codeTone";
import { failureParam } from "../components/DaemonErrorMessage";
import { FadeOverflow } from "../components/FadeOverflow";
import { useScrollFade } from "../components/scrollFade";
import { useErrorAcknowledgement } from "../components/useErrorAcknowledgement";
import type { ErrorPresentation } from "../errorPresentation";
import { errorText } from "../errorText";
import { isolate } from "../i18n/isolate";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { BrowseEntry } from "../protocol";
import { wireBaseName } from "../wirePath";
import { RowActions } from "./RowActions";
import { FileIcon } from "./FileIcon";
import type { TreeNode } from "./tree";
import { BrowserListMotion, useBrowserListMotion } from "./useBrowserListMotion";
import { DirectoryRow, ROW_CLASS, ROW_HEIGHT, RowContent, RowName } from "./treeRow";

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
  return a === b || (a !== undefined && b !== undefined && a.kind === b.kind && a.size === b.size && a.target === b.target);
}

function sameMessage(before: ErrorPresentation | undefined, next: ErrorPresentation | undefined): boolean {
  return before === undefined || next === undefined ? before === next : errorText(before) === errorText(next);
}

function sameNode(before: TreeNode, next: TreeNode): boolean {
  if (before.type === "directory" && next.type === "directory") {
    const sameChildren = before.children.length === next.children.length && before.children.every((child, i) => child === next.children[i]);
    return sameEntry(before.entry, next.entry) && sameChildren;
  }
  if (before.type === "file" && next.type === "file") {
    return sameEntry(before.entry, next.entry) && before.missing === next.missing && before.selected === next.selected;
  }
  if (before.type === "status" && next.type === "status") return before.status === next.status && sameMessage(before.message, next.message);
  return false;
}

/**
 * A project's files as a tree: react-aria-components' `Tree`, virtualized. HeroUI 3 has no tree, so
 * this is built on the react-aria component HeroUI itself builds on, which brings the treegrid
 * keyboard model (arrows, Home / End, type-ahead, Left / Right to collapse and expand). A row's
 * action is the one thing it does: a directory opens or closes, a file opens in the viewer, an error
 * row tries again; each file or directory row carries at its end the button of the others (`RowActions`), the same
 * menu a change's row carries in the Git mode. Nothing is selectable in react-aria's sense; the file the viewer last showed is
 * tinted and named as selected in its row's label instead.
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
  /** An unreadable directory's row was pressed: list it again. */
  onRetry: (dir: string) => void;
  treeRef: React.Ref<HTMLDivElement>;
}): React.ReactElement {
  const language = useCurrentLanguage();
  const items = useStableNodes(nodes);
  const visibleKeys = useMemo(() => {
    const keys = new Set<string>();
    const walk = (rows: TreeNode[]) => {
      for (const node of rows) {
        keys.add(node.key);
        if (node.type === "directory" && expanded.has(node.key)) {
          if (node.children.length > 0) walk(node.children);
          else keys.add(`collapsed:${node.path}`);
        }
      }
    };
    walk(items);
    return keys;
  }, [items, expanded]);
  const motion = useBrowserListMotion(treeRef, visibleKeys);
  const fade = useScrollFade({ ref: motion.ref, className: "min-h-0 flex-1 overflow-auto py-1 outline-none" });
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

  const onAction = (key: Key) => {
    const node = byKey.get(key);
    if (!node) return;
    if (node.type === "file") onOpenFile(node.path);
    else if (node.type === "status" && node.status === "error") onRetry(node.dir);
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
      <FileNodeRow node={node}>
        {node.type === "directory" && <Collection items={childrenOf(node)} dependencies={[language]}>{renderNode}</Collection>}
      </FileNodeRow>
    );
  };

  return (
    <BrowserListMotion keys={visibleKeys} prepare={motion.prepare}>
      <Virtualizer layout={ListLayout} layoutOptions={{ rowHeight: ROW_HEIGHT }}>
        <Tree
          {...fade}
          aria-label={label}
          items={items}
          expandedKeys={expanded}
          onExpandedChange={onExpandedChange}
          onAction={onAction}
          dependencies={[language]}
        >
          {renderNode}
        </Tree>
      </Virtualizer>
    </BrowserListMotion>
  );
}

function FileNodeRow({ node, children }: {
  node: Exclude<TreeNode, { type: "status" }>;
  children?: React.ReactNode;
}): React.ReactElement {
  const t = useT();
  const rowRef = useRef<HTMLDivElement>(null);
  const name = wireBaseName(node.path);
  if (node.type === "directory") {
    const link = node.entry.kind === "symlink";
    return (
      <TreeItem data-list-motion={node.key} id={node.key} textValue={name} hasChildItems className={ROW_CLASS} ref={rowRef}>
        <TreeItemContent>
          {({ isExpanded }) => (
            <DirectoryRow name={name} isExpanded={isExpanded} link={link}>
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
  const ariaLabel =
    node.missing === "removed"
      ? t("browser.tree.row.removed", { name: isolate(name) })
      : node.missing === "unlisted"
        ? t("browser.tree.row.unlisted", { name: isolate(name) })
        : isSelected
          ? t("browser.tree.row.selected", { name: isolate(name) })
          : name;
  // Marked `data-current`, not `data-selected`: react-aria-components writes the row's
  // `data-selected` itself, from a selection the tree does not have.
  return (
    <TreeItem data-list-motion={node.key} id={node.key} textValue={name} aria-label={ariaLabel} data-current={isSelected || undefined} className={ROW_CLASS} ref={rowRef}>
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

/** A row that says what a directory's listing is: still loading, empty, unreadable (pressing it
 * tries again) or cut short. */
function StatusItem({ node }: { node: Extract<TreeNode, { type: "status" }> }): React.ReactElement {
  const t = useT();
  useErrorAcknowledgement(node.status === "error" ? node.message : undefined);
  const message = node.message === undefined ? "" : errorText(node.message);
  const text =
    node.status === "loading"
      ? t("browser.tree.loading")
      : node.status === "empty"
        ? t("browser.tree.empty")
        : node.status === "partial"
          ? t("browser.tree.partial")
          : t("browser.tree.error", { message: isolate(message) });
  return (
    // Named by the plain message, not by `text`: `textValue` is what react-aria collates for
    // type-to-select, which an isolate mark in the middle of would only get in the way of.
    <TreeItem data-list-motion={node.key} id={node.key} textValue={node.status === "error" ? message : text} className={ROW_CLASS}>
      <TreeItemContent>
        <RowContent>
          {node.status === "loading" ? (
            <Spinner size="sm" aria-hidden="true" className="size-4 shrink-0" />
          ) : node.status === "error" ? (
            <TriangleAlert aria-hidden="true" className="size-4 shrink-0 text-app-danger" />
          ) : (
            <span className="size-4 shrink-0" />
          )}
          <FadeOverflow as="span" className={`min-w-0 flex-1 ${node.status === "error" ? "text-app-danger" : "text-app-muted"} italic`} titleWhenClipped={text}>
            {node.status === "error" && node.message !== undefined
              ? <CodeToneContext.Provider value="danger"><Message id="browser.tree.error" params={{ message: failureParam(node.message) }} /></CodeToneContext.Provider>
              : text}
          </FadeOverflow>
          {node.status === "error" && <RotateCw aria-hidden="true" className="size-3.5 shrink-0 text-app-muted" />}
        </RowContent>
      </TreeItemContent>
    </TreeItem>
  );
}
