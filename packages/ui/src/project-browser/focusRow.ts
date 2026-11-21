import { afterCollectionFocus } from "../components/focus/afterCollectionFocus";
import { itemKeyOf, itemKeySelector, scrollTreeAndRender } from "../primitives";

/** The row `key` names in `list`, found again by its key since the collection may have rebuilt it. */
function findRow(list: HTMLElement | null, key: string): HTMLElement | null {
  return list?.querySelector<HTMLElement>(itemKeySelector(key)) ?? null;
}

/** The key of the row of `list` that holds keyboard focus, itself or through a control inside it;
 * none while focus is anywhere else. */
export function focusedRowKey(list: HTMLElement | null): string | undefined {
  const focused = document.activeElement instanceof Element ? document.activeElement.closest<HTMLElement>(`[role=row]${itemKeySelector()}`) : null;
  return focused && list?.contains(focused) ? itemKeyOf(focused) : undefined;
}

/** Puts keyboard focus on the row `key` names in `list` once a viewer opened from it has closed:
 * after the dialog's own focus restore, which puts focus back on the row it opened from, and only
 * while focus is nowhere else the user put it. */
export function focusRow(list: HTMLElement | null, key: string): void {
  afterCollectionFocus(() => {
    const row = findRow(list, key);
    if (row && (document.activeElement === document.body || list?.contains(document.activeElement))) row.focus();
  });
}

/** Moves keyboard focus to the row `key` names in `list` ahead of a fold that takes away the row
 * holding it, and keeps that row in view across the fold. Call it in the handler, before the state
 * change: react-aria's focused-key reset re-aims focus at the nearest surviving row two renders
 * later and undoes a handoff made after it. Delaying the move instead is only for a destination
 * that is not a row; a row's menu returns focus from `ActionMenu`'s `onClosed` (`focusRowControl`).
 *
 * Focus moves at once, so the collection and the region's focus handoff both see it on a row that
 * stays: in WebKit a removed element gives focus to `<body>` without a blur, and the handoff would
 * take it out of the list before any later move. A virtualized list may not have rendered the row,
 * scrolled away from it; `scrollIntoView(false)` then scrolls it into view as the rows stand before
 * the fold, and the virtualizer renders it at once (`scrollTreeAndRender`). Where the row is
 * still not there, it takes focus once the collection has settled (`focusRow`). Rows above it may
 * fold too, so once the collection has settled `scrollIntoView(true)` brings it into view again as
 * they stand after the fold, while it still holds focus. `scrollIntoView` works from the rows'
 * places rather than the DOM, which holds only the rendered rows. */
export function focusRowBeforeFold(list: HTMLElement | null, key: string, scrollIntoView?: (afterFold: boolean) => void): void {
  let row = findRow(list, key);
  if (!row && list && scrollIntoView) {
    scrollTreeAndRender(list, () => scrollIntoView(false));
    row = findRow(list, key);
  }
  if (row) row.focus();
  else focusRow(list, key);
  if (scrollIntoView) {
    afterCollectionFocus(() => {
      const shown = findRow(list, key);
      if (shown && shown === document.activeElement) scrollIntoView(true);
    });
  }
}

/** Puts keyboard focus back on `control`, a control in the row `key` names in `list`, once a popup
 * opened from it has closed and the collection has moved focus to the row: only while that row is
 * still in `list` and still holds `control`, and while focus is on the page or inside that row, so
 * focus the user or a dialog has put elsewhere stays there. */
export function focusRowControl(list: HTMLElement | null, key: string, control: HTMLElement): void {
  afterCollectionFocus(() => {
    const row = findRow(list, key);
    const active = document.activeElement;
    if (row?.contains(control) && (active === document.body || row.contains(active))) control.focus();
  });
}

/** Puts keyboard focus on the first row of `list`, or on the first button of `fallback` when the
 * list has no row, once a viewer closed for want of its subject has given focus back. */
export function focusFirst(list: HTMLElement | null, fallback: HTMLElement | null): void {
  afterCollectionFocus(() => {
    if (document.activeElement !== document.body && !list?.contains(document.activeElement)) return;
    (list?.querySelector<HTMLElement>(`[role=row]${itemKeySelector()}`) ?? fallback?.querySelector<HTMLElement>("button"))?.focus();
  });
}
