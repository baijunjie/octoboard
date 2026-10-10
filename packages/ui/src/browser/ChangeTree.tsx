import React, { useMemo } from "react";
import { Collection, ListLayout, Tree, TreeHeader, TreeItem, TreeItemContent, TreeSection, Virtualizer, type Key } from "react-aria-components";

import { useScrollFade } from "../components/useScrollFade";
import { useCurrentLanguage, useT } from "../i18n/react";
import { ChangeRowBody, changeRowText, HEADING_CLASS, HEADING_HEIGHT, SectionHeadingText } from "./changeRow";
import { changeGroups, type ChangeItem, type ChangeNode, type ChangeSection } from "./changes";
import { DirectoryRow, ROW_CLASS, ROW_HEIGHT, RowContent } from "./treeRow";

/** The directories of `nodes` and everything under them. */
function directoryKeys(nodes: readonly ChangeNode[]): string[] {
  return nodes.flatMap((node) => (node.type === "directory" ? [node.key, ...directoryKeys(node.children)] : []));
}

/**
 * The change list's tree form: the same sections, each headed with its count, with a section's changes grouped
 * under their directories. HeroUI 3 has no tree, so this is built on the react-aria-components `Tree` HeroUI
 * itself builds on, virtualized, as the Files mode's tree is (`FileTree`), so it has the same keyboard model:
 * arrows, Home / End, type-ahead, Left / Right to collapse and expand a directory. A directory opens or closes
 * and a change opens in the viewer. Directories start open; the ones the user folded away are `collapsed`, and
 * a change under one has no row, so the viewer's navigation skips it.
 */
export function ChangeTree({
  items,
  selected,
  label,
  onOpen,
  listRef,
  collapsed,
  onCollapsedChange,
}: {
  items: ChangeItem[];
  /** The key of the change the viewer showed last. */
  selected: string | undefined;
  label: string;
  onOpen: (item: ChangeItem) => void;
  listRef: React.Ref<HTMLDivElement>;
  /** Row keys of the directories folded away. */
  collapsed: ReadonlySet<string>;
  onCollapsedChange: (collapsed: ReadonlySet<string>) => void;
}): React.ReactElement {
  const language = useCurrentLanguage();
  const fade = useScrollFade(listRef);
  const groups = useMemo(() => changeGroups(items, "tree").map((group) => ({ id: group.section, ...group })), [items]);
  const directories = useMemo(() => groups.flatMap((group) => directoryKeys(group.nodes)), [groups]);
  const byKey = useMemo(() => {
    const map = new Map<Key, ChangeNode>();
    const walk = (nodes: readonly ChangeNode[]) =>
      nodes.forEach((node) => {
        map.set(node.key, node);
        if (node.type === "directory") walk(node.children);
      });
    groups.forEach((group) => walk(group.nodes));
    return map;
  }, [groups]);
  const expanded = useMemo(() => new Set<Key>(directories.filter((key) => !collapsed.has(key))), [directories, collapsed]);

  // Only this list's directories are decided here; the folded ones of the other view stay as they are.
  const onExpandedChange = (keys: Set<Key>) => {
    const next = new Set([...collapsed].filter((key) => !directories.includes(key)));
    for (const key of directories) if (!keys.has(key)) next.add(key);
    onCollapsedChange(next);
  };
  const onAction = (key: Key) => {
    const node = byKey.get(key);
    if (node?.type === "change") onOpen(node.item);
    else if (node?.type === "directory") onExpandedChange(expanded.has(key) ? new Set([...expanded].filter((k) => k !== key)) : new Set([...expanded, key]));
  };

  const renderNode = (node: ChangeNode): React.ReactElement => {
    if (node.type === "directory") {
      return (
        <TreeItem id={node.key} textValue={node.name} hasChildItems className={ROW_CLASS}>
          <TreeItemContent>{({ isExpanded }) => <DirectoryRow name={node.name} isExpanded={isExpanded} />}</TreeItemContent>
          <Collection items={node.children} dependencies={[language, selected]}>
            {renderNode}
          </Collection>
        </TreeItem>
      );
    }
    return <ChangeTreeRow item={node.item} isSelected={node.item.key === selected} />;
  };

  return (
    <Virtualizer layout={ListLayout} layoutOptions={{ rowHeight: ROW_HEIGHT, headingHeight: HEADING_HEIGHT }}>
      <Tree
        ref={fade.ref}
        {...fade.props}
        aria-label={label}
        items={groups}
        expandedKeys={expanded}
        onExpandedChange={onExpandedChange}
        onAction={onAction}
        dependencies={[language, selected]}
        className={`${fade.className} min-h-0 flex-1 overflow-auto py-1 outline-none`}
      >
        {(group) => (
          <TreeSection id={group.section}>
            <SectionHeading section={group.section} count={group.items.length} />
            <Collection items={group.nodes} dependencies={[language, selected]}>
              {renderNode}
            </Collection>
          </TreeSection>
        )}
      </Tree>
    </Virtualizer>
  );
}

function SectionHeading({ section, count }: { section: ChangeSection; count: number }): React.ReactElement {
  return (
    <TreeHeader className={HEADING_CLASS}>
      <SectionHeadingText section={section} count={count} />
    </TreeHeader>
  );
}

function ChangeTreeRow({ item, isSelected }: { item: ChangeItem; isSelected: boolean }): React.ReactElement {
  const t = useT();
  const { name, detail, ariaLabel } = changeRowText(t, item, isSelected, true);
  // Marked `data-current`, not `data-selected`: react-aria-components writes the row's
  // `data-selected` itself, from a selection the tree does not have.
  return (
    <TreeItem id={item.key} textValue={name} aria-label={ariaLabel} data-current={isSelected || undefined} className={ROW_CLASS}>
      <TreeItemContent>
        <RowContent>
          <span className="size-4 shrink-0" />
          <ChangeRowBody item={item} name={name} detail={detail} />
        </RowContent>
      </TreeItemContent>
    </TreeItem>
  );
}
