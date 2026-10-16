import React, { useMemo, useRef } from "react";
import { Collection, Tree, TreeHeader, TreeItem, TreeItemContent, TreeSection, Virtualizer, type Key } from "react-aria-components";

import { useStickyScrollFade } from "../components/scrollFade";
import { useCurrentLanguage, useT } from "../i18n/react";
import { ChangeRowBody, changeRowText } from "./changeRow";
import { changeGroups, sectionKey, type ChangeItem, type ChangeNode } from "./changes";
import { ChangeSectionHeading, ChangeSectionHeadingStops, ChangeListLayout, CHANGE_SCROLL_CLASS, HEADING_CLASS, HEADING_HEIGHT } from "./changeSectionHeading";
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
 *
 * `collapsed` holds the sections the user folded away too (`sectionKey`), which each section's heading does
 * (`ChangeSectionHeading`, shared with the flat list). A folded section renders none of its rows, so a directory
 * inside one keeps the state it had, this tree deciding only the directories it shows.
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
  /** Row keys of the sections and the directories folded away. */
  collapsed: ReadonlySet<string>;
  onCollapsedChange: (collapsed: ReadonlySet<string>) => void;
}): React.ReactElement {
  const language = useCurrentLanguage();
  const { props: fade, wrap } = useStickyScrollFade({ ref: listRef, className: "min-h-0 flex-1 overflow-auto pb-1 outline-none " + CHANGE_SCROLL_CLASS, stickyHeadingHeight: HEADING_HEIGHT });
  const groups = useMemo(() => changeGroups(items, "tree").map((group) => ({ id: group.section, ...group })), [items]);
  // Only the directories of the sections on screen: a folded section renders none of its rows, so
  // whether a directory inside one is folded is not this list's to decide. react-aria hands an
  // expanded key it knows nothing of back unchanged, so those keys would survive either way;
  // leaving them out of what is decided here is what makes that beside the point.
  const shown = useMemo(() => groups.filter((group) => !collapsed.has(sectionKey(group.section))), [groups, collapsed]);
  const directories = useMemo(() => shown.flatMap((group) => directoryKeys(group.nodes)), [shown]);
  const byKey = useMemo(() => {
    const map = new Map<Key, ChangeNode>();
    const walk = (nodes: readonly ChangeNode[]) =>
      nodes.forEach((node) => {
        map.set(node.key, node);
        if (node.type === "directory") walk(node.children);
      });
    shown.forEach((group) => walk(group.nodes));
    return map;
  }, [shown]);
  const expanded = useMemo(() => new Set<Key>(directories.filter((key) => !collapsed.has(key))), [directories, collapsed]);

  // Only the directories this list shows are decided here: the sections' own keys, the directories
  // inside a folded section and the other view's folded rows all stay as they are.
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
    <ChangeSectionHeadingStops>
      {wrap(<Virtualizer layout={ChangeListLayout} layoutOptions={{ rowHeight: ROW_HEIGHT, headingHeight: HEADING_HEIGHT }}>
        <Tree
          {...fade}
          aria-label={label}
          items={groups}
          expandedKeys={expanded}
          onExpandedChange={onExpandedChange}
          onAction={onAction}
          // `collapsed` among the dependencies, or the collection keeps the rows of a section that
          // has just been folded: nothing else about it changed.
          dependencies={[language, selected, collapsed]}
        >
          {(group) => (
            <TreeSection className="h-full" id={group.section}>
              <TreeHeader className={HEADING_CLASS}>
                <ChangeSectionHeading section={group.section} count={group.items.length} collapsed={collapsed} onCollapsedChange={onCollapsedChange} />
              </TreeHeader>
              {/* A folded section is its heading and nothing else, still counting what it holds. */}
              <Collection items={collapsed.has(sectionKey(group.section)) ? [] : group.nodes} dependencies={[language, selected]}>
                {renderNode}
              </Collection>
            </TreeSection>
          )}
        </Tree>
      </Virtualizer>)}
    </ChangeSectionHeadingStops>
  );
}

function ChangeTreeRow({ item, isSelected }: { item: ChangeItem; isSelected: boolean }): React.ReactElement {
  const t = useT();
  const rowRef = useRef<HTMLDivElement>(null);
  const { name, detail, ariaLabel } = changeRowText(t, item, isSelected, true);
  // Marked `data-current`, not `data-selected`: react-aria-components writes the row's
  // `data-selected` itself, from a selection the tree does not have.
  return (
    <TreeItem id={item.key} textValue={name} aria-label={ariaLabel} data-current={isSelected || undefined} className={ROW_CLASS} ref={rowRef}>
      <TreeItemContent>
        <RowContent>
          <ChangeRowBody contextTargetRef={rowRef} item={item} name={name} detail={detail} />
        </RowContent>
      </TreeItemContent>
    </TreeItem>
  );
}
