// Where the Git mode's change list puts its rows and headings: the virtualizer's layout and the
// offset of a row, both of which follow from every row and heading being one height.
import { ListLayout, Rect } from "react-aria-components";

import { ROW_HEIGHT } from "../treeRow";
import { changeGroups, sectionKey, visibleNodes, type ChangeItem } from "./changes";
import type { ChangeLayout } from "./changeLayout";
import { HEADING_HEIGHT } from "./changeSectionHeading";

/** The virtualizer's sticky wrapper must stay within its own section, whose overflow is visible.
 * Render copies leave the layout's original positions intact for keyboard navigation and row offsets. */
export class ChangeListLayout extends ListLayout<unknown> {
  /**
   * Lays out every row and heading at once, which their fixed heights make exact and cheap, where
   * `ListLayout` (react-stately 3.50) lays out only what lies around the visible rectangle and
   * estimates a section's rest. Inside a `Tree` that estimate counts the content node of every
   * expanded row still to come (a commit, a directory) as one more row, and the virtualizer sizes the
   * scroller from it, keeping that size while scrolling lays the rest out; scrolled into the excess,
   * the section has already ended, and its heading, which sticks only within it, is gone with its
   * rows. And a section laid out again after the scroller resizes skips every child above the
   * visible rectangle, its own heading among them, so a section scrolled past its top loses its
   * heading. Starting from the top of the list every time leaves nothing to estimate or skip.
   */
  protected override buildCollection(offset?: number): ReturnType<ListLayout<unknown>["buildCollection"]> {
    this.requestedRect = new Rect(0, 0, Infinity, Infinity);
    const nodes = super.buildCollection(offset);
    this.requestedRect = new Rect(0, 0, this.contentSize.width, this.contentSize.height);
    return nodes;
  }

  override getVisibleLayoutInfos(rect: Parameters<ListLayout<unknown>["getVisibleLayoutInfos"]>[0]): ReturnType<ListLayout<unknown>["getVisibleLayoutInfos"]> {
    return super.getVisibleLayoutInfos(rect).map((info) => {
      if (info.type !== "section" && info.type !== "header") return info;
      const rendered = info.copy();
      if (info.type === "section") rendered.allowOverflow = true;
      else {
        rendered.isSticky = true;
        rendered.rect.y = 0;
        rendered.zIndex = 3;
      }
      return rendered;
    });
  }
}

/** Virtualizer section wrappers otherwise establish a stacking context through inline layout
 * containment, trapping their sticky headings below the scroller's overlay fades. Size and style
 * containment remain; the explicit section rectangle still bounds the heading's sticky travel. */
export const CHANGE_SCROLL_CLASS = "[&_[role=presentation]:has(>[role=rowgroup])]:[contain:size_style]! [&_[role=presentation]:has(>[role=rowgroup])]:z-auto!";

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
