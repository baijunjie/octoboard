import { SearchX } from "lucide-react";
import React, { useMemo, useRef } from "react";
import { Collection, GridList, GridListHeader, GridListItem, GridListSection, Virtualizer, type Key } from "react-aria-components";

import { CenteredEmptyPanel } from "../components/EmptyPanel";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { useStickyScrollFade } from "../components/scrollFade";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { ChangeLayout } from "./changeLayout";
import type { ChangeViewState } from "./changeView";
import { ChangeRowBody, changeRowText } from "./changeRow";
import { changeGroups, filterChanges, sectionKey, sections, visibleNodes, type ChangeItem, type ChangeSection } from "./changes";
import { ChangeSectionHeading, ChangeSectionHeadingStops, ChangeListLayout, CHANGE_SCROLL_CLASS, HEADING_CLASS, HEADING_HEIGHT } from "./changeSectionHeading";
import { ChangeTree } from "./ChangeTree";
import { useBrowserListMotion } from "./useBrowserListMotion";
import { ROW_CLASS, ROW_HEIGHT, RowContent } from "./treeRow";

/** How far below the top of the list's content the row of `key` is, or nothing when the list has
 * no such row on screen (a change in a folded-away section or under a collapsed directory has
 * none). Rows and headings are each one height and laid out one after another, so where a row is
 * follows from what comes before it — a folded section
 * contributing its heading alone. */
export function changeRowOffset(items: readonly ChangeItem[], key: string, layout: ChangeLayout, collapsed: ReadonlySet<string>): number | undefined {
  let top = 0;
  for (const group of changeGroups(items, layout)) {
    top += HEADING_HEIGHT;
    if (collapsed.has(sectionKey(group.section))) continue;
    const rows = visibleNodes(group.nodes, collapsed);
    const index = rows.findIndex((node) => node.key === key);
    if (index !== -1) return top + index * ROW_HEIGHT;
    top += rows.length * ROW_HEIGHT;
  }
  return undefined;
}


/**
 * A worktree's changes as a list in sections — in conflict, staged, unstaged, untracked — each
 * headed with its count: a react-aria `GridList`, virtualized, one row per change; or, when the
 * layout is the tree, the same sections with their changes grouped under directories
 * (`ChangeTree`). A row's action is to open its change in the viewer, and a button at its end opens
 * a menu of the others (`RowActions`); nothing is selectable in the list's sense. The change
 * the viewer showed last is tinted and named as selected in its row's label instead, as the file
 * tree marks its file. The view's filter field above it (`FileFilterField`) narrows its rows to
 * the changes whose file name contains the text (`filterChanges`); the sections' counts follow the
 * rows left, and the list says so where none are.
 *
 * Every section's heading folds the section away and unfolds it again, in either layout
 * (`ChangeSectionHeading`, keyed in `collapsed` by `sectionKey`). A folded section is its heading alone,
 * still counting the changes it holds, and those changes have no row, so the viewer's navigation
 * moves past them (`visibleChanges`). Everything starts unfolded.
 */
export function ChangeList({
  items,
  selected,
  label,
  onOpen,
  listRef,
  changeView,
}: {
  items: ChangeItem[];
  /** The key of the change the viewer showed last. */
  selected: string | undefined;
  label: string;
  onOpen: (item: ChangeItem) => void;
  listRef: React.Ref<HTMLDivElement>;
  changeView: ChangeViewState;
}): React.ReactElement {
  const t = useT();
  const shown = useMemo(() => filterChanges(items, changeView.filter), [items, changeView.filter]);
  const noMatch = shown.length === 0 && changeView.filter.trim() !== "" ? t("git.filter.noMatch") : undefined;
  return (
    <>
      {/* The visible heading is not a status, so the announcer says it once. */}
      <StatusAnnouncer text={noMatch} />
      {noMatch ? (
        <CenteredEmptyPanel icon={SearchX} message={noMatch} />
      ) : changeView.layout === "tree" ? (
        <ChangeTree
          items={shown}
          selected={selected}
          label={label}
          onOpen={onOpen}
          listRef={listRef}
          collapsed={changeView.collapsed}
          onCollapsedChange={changeView.onCollapsedChange}
        />
      ) : (
        <FlatChangeList
          items={shown}
          selected={selected}
          label={label}
          onOpen={onOpen}
          listRef={listRef}
          collapsed={changeView.collapsed}
          onCollapsedChange={changeView.onCollapsedChange}
        />
      )}
    </>
  );
}

