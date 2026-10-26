import { SearchX } from "lucide-react";
import React, { useMemo } from "react";

import { CenteredEmptyPanel } from "../components/EmptyPanel";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { useT } from "../i18n/react";
import type { ChangeLayout } from "./changeLayout";
import type { ChangeViewState } from "./changeView";
import { changeGroups, filterChanges, sectionKey, visibleNodes, type ChangeItem, type ChangeSection } from "./changes";
import { HEADING_HEIGHT } from "./changeSectionHeading";
import { ChangeTree } from "./ChangeTree";
import { ROW_HEIGHT } from "./treeRow";

/** How far below the top of the list's content the row of `key` is, or nothing when the list has
 * no such row on screen (a change in a folded-away section or under a collapsed commit or
 * directory has none). Rows and headings are each one height and laid out one after another, so
 * where a row is follows from what comes before it — a folded section contributing its heading
 * alone. */
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
 * A branch's changes (`ChangeTree`): the sections, each headed with its count, and a row per change,
 * grouped under directories when the layout is the tree, and in the committed section, when it is
 * listed by commit, under a row per commit. The view's filter field above it (`FileFilterField`)
 * narrows its rows to the changes whose file name contains the text (`filterChanges`); the
 * sections' and the commits' counts follow the rows left, a commit with none left has no row, and
 * the list says so where none are.
 *
 * Every section's heading folds the section away and unfolds it again (`ChangeSectionHeading`,
 * keyed in `collapsed` by `sectionKey`), and so does every commit's row (`commitKey`). A folded
 * section is its heading alone, still counting the changes it holds, and those changes have no
 * row, so the viewer's navigation moves past them (`visibleChanges`).
 */
export function ChangeList({
  items,
  selected,
  label,
  onOpen,
  listRef,
  changeView,
  headingMarks,
}: {
  items: ChangeItem[];
  /** The key of the change the viewer showed last. */
  selected: string | undefined;
  label: string;
  onOpen: (item: ChangeItem) => void;
  listRef: React.Ref<HTMLDivElement>;
  changeView: ChangeViewState;
  /** `ChangeTree`'s. */
  headingMarks?: ReadonlyMap<ChangeSection, React.ReactNode>;
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
      ) : (
        <ChangeTree
          items={shown}
          selected={selected}
          label={label}
          onOpen={onOpen}
          listRef={listRef}
          layout={changeView.layout}
          collapsed={changeView.collapsed}
          onCollapsedChange={changeView.onCollapsedChange}
          headingMarks={headingMarks}
        />
      )}
    </>
  );
}
