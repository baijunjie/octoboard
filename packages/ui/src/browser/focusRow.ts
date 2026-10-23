import { afterCollectionFocus } from "../components/afterCollectionFocus";

/** The row `key` names in `list`, found again by its key since the collection may have rebuilt it. */
function findRow(list: HTMLElement | null, key: string): HTMLElement | null {
  return list?.querySelector<HTMLElement>(`[data-key="${CSS.escape(key)}"]`) ?? null;
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
    (list?.querySelector<HTMLElement>("[role=row][data-key]") ?? fallback?.querySelector<HTMLElement>("button"))?.focus();
  });
}
