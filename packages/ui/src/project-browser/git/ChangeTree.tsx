import React, { useMemo, useRef } from "react";

import { FadeOverflow } from "../../components/text/FadeOverflow";
import { useCollectionOptionTab } from "../../components/focus/collectionOptionTab";
import { useStickyScrollFade } from "../../components/scroll/scrollFade";
import { isolate } from "../../i18n/isolate";
import { useCurrentLanguage, useT } from "../../i18n/react";
import type { ChangeLayout } from "./changeLayout";
import { ChangeRowBody, changeRowText } from "./changeRow";
import { changeGroups, sectionKey, type ChangeItem, type ChangeNode, type ChangeSection, type CommitRef } from "./changes";
import { collectionIndex, emptyCollectionIndex } from "../collectionIndex";
import { ChangeSectionHeading, ChangeSectionHeadingStops, HEADING_CLASS, HEADING_HEIGHT } from "./changeSectionHeading";
import { RowActions } from "../RowActions";
import { CommitIdLabel } from "./CommitIdLabel";
import { shortCommit } from "./shortCommit";
import { BrowserListMotion, useBrowserListMotion } from "../useBrowserListMotion";
import { DirectoryRow, ExpandChevron, ROW_CLASS, ROW_HEIGHT, RowContent } from "../treeRow";
import { Tree, TreeCollection, TreeHeader, TreeItem, TreeSection } from "../../primitives";

/** The rows of `nodes` that open and close — the directories and the commits — and everything
 * under them. */
function foldableKeys(nodes: readonly ChangeNode[]): string[] {
  return nodes.flatMap((node) => (node.type !== "change" ? [node.key, ...foldableKeys(node.children)] : []));
}

/**
 * A branch's changes as a list in sections — in conflict, staged, unstaged, untracked, then what it
 * has committed, as one flat run of changes or, when it is listed by commit, a row per commit
 * (newest first) holding that commit's changes — each section headed with its count. It is the
 * wrapper layer's virtualized `Tree`, as the Files mode's tree is (`FileTree`), so it has the same keyboard model: arrows,
 * Home / End, type-ahead, Left / Right to collapse and expand a row that has rows under it. In the
 * `tree` layout a section's changes (or a commit's) are grouped under their directories, in the
 * `flat` one they are one row each. A directory or a commit opens or closes and a change opens in
 * the viewer. A row's action is to open its change, and a button at its end opens a menu of the
 * others (`RowActions`); nothing is selectable in the list's sense. The change the viewer showed
 * last is tinted and named as selected in its row's label instead, as the file tree marks its file.
 *
 * `collapsed` holds the sections the user folded away (`sectionKey`), which each section's heading
 * does (`ChangeSectionHeading`), and the commits and directories folded away (`commitKey`,
 * `directoryKey`). A folded section renders none of its rows, so a commit or a directory inside one
 * keeps the state it had, this tree deciding only the rows it shows; a change under a folded row
 * has no row, so the viewer's navigation skips it (`visibleChanges`). Everything starts unfolded.
 */
