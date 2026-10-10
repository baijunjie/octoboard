import { Header, ListBox, SearchField } from "@heroui/react";
import { SearchX } from "lucide-react";
import React, { useMemo } from "react";
import { Collection, ListLayout, Virtualizer, type Key } from "react-aria-components";

import { EmptyPanel } from "../components/EmptyPanel";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { useScrollFade } from "../components/useScrollFade";
import { useCurrentLanguage, useT } from "../i18n/react";
import { isImeKey } from "../imeKey";
import type { ChangeLayout } from "./changeLayout";
import { ChangeRowBody, changeRowText, HEADING_CLASS, HEADING_HEIGHT, SectionHeadingText } from "./changeRow";
import { changeGroups, filterChanges, sections, visibleNodes, type ChangeItem, type ChangeSection } from "./changes";
import { ChangeTree } from "./ChangeTree";
import { ROW_HEIGHT } from "./treeRow";

/** How far below the top of the list's content the row of `key` is, or nothing when the list has
 * no such row on screen (a change under a collapsed directory has none). Rows and headings are each
 * one height and laid out one after another below the list's top padding (`py-1`), so where a row
 * is follows from what comes before it. */
export function changeRowOffset(items: readonly ChangeItem[], key: string, layout: ChangeLayout, collapsed: ReadonlySet<string>): number | undefined {
  let top = 4;
  for (const group of changeGroups(items, layout)) {
    top += HEADING_HEIGHT;
    const rows = visibleNodes(group.nodes, collapsed);
    const index = rows.findIndex((node) => node.key === key);
    if (index !== -1) return top + index * ROW_HEIGHT;
    top += rows.length * ROW_HEIGHT;
  }
  return undefined;
}

/** How the list shows its changes and what it remembers of that: the layout, in the tree the
 * directories folded away (their row keys, `directoryKey`), and the file name text narrowing the
 * rows (`filter`, empty for none). */
export interface ChangeListView {
  layout: ChangeLayout;
  collapsed: ReadonlySet<string>;
  onCollapsedChange: (collapsed: ReadonlySet<string>) => void;
  filter: string;
  onFilterChange: (filter: string) => void;
}

/**
 * A worktree's changes as a list in sections — staged, in conflict, unstaged, untracked — each
 * headed with its count: HeroUI's `ListBox`, virtualized, one row per change; or, when the layout
 * is the tree, the same sections with their changes grouped under directories (`ChangeTree`). A
 * row's action is to open its change in the viewer; nothing is selectable in the list's sense. The
 * change the viewer showed last is tinted and named as selected in its row's label instead, as the
 * file tree marks its file. A field above the list narrows its rows to the changes whose file name
 * contains the text (`filterChanges`); the sections' counts follow the rows left, and the field
 * stays when none are.
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
  changeView: ChangeListView;
}): React.ReactElement {
  const t = useT();
  const shown = useMemo(() => filterChanges(items, changeView.filter), [items, changeView.filter]);
  const noMatch = shown.length === 0 && changeView.filter.trim() !== "" ? t("git.filter.noMatch") : undefined;
  // The field is a sibling of the list, not inside it, so what is typed in it never reaches the
  // list's type-ahead or arrow keys. An input method's keys are stopped here before the field's
  // own handler and before whatever handles keys above it in the React tree: the composition owns
  // them. react-aria's `useKeyboard` ignores only `isComposing`, so a key an engine reports with
  // `keyCode` 229 alone is held back by this guard alone, or its Escape would clear the field. A
  // listener on the window or the document, such as the pane's Escape (which leaves editable
  // targets alone for this), runs before React's and is beyond this guard's reach.
  const guardComposition = (event: React.KeyboardEvent) => {
    if (isImeKey(event.nativeEvent)) event.stopPropagation();
  };
  return (
    <>
      <div className="shrink-0 border-b border-separator px-3 py-2" onKeyDownCapture={guardComposition}>
        <SearchField aria-label={t("git.filter.label")} value={changeView.filter} onChange={changeView.onFilterChange} fullWidth variant="secondary">
          <SearchField.Group>
            <SearchField.SearchIcon />
            <SearchField.Input placeholder={t("git.filter.placeholder")} />
            <SearchField.ClearButton />
          </SearchField.Group>
        </SearchField>
      </div>
      {/* The visible line has no role of its own, so the announcer says it once. */}
      <StatusAnnouncer text={noMatch} />
      {noMatch ? (
        <EmptyPanel icon={SearchX} message={noMatch} />
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
        <FlatChangeList items={shown} selected={selected} label={label} onOpen={onOpen} listRef={listRef} />
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
}: {
  items: ChangeItem[];
  selected: string | undefined;
  label: string;
  onOpen: (item: ChangeItem) => void;
  listRef: React.Ref<HTMLDivElement>;
}): React.ReactElement {
  const language = useCurrentLanguage();
  const fade = useScrollFade(listRef);
  // A section's `id` is what the collection keys it by; a change's own `key` keys its row.
  const groups = useMemo(() => sections(items).map((group) => ({ id: group.section, ...group })), [items]);
  const byKey = useMemo(() => new Map(items.map((item) => [item.key, item])), [items]);
  const onAction = (key: Key) => {
    const item = byKey.get(String(key));
    if (item) onOpen(item);
  };
  return (
    // No side padding on the scroll container (`px-0` clears HeroUI's own): the virtualized layout
    // makes each row as wide as the list's inner width and places it inside that padding, so the
    // list would overflow sideways. The rows and headings carry the inset themselves.
    <Virtualizer layout={ListLayout} layoutOptions={{ rowHeight: ROW_HEIGHT, headingHeight: HEADING_HEIGHT }}>
      <ListBox
        ref={fade.ref}
        {...fade.props}
        aria-label={label}
        items={groups}
        onAction={onAction}
        dependencies={[language, selected]}
        className={`${fade.className} min-h-0 flex-1 overflow-auto px-0 py-1 outline-none`}
      >
        {(group) => <ChangeSectionView key={group.section} section={group.section} items={group.items} selected={selected} />}
      </ListBox>
    </Virtualizer>
  );
}

function ChangeSectionView({
  section,
  items,
  selected,
}: {
  section: ChangeSection;
  items: ChangeItem[];
  selected: string | undefined;
}): React.ReactElement {
  const language = useCurrentLanguage();
  return (
    <ListBox.Section id={section}>
      <Header className={HEADING_CLASS}>
        <SectionHeadingText section={section} count={items.length} />
      </Header>
      <Collection items={items} dependencies={[language, selected]}>
        {(item) => <ChangeRow item={item} isSelected={item.key === selected} />}
      </Collection>
    </ListBox.Section>
  );
}

function ChangeRow({ item, isSelected }: { item: ChangeItem; isSelected: boolean }): React.ReactElement {
  const t = useT();
  const { name, detail, ariaLabel } = changeRowText(t, item, isSelected, false);
  // Marked `data-current`, not `data-selected`: react-aria-components writes the row's
  // `data-selected` itself, from a selection the list does not have.
  return (
    <ListBox.Item
      id={item.key}
      textValue={name}
      aria-label={ariaLabel}
      data-current={isSelected || undefined}
      className="mx-1 h-7 min-h-0 w-auto gap-2 rounded-md px-2 py-0 text-sm data-[current]:bg-panel-selected"
    >
      <ChangeRowBody item={item} name={name} detail={detail} />
    </ListBox.Item>
  );
}
