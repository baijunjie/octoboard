// Where the Git mode's change list puts a row: its offset, which follows from every row and heading
// being one height, as the tree's sectioned layout lays them out.
import { ROW_HEIGHT, scrollRowIntoView } from "../treeRow";
import { changeGroups, sectionKey, visibleNodes, type ChangeItem, type ChangeSection } from "./changes";
import type { ChangeLayout } from "./changeLayout";
import { HEADING_HEIGHT } from "./changeSectionHeading";

/** How far below the top of the list's content the row of `key` is, or nothing when the list has
 * no such row on screen (a change in a folded-away section or under a collapsed commit or
 * directory has none). Rows and headings are each one height and laid out one after another, so
 * where a row is follows from what comes before it — a folded section contributing its heading
 * alone, as does a section in `kept` that the list shows with no rows (`changeGroups`). */
export function changeRowOffset(
  items: readonly ChangeItem[],
  key: string,
  layout: ChangeLayout,
  collapsed: ReadonlySet<string>,
  kept: Iterable<ChangeSection> = [],
): number | undefined {
  let top = 0;
  for (const group of changeGroups(items, layout, kept)) {
    top += HEADING_HEIGHT;
    if (collapsed.has(sectionKey(group.section))) continue;
    const rows = visibleNodes(group.nodes, collapsed);
    const index = rows.findIndex((node) => node.key === key);
    if (index !== -1) return top + index * ROW_HEIGHT;
    top += rows.length * ROW_HEIGHT;
  }
  return undefined;
}

/** Scrolls `list` the least that brings the row of `key` among `items` (a change's, a commit's or a
 * directory's) into view, as the rows stand with `collapsed` folded away, whether or not the
 * virtualizer has rendered it; false when there is no such row on screen. Rows and headings are
 * each one height, so where a row is follows from its place (`changeRowOffset`). */
export function scrollToChange(
  list: HTMLElement | null,
  items: readonly ChangeItem[],
  key: string | undefined,
  layout: ChangeLayout,
  collapsed: ReadonlySet<string>,
  kept: Iterable<ChangeSection> = [],
): boolean {
  if (key === undefined || !list) return false;
  const top = changeRowOffset(items, key, layout, collapsed, kept);
  if (top === undefined) return false;
  scrollRowIntoView(list, top, { stickyTop: HEADING_HEIGHT });
  return true;
}