export function ChangeTree({
  items,
  selected,
  label,
  onOpen,
  listRef,
  layout,
  collapsed,
  onCollapsedChange,
  headingMarks,
  byCommit,
}: {
  items: ChangeItem[];
  /** The key of the change the viewer showed last. */
  selected: string | undefined;
  label: string;
  onOpen: (item: ChangeItem) => void;
  listRef: React.Ref<HTMLDivElement>;
  layout: ChangeLayout;
  /** Row keys of the sections, the commits and the directories folded away. */
  collapsed: ReadonlySet<string>;
  onCollapsedChange: (collapsed: ReadonlySet<string>) => void;
  /** The mark of a section's failure to be listed, or of its list being cut short, which that section's
   * heading carries; a section with none of its rows keeps its heading for it. */
  headingMarks?: ReadonlyMap<ChangeSection, React.ReactNode>;
  byCommit?: { value: boolean; onChange: (byCommit: boolean) => void };
}): React.ReactElement {
  const language = useCurrentLanguage();
  const groups = useMemo(() => changeGroups(items, layout, [...(headingMarks?.keys() ?? []), ...(byCommit ? ["committed" as const] : [])]).map((group, index, all) => ({ id: group.section, previousSection: all[index - 1]?.section, ...group })), [items, layout, headingMarks, byCommit]);
  // Only the commits and directories of the sections on screen: a folded section renders none of
  // its rows, so whether a row inside one is folded is not this list's to decide. react-aria hands
  // an expanded key it knows nothing of back unchanged, so those keys would survive either way;
  // leaving them out of what is decided here is what makes that beside the point.
  const shown = useMemo(() => groups.filter((group) => !collapsed.has(sectionKey(group.section))), [groups, collapsed]);
  const foldable = useMemo(() => shown.flatMap((group) => foldableKeys(group.nodes)), [shown]);
  const expanded = useMemo(() => new Set(foldable.filter((key) => !collapsed.has(key))), [foldable, collapsed]);

  const { byKey, visibleKeys, folds } = useMemo(() => {
    const index = emptyCollectionIndex<ChangeNode>();
    for (const group of groups) {
      const heading = `heading:${group.section}`;
      index.visibleKeys.add(heading);
      if (collapsed.has(sectionKey(group.section))) continue;
      index.folds.open.add(heading);
      collectionIndex(group.nodes, (node) => (node.type !== "change" ? node.children : []), (node) => node.type !== "change" && expanded.has(node.key), { parent: heading, into: index });
    }
    return index;
  }, [groups, expanded, collapsed]);
  const motion = useBrowserListMotion(listRef, visibleKeys, folds);
  const collectionRef = useCollectionOptionTab(motion.ref);
  const { props: fade, wrap } = useStickyScrollFade({ ref: collectionRef, className: "min-h-0 flex-1 overflow-auto pb-1 outline-none", stickyHeadingHeight: HEADING_HEIGHT });

  // Only the commits and directories this list shows are decided here: the sections' own keys and
  // the rows inside a folded section all stay as they are.
  const onExpandedChange = (keys: Set<string>) => {
    const next = new Set([...collapsed].filter((key) => !foldable.includes(key)));
    for (const key of foldable) if (!keys.has(key)) next.add(key);
    onCollapsedChange(next);
  };
  const onAction = (key: string) => {
    const node = byKey.get(key);
    if (node?.type === "change") onOpen(node.item);
    else if (node) onExpandedChange(expanded.has(key) ? new Set([...expanded].filter((k) => k !== key)) : new Set([...expanded, key]));
  };

  const renderNode = (node: ChangeNode): React.ReactElement => {
    if (node.type === "change") return <ChangeTreeRow item={node.item} isSelected={node.item.key === selected} layout={layout} />;
    const children = (
      <TreeCollection items={node.children} dependencies={[language, selected, layout]}>
        {renderNode}
      </TreeCollection>
    );
    if (node.type === "directory") return <ChangeDirectoryRow node={node}>{children}</ChangeDirectoryRow>;
    return (
      <TreeItem
        data-list-motion={node.key}
        id={node.key}
        textValue={commitText(node.commit)}
        label={commitLabel(node.commit, node.count, language)}
        expandable
        className={ROW_CLASS}
        content={({ expanded }) => <CommitRow commit={node.commit} count={node.count} isExpanded={expanded} />}
      >
        {children}
      </TreeItem>
    );
  };

  return (
    <BrowserListMotion keys={visibleKeys} folds={folds} prepare={motion.prepare}>
      <ChangeSectionHeadingStops>
        {wrap(
          <Tree
            {...fade}
            label={label}
            items={groups}
            rowHeight={ROW_HEIGHT}
            headingHeight={HEADING_HEIGHT}
            expandedKeys={expanded}
            onExpandedChange={onExpandedChange}
            onAction={onAction}
            // `collapsed` among the dependencies, or the collection keeps the rows of a section that
            // has just been folded: nothing else about it changed.
            dependencies={[language, selected, collapsed, layout, headingMarks, byCommit]}
          >
            {(group) => (
              <TreeSection className="h-full" id={group.section}>
                <TreeHeader data-list-motion={`heading:${group.section}`} className={HEADING_CLASS}>
                  <ChangeSectionHeading
                    section={group.section}
                    count={group.items.length}
                    mark={headingMarks?.get(group.section)}
                    byCommit={group.section === "committed" ? byCommit : undefined}
                    commitKeys={group.nodes.filter((node) => node.type === "commit").map((node) => node.key)}
                    collapsed={collapsed}
                    onCollapsedChange={onCollapsedChange}
                    topDivider={group.previousSection !== undefined && !collapsed.has(sectionKey(group.previousSection))}
                  />
                </TreeHeader>
                {/* A folded section is its heading and nothing else, still counting what it holds. */}
                <TreeCollection items={collapsed.has(sectionKey(group.section)) ? [] : group.nodes} dependencies={[language, selected, layout]}>
                  {renderNode}
                </TreeCollection>
              </TreeSection>
            )}
          </Tree>,
        )}
      </ChangeSectionHeadingStops>
    </BrowserListMotion>
  );
}

