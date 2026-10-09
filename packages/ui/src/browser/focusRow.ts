/** Puts keyboard focus on the row `key` names in `list` once a viewer opened from it has closed:
 * after the dialog's own focus restore, which puts focus back on the row it opened from, and only
 * while focus is nowhere else the user put it. */
export function focusRow(list: HTMLElement | null, key: string): void {
  requestAnimationFrame(() =>
    requestAnimationFrame(() => {
      const row = list?.querySelector<HTMLElement>(`[data-key="${CSS.escape(key)}"]`);
      if (row && (document.activeElement === document.body || list?.contains(document.activeElement))) row.focus();
    }),
  );
}

/** Puts keyboard focus on the first row of `list`, or on the first button of `fallback` when the
 * list has no row, once a viewer closed for want of its subject has given focus back. */
export function focusFirst(list: HTMLElement | null, fallback: HTMLElement | null): void {
  requestAnimationFrame(() =>
    requestAnimationFrame(() => {
      if (document.activeElement !== document.body && !list?.contains(document.activeElement)) return;
      (list?.querySelector<HTMLElement>("[role=option]") ?? fallback?.querySelector<HTMLElement>("button"))?.focus();
    }),
  );
}
