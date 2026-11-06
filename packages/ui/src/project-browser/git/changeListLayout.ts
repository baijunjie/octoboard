// Where the Git mode's change list puts a row: its offset, which follows from every row and heading
// being one height, as the tree's sectioned layout lays them out.
import { ROW_HEIGHT } from "../treeRow";
import { changeGroups, sectionKey, visibleNodes, type ChangeItem } from "./changes";
import type { ChangeLayout } from "./changeLayout";
import { HEADING_HEIGHT } from "./changeSectionHeading";

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