/** What a commit's row is searched by, for type-ahead: its short id and its subject. */
const commitText = (commit: CommitRef): string => `${shortCommit(commit.commit)} ${commit.subject}`.trim();

/** What a commit's row is named, for assistive technology, in the order it is drawn: its short id,
 * its subject (isolated, as free text) and its count of changes, which the visible row shows too. */
const commitLabel = (commit: CommitRef, count: number, language: string): string =>
  [shortCommit(commit.commit), commit.subject !== "" ? isolate(commit.subject) : "", new Intl.NumberFormat(language).format(count)].filter(Boolean).join(" ");

/**
 * A commit's row inside the committed section, subordinate to the section's band (no fill, and the
 * band's own smaller type) yet the parent of the folders and changes beneath it, which it must not
 * read as lighter than: the rows' text size, a chevron like a directory's, and the commit's short
 * id in muted monospace text, then its subject in the normal foreground at medium weight, which
 * fades out where it is too long with its whole text as its tooltip, and the muted count of its
 * changes, which stays in view after it. It sits in the flow
 * rather than pinned under the band, as a directory's row does; its row is named by `commitLabel`,
 * the same three in the same order.
 */
function CommitRow({ commit, count, isExpanded }: { commit: CommitRef; count: number; isExpanded: boolean }): React.ReactElement {
  const language = useCurrentLanguage();
  return (
    <RowContent chevron={<ExpandChevron isExpanded={isExpanded} />}>
      <CommitIdLabel commit={commit.commit} />
      {commit.subject !== "" ? (
        <FadeOverflow as="span" dir="auto" className="min-w-0 flex-1 font-medium" titleWhenClipped={commit.subject}>
          {commit.subject}
        </FadeOverflow>
      ) : (
        <span className="flex-1" />
      )}
      <span className="shrink-0 text-app-muted">{new Intl.NumberFormat(language).format(count)}</span>
    </RowContent>
  );
}

/** A directory's row in the tree layout: the folder the changes under it are grouped by, with the
 * button of the row's actions at its end, which keeps its room while hidden as a change's does. */
function ChangeDirectoryRow({ node, children }: { node: Extract<ChangeNode, { type: "directory" }>; children: React.ReactNode }): React.ReactElement {
  const rowRef = useRef<HTMLDivElement>(null);
  return (
    <TreeItem
      data-list-motion={node.key}
      id={node.key}
      textValue={node.name}
      expandable
      className={ROW_CLASS}
      ref={rowRef}
      content={({ expanded }) => (
        <DirectoryRow name={node.name} isExpanded={expanded}>
          <RowActions reserve contextTargetRef={rowRef} path={node.path} name={node.name} directory />
        </DirectoryRow>
      )}
    >
      {children}
    </TreeItem>
  );
}

function ChangeTreeRow({ item, isSelected, layout }: { item: ChangeItem; isSelected: boolean; layout: ChangeLayout }): React.ReactElement {
  const t = useT();
  const rowRef = useRef<HTMLDivElement>(null);
  const flat = layout === "flat";
  const { name, detail, ariaLabel } = changeRowText(t, item, isSelected, !flat);
  // Marked `data-current`, not `data-selected`: react-aria-components writes the row's
  // `data-selected` itself, from a selection the tree does not have.
  return (
    <TreeItem
      data-list-motion={item.key}
      id={item.key}
      textValue={name}
      label={ariaLabel}
      data-current={isSelected || undefined}
      className={ROW_CLASS}
      ref={rowRef}
      content={
        <RowContent flat={flat}>
          <ChangeRowBody contextTargetRef={rowRef} item={item} name={name} detail={detail} />
        </RowContent>
      }
    />
  );
}
