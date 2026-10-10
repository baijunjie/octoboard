import { Spinner } from "@heroui/react";
import {
  ChevronDown,
  ChevronRight,
  File,
  FileImage,
  FileQuestion,
  FileSymlink,
  Folder,
  FolderOpen,
  FolderSymlink,
  RotateCw,
  TriangleAlert,
  type LucideIcon,
} from "lucide-react";
import React, { useMemo } from "react";
import { Button, Collection, ListLayout, Tree, TreeItem, TreeItemContent, Virtualizer, type Key } from "react-aria-components";

import { FadeOverflow } from "../components/FadeOverflow";
import { TitledControl } from "../components/TitledControl";
import { useScrollFade } from "../components/useScrollFade";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { BrowseEntry } from "../protocol";
import { wireBaseName } from "../wirePath";
import type { TreeNode } from "./tree";

/** Every row is one line of this height, which is what lets the tree be virtualized and a row be
 * scrolled into view from its position alone. */
export const ROW_HEIGHT = 28;

const IMAGE_NAME = /\.(png|jpe?g|gif|webp|bmp|ico|avif|svg)$/i;

function fileIcon(entry: BrowseEntry | undefined, name: string): LucideIcon {
  if (!entry) return FileQuestion;
  if (entry.kind === "symlink") return FileSymlink;
  if (entry.kind === "other") return FileQuestion;
  return IMAGE_NAME.test(name) ? FileImage : File;
}

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
    previous.clear();
    for (const [key, node] of seen) previous.set(key, node);
    return result;
  }, [nodes]);
}

function sameEntry(a: BrowseEntry | undefined, b: BrowseEntry | undefined): boolean {
  return a === b || (a !== undefined && b !== undefined && a.kind === b.kind && a.size === b.size && a.target === b.target);
}

function sameNode(before: TreeNode, next: TreeNode): boolean {
  if (before.type === "directory" && next.type === "directory") {
    const sameChildren = before.children.length === next.children.length && before.children.every((child, i) => child === next.children[i]);
    return sameEntry(before.entry, next.entry) && sameChildren;
  }
  if (before.type === "file" && next.type === "file") {
    return sameEntry(before.entry, next.entry) && before.missing === next.missing && before.selected === next.selected;
  }
  if (before.type === "status" && next.type === "status") return before.status === next.status && before.message === next.message;
  return false;
}

/**
 * A project's files as a tree: react-aria-components' `Tree`, virtualized. HeroUI 3 has no tree, so
 * this is built on the react-aria component HeroUI itself builds on, which brings the treegrid
 * keyboard model (arrows, Home / End, type-ahead, Left / Right to collapse and expand). A row's
 * action is the one thing it does: a directory opens or closes, a file opens in the viewer, an error
 * row tries again. Nothing is selectable in react-aria's sense; the file the viewer last showed is
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
  const t = useT();
  const language = useCurrentLanguage();
  const items = useStableNodes(nodes);
  const fade = useScrollFade(treeRef);
  // react-aria-components 1.21.1 decides whether a row can expand — its `aria-expanded` and Left /
  // Right — from the children it holds, not from `hasChildItems`, which it hands only to the
  // chevron. So a collapsed directory, whose rows are not built, holds one stand-in row that is never
  // on screen (a collapsed row's children are not rows of the tree), the same object every time. It
  // costs the tree's `aria-rowcount` one row per collapsed directory.
  // TODO: drop the stand-ins once react-aria-components hands `hasChildItems` to `useTreeItem`.
  const standIns = useMemo(() => new Map<string, TreeNode[]>(), []);
  const childrenOf = (node: Extract<TreeNode, { type: "directory" }>): TreeNode[] => {
    if (node.children.length > 0) return node.children;
    let standIn = standIns.get(node.path);
    if (!standIn) {
      standIn = [{ type: "status", key: `collapsed:${node.path}`, dir: node.path, status: "loading" }];
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

  const chevronTitle = (isExpanded: boolean): string => (isExpanded ? t("browser.tree.collapse") : t("browser.tree.expand"));

  const renderNode = (node: TreeNode): React.ReactElement => {
    if (node.type === "status") return <StatusItem node={node} />;
    const name = wireBaseName(node.path);
    if (node.type === "directory") {
      const link = node.entry.kind === "symlink";
      return (
        <TreeItem id={node.key} textValue={name} hasChildItems className={ROW_CLASS}>
          <TreeItemContent>
            {({ isExpanded }) => {
              const Icon = link ? FolderSymlink : isExpanded ? FolderOpen : Folder;
              return (
                <RowContent>
                  {/* React Aria keeps the chevron out of the tab order; the row does the same on a press,
                      so the chevron is for the pointer alone. React Aria's own label is a translation of
                      "Expand" that disagrees with the tooltip's wording (zh-CN "扩展"), so the label is the
                      tooltip's word from the catalog; React Aria appends the row's name through
                      aria-labelledby, which gives "Expand src". */}
                  <TitledControl title={chevronTitle(isExpanded)}>
                    <Button
                      slot="chevron"
                      aria-label={chevronTitle(isExpanded)}
                      className="flex size-4 shrink-0 items-center justify-center text-muted outline-none"
                    >
                      {isExpanded ? (
                        <ChevronDown aria-hidden="true" className="size-3.5" />
                      ) : (
                        <ChevronRight aria-hidden="true" className="size-3.5 rtl:-scale-x-100" />
                      )}
                    </Button>
                  </TitledControl>
                  <Icon aria-hidden="true" className="size-4 shrink-0 text-muted" />
                  <RowName name={name} />
                </RowContent>
              );
            }}
          </TreeItemContent>
          <Collection items={childrenOf(node)} dependencies={[language]}>
            {renderNode}
          </Collection>
        </TreeItem>
      );
    }
    const isSelected = node.selected;
    const Icon = fileIcon(node.entry, name);
    const missing = node.missing === "removed" ? t("browser.tree.removed") : node.missing === "unlisted" ? t("browser.tree.unlisted") : undefined;
    const ariaLabel =
      node.missing === "removed"
        ? t("browser.tree.row.removed", { name })
        : node.missing === "unlisted"
          ? t("browser.tree.row.unlisted", { name })
          : isSelected
            ? t("browser.tree.row.selected", { name })
            : name;
    // Marked `data-current`, not `data-selected`: react-aria-components writes the row's
    // `data-selected` itself, from a selection the tree does not have.
    return (
      <TreeItem id={node.key} textValue={name} aria-label={ariaLabel} data-current={isSelected || undefined} className={ROW_CLASS}>
        <TreeItemContent>
          <RowContent>
            <span className="size-4 shrink-0" />
            <Icon aria-hidden="true" className="size-4 shrink-0 text-muted" />
            <RowName name={name} className={missing ? "text-muted line-through" : undefined} />
            {missing && <span className="shrink-0 text-xs text-muted">{missing}</span>}
          </RowContent>
        </TreeItemContent>
      </TreeItem>
    );
  };

  return (
    <Virtualizer layout={ListLayout} layoutOptions={{ rowHeight: ROW_HEIGHT }}>
      <Tree
        ref={fade.ref}
        {...fade.props}
        aria-label={label}
        items={items}
        expandedKeys={expanded}
        onExpandedChange={onExpandedChange}
        onAction={onAction}
        dependencies={[language]}
        className={`${fade.className} min-h-0 flex-1 overflow-auto py-1 outline-none`}
      >
        {renderNode}
      </Tree>
    </Virtualizer>
  );
}