function FlatChangeList({
  items,
  selected,
  label,
  onOpen,
  listRef,
  collapsed,
  onCollapsedChange,
}: {
  items: ChangeItem[];
  selected: string | undefined;
  label: string;
  onOpen: (item: ChangeItem) => void;
  listRef: React.Ref<HTMLDivElement>;
  /** The rows folded away; a flat list has only the sections' own keys to read (`sectionKey`). */
  collapsed: ReadonlySet<string>;
  onCollapsedChange: (collapsed: ReadonlySet<string>) => void;
}): React.ReactElement {
  const language = useCurrentLanguage();
  const motionRef = useBrowserListMotion(listRef);
  const { props: fade, wrap } = useStickyScrollFade({ ref: motionRef, className: "min-h-0 flex-1 overflow-auto pb-1 outline-none " + CHANGE_SCROLL_CLASS, stickyHeadingHeight: HEADING_HEIGHT });
  // A section's `id` is what the collection keys it by; a change's own `key` keys its row.
  const groups = useMemo(() => sections(items).map((group) => ({ id: group.section, ...group })), [items]);
  const byKey = useMemo(() => new Map(items.map((item) => [item.key, item])), [items]);
  const onAction = (key: Key) => {
    const item = byKey.get(String(key));
    if (item) onOpen(item);
  };
  return (
    // No side padding on the scroll container: the virtualized layout makes each row as wide as the
    // list's inner width and places it inside that padding, so the list would overflow sideways. The
    // shared row class and headings carry the inset themselves.
    // Not HeroUI's `ListBox`, which has no place for a control inside an option: a row's action
    // button has to be reached by keyboard, and a `GridList` row (like the tree's) hands focus to its
    // children with the arrow keys that run along the row, HeroUI having no equivalent.
    <ChangeSectionHeadingStops>
      {wrap(<Virtualizer layout={ChangeListLayout} layoutOptions={{ rowHeight: ROW_HEIGHT, headingHeight: HEADING_HEIGHT }}>
        <GridList
          {...fade}
          aria-label={label}
          items={groups}
          onAction={onAction}
          // `collapsed` among the dependencies, or the collection keeps the rows of a section that
          // has just been folded: nothing else about it changed.
          dependencies={[language, selected, collapsed]}
        >
          {(group) => (
            <ChangeSectionView
              key={group.section}
              section={group.section}
              items={group.items}
              selected={selected}
              collapsed={collapsed}
              onCollapsedChange={onCollapsedChange}
            />
          )}
        </GridList>
      </Virtualizer>)}
    </ChangeSectionHeadingStops>
  );
}

function ChangeSectionView({
  section,
  items,
  selected,
  collapsed,
  onCollapsedChange,
}: {
  section: ChangeSection;
  items: ChangeItem[];
  selected: string | undefined;
  collapsed: ReadonlySet<string>;
  onCollapsedChange: (collapsed: ReadonlySet<string>) => void;
}): React.ReactElement {
  const language = useCurrentLanguage();
  // A folded section is its heading and nothing else, the heading counting the changes the section
  // holds either way.
  const rows = collapsed.has(sectionKey(section)) ? [] : items;
  return (
    <GridListSection className="h-full" id={section}>
      <GridListHeader data-list-motion={`heading:${section}`} className={HEADING_CLASS}>
        <ChangeSectionHeading section={section} count={items.length} collapsed={collapsed} onCollapsedChange={onCollapsedChange} />
      </GridListHeader>
      <Collection items={rows} dependencies={[language, selected]}>
        {(item) => <ChangeRow item={item} isSelected={item.key === selected} />}
      </Collection>
    </GridListSection>
  );
}

function ChangeRow({ item, isSelected }: { item: ChangeItem; isSelected: boolean }): React.ReactElement {
  const t = useT();
  const rowRef = useRef<HTMLDivElement>(null);
  const { name, detail, ariaLabel } = changeRowText(t, item, isSelected, false);
  // Marked `data-current`, not `data-selected`: react-aria-components writes the row's
  // `data-selected` itself, from a selection the list does not have.
  return (
    <GridListItem data-list-motion={item.key} id={item.key} textValue={name} aria-label={ariaLabel} data-current={isSelected || undefined} className={ROW_CLASS} ref={rowRef}>
      <RowContent flat>
        <ChangeRowBody contextTargetRef={rowRef} item={item} name={name} detail={detail} />
      </RowContent>
    </GridListItem>
  );
}