/** A row: indented by its level, with a visible ring under keyboard focus. */
const ROW_CLASS = [
  "group cursor-default rounded-md text-sm outline-none select-none hover:bg-panel-hover data-[current]:bg-panel-selected",
  "data-[focus-visible]:ring-2 data-[focus-visible]:ring-focus data-[focus-visible]:ring-inset",
].join(" ");

function RowContent({ children }: { children: React.ReactNode }): React.ReactElement {
  return (
    <div className="flex h-7 min-w-0 items-center gap-1.5 pe-2 ps-[calc((var(--tree-item-level)-1)*0.875rem+0.5rem)]">{children}</div>
  );
}

function RowName({ name, className = "" }: { name: string; className?: string }): React.ReactElement {
  return (
    <FadeOverflow as="span" dir="auto" className={`min-w-0 flex-1 ${className}`} titleWhenClipped={name}>
      {name}
    </FadeOverflow>
  );
}

/** A row that says what a directory's listing is: still loading, empty, unreadable (pressing it
 * tries again) or cut short. */
function StatusItem({ node }: { node: Extract<TreeNode, { type: "status" }> }): React.ReactElement {
  const t = useT();
  const text =
    node.status === "loading"
      ? t("browser.tree.loading")
      : node.status === "empty"
        ? t("browser.tree.empty")
        : node.status === "partial"
          ? t("browser.tree.partial")
          : t("browser.tree.error", { message: node.message ?? "" });
  return (
    <TreeItem id={node.key} textValue={text} className={ROW_CLASS}>
      <TreeItemContent>
        <RowContent>
          <span className="size-4 shrink-0" />
          {node.status === "loading" ? (
            <Spinner size="sm" aria-hidden="true" className="size-4 shrink-0" />
          ) : node.status === "error" ? (
            <TriangleAlert aria-hidden="true" className="size-4 shrink-0 text-danger" />
          ) : (
            <span className="size-4 shrink-0" />
          )}
          <FadeOverflow as="span" className={`min-w-0 flex-1 ${node.status === "error" ? "text-danger" : "text-muted"} italic`} titleWhenClipped={text}>
            {text}
          </FadeOverflow>
          {node.status === "error" && <RotateCw aria-hidden="true" className="size-3.5 shrink-0 text-muted" />}
        </RowContent>
      </TreeItemContent>
    </TreeItem>
  );
}
